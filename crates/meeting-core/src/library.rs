//! Rebuildable local SQLite/FTS index. Original session files remain authoritative.
use crate::{metadata, storage::exists_no_link, Result, SessionData};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, time::Duration};

/// Version 3 adds the meeting title as `context` to every section row (issue #22).
const SCHEMA_VERSION: i64 = 3;
/// Row layout: one meeting row (`source_id` empty, `title` set, `body` and `context` empty) and
/// one row per transcript section (`title` empty, `body` = section text, `context` = title).
const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS meetings (
    id TEXT PRIMARY KEY, title TEXT NOT NULL, started TEXT NOT NULL, duration INTEGER NOT NULL,
    status TEXT NOT NULL, model TEXT NOT NULL, pinned INTEGER NOT NULL, audio_removed INTEGER NOT NULL,
    revision INTEGER NOT NULL, fingerprint TEXT NOT NULL);
  CREATE INDEX IF NOT EXISTS meetings_time ON meetings(pinned DESC,started DESC,id);
  CREATE VIRTUAL TABLE IF NOT EXISTS transcript_search USING fts5(
    meeting_id UNINDEXED, source_id UNINDEXED, start_ms UNINDEXED, end_ms UNINDEXED,
    title, body, context, tokenize='unicode61 remove_diacritics 2');
  CREATE TABLE IF NOT EXISTS search_rows(row_id INTEGER PRIMARY KEY, meeting_id TEXT NOT NULL);
  CREATE INDEX IF NOT EXISTS search_rows_meeting ON search_rows(meeting_id);";
// A compatible, content-free cache identity, not a meeting or schema version.
// Recreated SQLite caches get a new identity even if a different app process
// still remembers a previously completed scan.
const IDENTITY_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS index_identity (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1), generation TEXT NOT NULL);";

pub struct LibraryIndex {
    db: Connection,
    // Declared after the connection: close SQLite before releasing cross-process admission.
    _lease: std::fs::File,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub status: String,
    pub model_id: String,
    pub pinned: bool,
    pub audio_removed: bool,
    pub metadata_revision: u64,
    pub source_id: Option<String>,
    pub start_ms: Option<u64>,
    pub end_ms: Option<u64>,
    pub excerpt: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub items: Vec<SearchHit>,
    pub total: u64,
    pub offset: u64,
    pub limit: u32,
}
impl SearchHit {
    pub fn session_summary(&self) -> Result<SessionData> {
        Ok(serde_json::from_value(
            serde_json::json!({"schemaVersion":2,"id":self.id,"title":self.title,
          "startedAt":self.started_at,"durationMs":self.duration_ms,"status":self.status,"modelId":self.model_id,
          "parts":[],"warnings":[],"readOnly":self.audio_removed}),
        )?)
    }
}

impl LibraryIndex {
    /// Opens the index for a user-requested rebuild. A database SQLite cannot read, or one
    /// that fails its integrity checks, is moved aside, never deleted, and a fresh index is created in its place. Returns the
    /// folder that now holds the unreadable files.
    pub fn open_for_rebuild(root: &Path) -> Result<(Self, Option<std::path::PathBuf>)> {
        match Self::open(root) {
            // A readable header can still hide damaged tables or FTS postings, and
            // unchanged sessions are skipped by the scan, so check before reuse.
            // The damaged connection is closed before its files are moved.
            Ok(index) => {
                if !index.damaged()? {
                    return Ok((index, None));
                }
            }
            Err(error) if unreadable_database(error.as_ref()) => (),
            Err(error) => return Err(error),
        }
        let lease = erasure_index_lock(root)?;
        let aside = set_aside_unreadable(root)?;
        drop(lease);
        Ok((Self::open(root)?, Some(aside)))
    }
    /// SQLite's quick check plus the FTS5 integrity check. Only corruption reports `true`.
    fn damaged(&self) -> Result<bool> {
        let quick: String = match self.db.query_row("PRAGMA quick_check", [], |r| r.get(0)) {
            Ok(value) => value,
            Err(error) if unreadable_database(&error) => return Ok(true),
            Err(error) => return Err(error.into()),
        };
        if quick != "ok" {
            return Ok(true);
        }
        if !valid_cache_identity(&self.stored_cache_identity()?) {
            return Ok(true);
        }
        match self.db.execute(
            "INSERT INTO transcript_search(transcript_search) VALUES('integrity-check')",
            [],
        ) {
            Ok(_) => Ok(false),
            Err(error) if unreadable_database(&error) => Ok(true),
            Err(error) => Err(error.into()),
        }
    }
    /// Opens the derived cache while holding shared process admission until SQLite closes.
    pub fn open(root: &Path) -> Result<Self> {
        metadata::checked_directory(root)?;
        let lease = index_lease(root, false)?;
        let folder = root.join("library-index");
        match fs::create_dir(&folder) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
        metadata::checked_directory(&folder)?;
        let path = folder.join("library.sqlite3");
        for name in INDEX_FILES {
            // SQLite may create missing files, but must never follow a dangling
            // database/WAL/SHM link that Path::exists would report as absent.
            exists_no_link(&folder.join(name))?;
        }
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(3))?;
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            Self::upgrade(&db)?;
        }
        db.execute_batch(IDENTITY_SCHEMA)?;
        let has_identity: bool = db.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_identity WHERE singleton=1)",
            [],
            |row| row.get(0),
        )?;
        if !has_identity {
            db.execute(
                "INSERT OR IGNORE INTO index_identity(singleton,generation) VALUES(1,?1)",
                [uuid::Uuid::new_v4().simple().to_string()],
            )?;
        }
        Ok(Self { db, _lease: lease })
    }
    /// Binds a completed scan to this actual derived database. Its existing shared
    /// lease prevents erasure from replacing the cache until this handle closes.
    pub fn cache_identity(&self) -> Result<String> {
        let value = self.stored_cache_identity()?;
        if !valid_cache_identity(&value) {
            return Err("Invalid derived index identity; original sessions are preserved".into());
        }
        Ok(value)
    }
    /// Reads the cache token separately so an explicit rebuild can detect an
    /// invalid token without converting SQLite access errors into corruption.
    fn stored_cache_identity(&self) -> Result<String> {
        Ok(self.db.query_row(
            "SELECT generation FROM index_identity WHERE singleton=1",
            [],
            |row| row.get(0),
        )?)
    }
    /// The index is derived from the session folders. An index of version 2 or older is
    /// converted row by row (title context added to section rows; older layouts also get
    /// their row ownership table filled), so search keeps working before the next scan.
    /// Any newer version is rebuilt empty and refilled by that scan, so it is never misread.
    fn upgrade(db: &Connection) -> Result<()> {
        db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<()> {
            let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
            if version == SCHEMA_VERSION {
                return Ok(()); // Another process finished the upgrade first.
            }
            let fts_exists: bool = db.query_row(
                "SELECT count(*)>0 FROM sqlite_master WHERE name='transcript_search'",
                [],
                |r| r.get(0),
            )?;
            if (0..=2).contains(&version) && fts_exists {
                db.execute_batch("ALTER TABLE transcript_search RENAME TO transcript_search_v2;")?;
                db.execute_batch(SCHEMA)?;
                db.execute_batch(
                    "INSERT INTO transcript_search(rowid,meeting_id,source_id,start_ms,end_ms,title,body,context)
                       SELECT s.rowid,s.meeting_id,s.source_id,s.start_ms,s.end_ms,s.title,s.body,
                         CASE WHEN s.source_id='' THEN '' ELSE coalesce((SELECT m.title FROM meetings m WHERE m.id=s.meeting_id),'') END
                       FROM transcript_search_v2 s;
                     DROP TABLE transcript_search_v2;
                     INSERT OR IGNORE INTO search_rows SELECT rowid,meeting_id FROM transcript_search;",
                )?;
            } else {
                db.execute_batch(
                    "DROP TABLE IF EXISTS index_identity;
                     DROP TABLE IF EXISTS transcript_search; DROP TABLE IF EXISTS search_rows;
                     DROP TABLE IF EXISTS meetings;",
                )?;
                db.execute_batch(SCHEMA)?;
            }
            db.execute_batch(&format!("PRAGMA user_version={SCHEMA_VERSION};"))?;
            Ok(())
        })();
        match result {
            Ok(()) => db.execute_batch("COMMIT")?,
            Err(e) => {
                db.execute_batch("ROLLBACK")?;
                return Err(e);
            }
        }
        Ok(())
    }
    pub fn current_fingerprint(&self, id: &str) -> Result<Option<String>> {
        use rusqlite::OptionalExtension;
        Ok(self
            .db
            .query_row("SELECT fingerprint FROM meetings WHERE id=?1", [id], |r| {
                r.get(0)
            })
            .optional()?)
    }
    /// Remove only derived rows for truly missing native session folders.
    /// Unreadable paths and imported legacy sessions remain indexed.
    pub fn prune_missing_sessions(
        &mut self,
        sessions: &Path,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<u64> {
        metadata::checked_directory(sessions)?;
        let ids: Vec<String> = {
            let mut statement = self.db.prepare("SELECT id FROM meetings LIMIT 1000001")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        if ids.len() > 1000000 {
            return Err("Search index exceeds safe pruning bound".into());
        }
        let mut removed = 0;
        self.transaction_batch(|db|{
            for id in &ids{
                if cancel.load(std::sync::atomic::Ordering::Acquire){return Err("Search pruning cancelled; original sessions unchanged".into());}
                if !metadata::valid_id(id)||id.starts_with("legacy-"){continue;}
                if matches!(fs::symlink_metadata(sessions.join(id)),Err(e) if e.kind()==std::io::ErrorKind::NotFound){db.remove(id)?;removed+=1;}
            }Ok(())
        })?;
        Ok(removed)
    }
    pub fn upsert(
        &mut self,
        data: &SessionData,
        meta: &metadata::Metadata,
        fingerprint: &str,
    ) -> Result<()> {
        if !metadata::valid_id(&data.id)
            || data.duration_ms > i64::MAX as u64
            || meta.revision > i64::MAX as u64
        {
            return Err("Invalid index document identity".into());
        }
        let title = meta.title.as_deref().unwrap_or(&data.title);
        let tx = self.db.savepoint()?;
        tx.execute(
            "DELETE FROM transcript_search WHERE rowid IN (SELECT row_id FROM search_rows WHERE meeting_id=?1)",
            [&data.id],
        )?;
        tx.execute("DELETE FROM search_rows WHERE meeting_id=?1", [&data.id])?;
        tx.execute("INSERT INTO meetings VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
          ON CONFLICT(id) DO UPDATE SET title=excluded.title,started=excluded.started,duration=excluded.duration,
          status=excluded.status,model=excluded.model,pinned=excluded.pinned,audio_removed=excluded.audio_removed,
          revision=excluded.revision,fingerprint=excluded.fingerprint",
          params![data.id,title,data.started_at,data.duration_ms as i64,data.status,data.model_id,meta.pinned,meta.audio_removed||(data.read_only&&!data.id.starts_with("legacy-")),meta.revision as i64,fingerprint])?;
        {
            let mut insert=tx.prepare_cached("INSERT INTO transcript_search(meeting_id,source_id,start_ms,end_ms,title,body,context) VALUES(?1,?2,?3,?4,?5,?6,?7)")?;
            insert.execute(params![data.id, "", 0, 0, title, "", ""])?;
            tx.execute(
                "INSERT INTO search_rows VALUES(?1,?2)",
                params![tx.last_insert_rowid(), data.id],
            )?;
            for part in &data.parts {
                let Some(text) = part.text.as_deref().filter(|s| !s.trim().is_empty()) else {
                    continue;
                };
                let start = part.start_sample.saturating_mul(1000) / u64::from(crate::SAMPLE_RATE);
                let end = part
                    .start_sample
                    .saturating_add(part.confirmed_sample_count)
                    .saturating_mul(1000)
                    / u64::from(crate::SAMPLE_RATE);
                if start > i64::MAX as u64 || end > i64::MAX as u64 {
                    return Err("Index section time overflow".into());
                }
                insert.execute(params![
                    data.id,
                    part.file,
                    start as i64,
                    end as i64,
                    "",
                    text,
                    title
                ])?;
                tx.execute(
                    "INSERT INTO search_rows VALUES(?1,?2)",
                    params![tx.last_insert_rowid(), data.id],
                )?;
            }
        }
        tx.commit()?;
        Ok(())
    }
    pub fn remove(&mut self, id: &str) -> Result<()> {
        let tx = self.db.savepoint()?;
        tx.execute("DELETE FROM transcript_search WHERE rowid IN (SELECT row_id FROM search_rows WHERE meeting_id=?1)", [id])?;
        tx.execute("DELETE FROM search_rows WHERE meeting_id=?1", [id])?;
        tx.execute("DELETE FROM meetings WHERE id=?1", [id])?;
        tx.commit()?;
        Ok(())
    }
    pub fn search(&self, query: &str, offset: u64, limit: u32) -> Result<SearchPage> {
        if query.chars().count() > 200 || !(1..=100).contains(&limit) || offset > i64::MAX as u64 {
            return Err("Invalid search range".into());
        }
        let terms = query
            .split_whitespace()
            // A token without letters or digits becomes an empty FTS phrase that matches nothing.
            .filter(|word| word.chars().any(char::is_alphanumeric))
            .take(20)
            .map(|word| format!("\"{}\"*", word.replace('"', "\"\"")))
            .collect::<Vec<_>>();
        let expression = match_expression(&terms);
        let (total, items) = if expression.is_empty() {
            let total = self
                .db
                .query_row("SELECT count(*) FROM meetings", [], |r| r.get::<_, i64>(0))?;
            let mut stmt=self.db.prepare("SELECT id,title,started,duration,status,model,pinned,audio_removed,revision,NULL,NULL,NULL,'' FROM meetings ORDER BY pinned DESC,started DESC,id LIMIT ?1 OFFSET ?2")?;
            let rows = stmt
                .query_map(params![limit, offset as i64], hit)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            (total, rows)
        } else {
            let total = self.db.query_row(
                "SELECT count(*) FROM transcript_search WHERE transcript_search MATCH ?1",
                [&expression],
                |r| r.get::<_, i64>(0),
            )?;
            let mut stmt=self.db.prepare("SELECT m.id,m.title,m.started,m.duration,m.status,m.model,m.pinned,m.audio_removed,m.revision,
              NULLIF(s.source_id,''),s.start_ms,s.end_ms,snippet(transcript_search,5,'','', ' … ',30)
              FROM transcript_search s JOIN meetings m ON m.id=s.meeting_id
              WHERE transcript_search MATCH ?1 ORDER BY m.pinned DESC,m.started DESC,m.id,s.rowid LIMIT ?2 OFFSET ?3")?;
            let rows = stmt
                .query_map(params![expression, limit, offset as i64], hit)?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            (total, rows)
        };
        Ok(SearchPage {
            items,
            total: total as u64,
            offset,
            limit,
        })
    }
    pub fn transaction_batch(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        // Explicit batches make the 100k benchmark representative of one indexing run.
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = operation(self);
        if result.is_ok() {
            self.db.execute_batch("COMMIT")?;
        } else {
            self.db.execute_batch("ROLLBACK")?;
        }
        result
    }
}
/// Every term must occur in the row. A meeting row matches when all terms are in its title.
/// A section row matches when all terms are in its text or meeting title and at least one is
/// in its text, so a title word plus a transcript word finds that section, while a title-only
/// query still yields the single meeting-level hit instead of one hit per section.
fn match_expression(terms: &[String]) -> String {
    if terms.is_empty() {
        return String::new();
    }
    let join = |column: &str, operator: &str| {
        terms
            .iter()
            .map(|term| format!("{column} : {term}"))
            .collect::<Vec<_>>()
            .join(operator)
    };
    format!(
        "({}) OR (({}) AND ({}))",
        join("title", " AND "),
        join("{body context}", " AND "),
        join("body", " OR ")
    )
}
fn hit(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchHit> {
    Ok(SearchHit {
        id: row.get(0)?,
        title: row.get(1)?,
        started_at: row.get(2)?,
        duration_ms: row.get::<_, i64>(3)? as u64,
        status: row.get(4)?,
        model_id: row.get(5)?,
        pinned: row.get(6)?,
        audio_removed: row.get(7)?,
        metadata_revision: row.get::<_, i64>(8)? as u64,
        source_id: row.get(9)?,
        start_ms: row.get::<_, Option<i64>>(10)?.map(|v| v as u64),
        end_ms: row.get::<_, Option<i64>>(11)?.map(|v| v as u64),
        excerpt: row.get(12)?,
    })
}

/// Cache identities are opaque native tokens, never session paths or content.
fn valid_cache_identity(value: &str) -> bool {
    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn unreadable_database(error: &(dyn std::error::Error + 'static)) -> bool {
    error.downcast_ref::<rusqlite::Error>().is_some_and(|e| {
        matches!(
            e.sqlite_error_code(),
            Some(rusqlite::ErrorCode::NotADatabase | rusqlite::ErrorCode::DatabaseCorrupt)
        )
    })
}
/// Every index reader/writer holds shared admission; explicit erasure holds exclusive
/// admission across complete cache removal, including retained unreadable database copies.
/// Exclusive admission tolerates only the existing bounded lock-retry budget;
/// shared readers still fail immediately when erasure owns the index.
fn index_lease(root: &Path, exclusive: bool) -> Result<std::fs::File> {
    metadata::checked_directory(root)?;
    let file = metadata::open_lock_file(&root.join("library-index.lock"))?;
    let result = if exclusive {
        metadata::lock_soon(&file)
    } else {
        fs2::FileExt::try_lock_shared(&file)
    };
    result.map_err(|_| {
        if exclusive {
            "The local search index is in use; wait for it to finish before continuing"
        } else {
            "The local search index is being updated; try again shortly"
        }
    })?;
    Ok(file)
}

#[cfg(test)]
mod index_admission_tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    /// Existing caches retain their token; explicit rebuild preserves invalid
    /// evidence aside and creates a new identity without touching originals.
    fn cache_identity_survives_reopen_and_invalid_identity_is_rebuildable() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("synthetic-original.txt");
        fs::write(&original, b"retained synthetic original").unwrap();
        let index = LibraryIndex::open(root.path()).unwrap();
        let identity = index.cache_identity().unwrap();
        drop(index);
        let index = LibraryIndex::open(root.path()).unwrap();
        assert_eq!(index.cache_identity().unwrap(), identity);
        index
            .db
            .execute(
                "UPDATE index_identity SET generation='invalid' WHERE singleton=1",
                [],
            )
            .unwrap();
        drop(index);
        let (rebuilt, retained) = LibraryIndex::open_for_rebuild(root.path()).unwrap();
        assert!(retained.is_some());
        assert_ne!(rebuilt.cache_identity().unwrap(), identity);
        assert_eq!(fs::read(original).unwrap(), b"retained synthetic original");
    }

    #[test]
    /// Release an actual shared OS lease only after exclusive admission observes
    /// contention. This requires the retry without a sleeping/probing race.
    fn exclusive_erasure_admission_retries_a_transient_index_reader() {
        let root = tempfile::tempdir().unwrap();
        let reader = index_lease(root.path(), false).unwrap();
        let observed = Arc::new(AtomicBool::new(false));
        let marker = observed.clone();
        let exclusive = metadata::observe_lock_retry(
            move || {
                marker.store(true, Ordering::Release);
                drop(reader);
            },
            || erasure_index_lock(root.path()),
        );
        assert!(
            observed.load(Ordering::Acquire),
            "A real held reader must cause contention"
        );
        let exclusive =
            exclusive.expect("Brief reader must not force another deletion confirmation");
        assert!(
            index_lease(root.path(), false).is_err(),
            "Shared admission remains fail-fast while erasure owns the lease"
        );
        drop(exclusive);
        assert!(index_lease(root.path(), false).is_ok());
    }

    #[test]
    /// A sustained reader still prevents destructive admission; no cache or
    /// source file is removed merely because bounded retries were exhausted.
    fn exclusive_erasure_admission_stays_closed_for_a_retained_reader() {
        let root = tempfile::tempdir().unwrap();
        let sentinel = root.path().join("synthetic-index-sentinel.txt");
        fs::write(&sentinel, b"retained synthetic content").unwrap();
        let reader = index_lease(root.path(), false).unwrap();
        assert!(erasure_index_lock(root.path()).is_err());
        assert_eq!(fs::read(&sentinel).unwrap(), b"retained synthetic content");
        drop(reader);
        assert!(erasure_index_lock(root.path()).is_ok());
    }
}
/// Reserve the rebuildable index against concurrent processes before deleting any source data.
pub(crate) fn erasure_index_lock(root: &Path) -> Result<std::fs::File> {
    index_lease(root, true)
}
const INDEX_FILES: [&str; 3] = [
    "library.sqlite3",
    "library.sqlite3-wal",
    "library.sqlite3-shm",
];
/// Only regular index files are moved; a link or a missing folder stops the recovery
/// before any file moves. A failed move puts the already moved files back.
fn set_aside_unreadable(root: &Path) -> Result<std::path::PathBuf> {
    let folder = metadata::checked_directory(&root.join("library-index"))?;
    let mut present = Vec::new();
    for name in INDEX_FILES {
        match fs::symlink_metadata(folder.join(name)) {
            Ok(meta) if meta.file_type().is_file() => present.push(name),
            Ok(_) => return Err("Search index file is not a regular file".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    let aside = folder.join(format!(
        "unreadable-{}-{}",
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ"),
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir(&aside)?;
    move_index_files(&folder, &aside, &present)?;
    Ok(aside)
}
fn move_index_files(folder: &Path, aside: &Path, names: &[&str]) -> Result<()> {
    for (done, name) in names.iter().enumerate() {
        if let Err(error) = fs::rename(folder.join(name), aside.join(name)) {
            for moved in &names[..done] {
                // Best effort: a file that cannot be moved back stays intact in `aside`.
                let _ = fs::rename(aside.join(moved), folder.join(moved));
            }
            // Removes only an empty folder; anything left in it is kept.
            let _ = fs::remove_dir(aside);
            return Err(error.into());
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(unix, windows))]
    #[test]
    fn index_open_and_rebuild_reject_existing_and_dangling_database_wal_and_shm_links() {
        for name in INDEX_FILES {
            for dangling in [false, true] {
                let root = tempfile::tempdir().unwrap();
                let outside = tempfile::tempdir().unwrap();
                let folder = root.path().join("library-index");
                fs::create_dir(&folder).unwrap();
                let sentinel = outside.path().join("user-file");
                fs::write(&sentinel, b"external user sentinel").unwrap();
                let target = if dangling {
                    outside.path().join("must-not-be-created")
                } else {
                    sentinel.clone()
                };
                let link = folder.join(name);
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &link).unwrap();
                #[cfg(windows)]
                std::os::windows::fs::symlink_file(&target, &link).unwrap();
                assert!(
                    LibraryIndex::open(root.path()).is_err(),
                    "{name}, dangling={dangling}"
                );
                assert!(
                    LibraryIndex::open_for_rebuild(root.path()).is_err(),
                    "{name}, dangling={dangling}"
                );
                assert!(fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink());
                assert_eq!(fs::read(&sentinel).unwrap(), b"external user sentinel");
                if dangling {
                    assert!(
                        matches!(fs::symlink_metadata(target), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
                    );
                }
                assert_eq!(
                    fs::read_dir(&folder).unwrap().count(),
                    1,
                    "No database or recovery files may be created before rejecting {name}"
                );
            }
        }
    }

    #[test]
    fn pruning_removes_only_missing_native_rows_and_cancellation_preserves_existing_index() {
        let root = tempfile::tempdir().unwrap();
        let sessions = root.path().join("sessions");
        fs::create_dir(&sessions).unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let absent = data("0123456789abcdef0123456789abcdef");
        let present = data("1123456789abcdef0123456789abcdef");
        fs::create_dir(sessions.join(&present.id)).unwrap();
        fs::write(
            sessions.join(&present.id).join("user-note.txt"),
            b"retained",
        )
        .unwrap();
        index
            .upsert(&absent, &metadata::Metadata::default(), "old")
            .unwrap();
        index
            .upsert(&present, &metadata::Metadata::default(), "present")
            .unwrap();
        assert!(index
            .prune_missing_sessions(&sessions, &std::sync::atomic::AtomicBool::new(true))
            .is_err());
        assert_eq!(index.search("", 0, 25).unwrap().total, 2);
        assert_eq!(
            index
                .prune_missing_sessions(&sessions, &std::sync::atomic::AtomicBool::new(false))
                .unwrap(),
            1
        );
        assert_eq!(index.search("", 0, 25).unwrap().items[0].id, present.id);
        assert_eq!(
            fs::read(sessions.join(present.id).join("user-note.txt")).unwrap(),
            b"retained"
        );
    }
    #[test]
    fn rebuild_moves_an_unreadable_index_aside_and_starts_fresh() {
        // Synthetic damage: the derived index file holds bytes SQLite cannot read.
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("library-index");
        fs::create_dir(&folder).unwrap();
        let damaged = b"not a sqlite database, synthetic damage ".repeat(200);
        fs::write(folder.join("library.sqlite3"), &damaged).unwrap();
        assert!(LibraryIndex::open(root.path()).is_err());
        let (mut index, aside) = LibraryIndex::open_for_rebuild(root.path()).unwrap();
        let aside = aside.expect("the unreadable index is set aside");
        // The root is canonical internally (/private/var on macOS, \\?\ on Windows).
        assert!(aside.starts_with(folder.canonicalize().unwrap()));
        assert_eq!(fs::read(aside.join("library.sqlite3")).unwrap(), damaged);
        let d = data("0123456789abcdef0123456789abcdef");
        index
            .upsert(&d, &metadata::Metadata::default(), "fixture")
            .unwrap();
        drop(index);
        assert_eq!(
            LibraryIndex::open(root.path())
                .unwrap()
                .search("Prüfbericht", 0, 20)
                .unwrap()
                .total,
            1
        );
        // A readable index is reused as it is.
        let (_, again) = LibraryIndex::open_for_rebuild(root.path()).unwrap();
        assert!(again.is_none());
    }
    #[test]
    fn rebuild_replaces_an_index_whose_fts_postings_are_damaged() {
        // Synthetic damage: the SQLite header is intact but FTS posting blocks are gone.
        let root = tempfile::tempdir().unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let d = data("0123456789abcdef0123456789abcdef");
        index
            .upsert(&d, &metadata::Metadata::default(), "fixture")
            .unwrap();
        drop(index);
        let path = root.path().join("library-index/library.sqlite3");
        let raw = Connection::open(&path).unwrap();
        assert!(
            raw.execute("DELETE FROM transcript_search_data WHERE id > 10", [])
                .unwrap()
                > 0
        );
        drop(raw);
        let reopened = LibraryIndex::open(root.path()).unwrap();
        assert!(reopened.damaged().unwrap());
        drop(reopened);
        let (index, aside) = LibraryIndex::open_for_rebuild(root.path()).unwrap();
        assert!(aside.unwrap().join("library.sqlite3").is_file());
        assert!(!index.damaged().unwrap());
        assert_eq!(index.search("", 0, 25).unwrap().total, 0);
    }
    #[test]
    fn a_failed_move_puts_the_index_files_back() {
        // Synthetic failure: a folder already occupies the WAL destination.
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("library-index");
        let aside = folder.join("unreadable-test");
        fs::create_dir_all(aside.join("library.sqlite3-wal")).unwrap();
        fs::write(folder.join("library.sqlite3"), b"db").unwrap();
        fs::write(folder.join("library.sqlite3-wal"), b"wal").unwrap();
        let names = ["library.sqlite3", "library.sqlite3-wal"];
        assert!(move_index_files(&folder, &aside, &names).is_err());
        assert_eq!(fs::read(folder.join("library.sqlite3")).unwrap(), b"db");
        assert_eq!(
            fs::read(folder.join("library.sqlite3-wal")).unwrap(),
            b"wal"
        );
        // The occupied folder is not empty, so it is kept rather than removed.
        assert!(aside.join("library.sqlite3-wal").is_dir());
        assert!(!aside.join("library.sqlite3").exists());
    }
    #[cfg(unix)]
    #[test]
    fn rebuild_never_moves_a_linked_index() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("library-index");
        fs::create_dir(&folder).unwrap();
        let target = root.path().join("elsewhere.sqlite3");
        fs::write(&target, b"user file").unwrap();
        std::os::unix::fs::symlink(&target, folder.join("library.sqlite3")).unwrap();
        assert!(LibraryIndex::open_for_rebuild(root.path()).is_err());
        assert!(fs::symlink_metadata(folder.join("library.sqlite3"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&target).unwrap(), b"user file");
    }
    fn data(id: &str) -> SessionData {
        serde_json::from_value(serde_json::json!({"schemaVersion":2,"id":id,"title":"Original title","startedAt":"2026-10-27T06:00:00Z","status":"complete","modelId":"synthetic","parts":[{"file":"system-0.wav","source":"system","startSample":16000,"sampleCount":32000,"confirmedSampleCount":32000,"text":"Anna bestätigt den Prüfbericht am Freitag.","status":"complete"}]})).unwrap()
    }
    #[test]
    fn unicode_search_returns_original_section_times_and_literal_evidence() {
        let root = tempfile::tempdir().unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let d = data("0123456789abcdef0123456789abcdef");
        index
            .upsert(&d, &metadata::Metadata::default(), "fixture")
            .unwrap();
        let found = index.search("Prüfbericht", 0, 20).unwrap();
        assert_eq!(found.total, 1);
        let item = &found.items[0];
        assert_eq!(item.source_id.as_deref(), Some("system-0.wav"));
        assert_eq!(item.start_ms, Some(1000));
        assert_eq!(item.end_ms, Some(3000));
        assert!(item.excerpt.contains("Prüfbericht"));
        assert!(index
            .search("\" OR 1=1 --", 0, 20)
            .unwrap()
            .items
            .is_empty());
        assert!(index.search("Anna", 0, 101).is_err());
        // Standalone punctuation such as a dash must not empty the whole AND query.
        assert_eq!(index.search("Prüfbericht – /", 0, 20).unwrap().total, 1);
    }
    #[test]
    fn renamed_pinned_metadata_is_searchable_without_changing_the_source() {
        let root = tempfile::tempdir().unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let d = data("0123456789abcdef0123456789abcdef");
        let original = serde_json::to_vec(&d).unwrap();
        let meta = metadata::Metadata {
            revision: 1,
            title: Some("Renamed priority".into()),
            pinned: true,
            audio_removed: false,
        };
        index.upsert(&d, &meta, "changed").unwrap();
        let found = index.search("priority", 0, 20).unwrap();
        assert!(found.items[0].pinned);
        assert_eq!(found.items[0].metadata_revision, 1);
        assert_eq!(serde_json::to_vec(&d).unwrap(), original);
        index.remove(&d.id).unwrap();
        assert_eq!(index.search("", 0, 20).unwrap().total, 0);
    }
    fn two_sections(id: &str) -> SessionData {
        serde_json::from_value(serde_json::json!({"schemaVersion":2,"id":id,"title":"Quartalsplanung Vertrieb","startedAt":"2026-10-27T06:00:00Z","status":"complete","modelId":"synthetic","parts":[
            {"file":"system-0.wav","source":"system","startSample":16000,"sampleCount":32000,"confirmedSampleCount":32000,"text":"Anna bestätigt den Prüfbericht am Freitag.","status":"complete"},
            {"file":"system-1.wav","source":"system","startSample":64000,"sampleCount":32000,"confirmedSampleCount":16000,"text":"Ben fragt nach dem Budget.","status":"complete"}]})).unwrap()
    }
    #[test]
    fn query_mixing_title_and_transcript_words_finds_the_matching_section_only() {
        let root = tempfile::tempdir().unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let d = two_sections("0123456789abcdef0123456789abcdef");
        index
            .upsert(&d, &metadata::Metadata::default(), "fixture")
            .unwrap();
        // Regression for #22: title and section text used to sit in different rows.
        let mixed = index.search("Quartalsplanung Prüfbericht", 0, 20).unwrap();
        assert_eq!(mixed.total, 1);
        assert_eq!(mixed.items[0].source_id.as_deref(), Some("system-0.wav"));
        assert_eq!(mixed.items[0].start_ms, Some(1000));
        assert_eq!(mixed.items[0].end_ms, Some(3000));
        assert!(mixed.items[0].excerpt.contains("Prüfbericht"));
        let second = index.search("vertrieb budget", 0, 20).unwrap();
        assert_eq!(second.total, 1);
        assert_eq!(second.items[0].source_id.as_deref(), Some("system-1.wav"));
        assert_eq!(second.items[0].start_ms, Some(4000));
        assert_eq!(second.items[0].end_ms, Some(5000));
        // A title-only query stays one meeting-level hit, not one hit per section.
        let title = index.search("Quartalsplanung", 0, 20).unwrap();
        assert_eq!(title.total, 1);
        assert_eq!(title.items[0].source_id, None);
        // Words from two different sections are not combined into one hit.
        assert_eq!(index.search("Prüfbericht Budget", 0, 20).unwrap().total, 0);
        assert_eq!(
            index
                .search("Quartalsplanung Unbekannt", 0, 20)
                .unwrap()
                .total,
            0
        );
        // A renamed title replaces the original one in every section row.
        let meta = metadata::Metadata {
            revision: 1,
            title: Some("Umbenannt".into()),
            pinned: false,
            audio_removed: false,
        };
        index.upsert(&d, &meta, "renamed").unwrap();
        assert_eq!(index.search("Umbenannt Budget", 0, 20).unwrap().total, 1);
        assert_eq!(
            index.search("Quartalsplanung Budget", 0, 20).unwrap().total,
            0
        );
        assert_eq!(index.search("", 0, 20).unwrap().total, 1);
        let rows: i64 = index
            .db
            .query_row("SELECT count(*) FROM search_rows", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 3, "one meeting row and one row per section");
    }
    #[test]
    fn version_one_index_keeps_its_rows_while_converting() {
        // Synthetic layout of early 1.0.2 development builds: no row ownership table yet.
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("library-index");
        fs::create_dir(&folder).unwrap();
        let id = "0123456789abcdef0123456789abcdef";
        {
            let db = Connection::open(folder.join("library.sqlite3")).unwrap();
            db.execute_batch("CREATE TABLE meetings (
                id TEXT PRIMARY KEY, title TEXT NOT NULL, started TEXT NOT NULL, duration INTEGER NOT NULL,
                status TEXT NOT NULL, model TEXT NOT NULL, pinned INTEGER NOT NULL, audio_removed INTEGER NOT NULL,
                revision INTEGER NOT NULL, fingerprint TEXT NOT NULL);
              CREATE VIRTUAL TABLE transcript_search USING fts5(
                meeting_id UNINDEXED, source_id UNINDEXED, start_ms UNINDEXED, end_ms UNINDEXED,
                title, body, tokenize='unicode61 remove_diacritics 2');
              PRAGMA user_version=1;").unwrap();
            db.execute(
                "INSERT INTO meetings VALUES(?1,'Quartalsplanung','2026-10-27T06:00:00Z',3000,'complete','synthetic',0,0,0,'v1')",
                [id],
            )
            .unwrap();
            for (source, start, end, title, body) in [
                ("", 0, 0, "Quartalsplanung", ""),
                (
                    "system-0.wav",
                    1000,
                    3000,
                    "",
                    "Anna bestätigt den Prüfbericht.",
                ),
            ] {
                db.execute(
                    "INSERT INTO transcript_search(meeting_id,source_id,start_ms,end_ms,title,body) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![id, source, start, end, title, body],
                )
                .unwrap();
            }
        }
        let mut index = LibraryIndex::open(root.path()).unwrap();
        assert_eq!(
            index.current_fingerprint(id).unwrap().as_deref(),
            Some("v1")
        );
        let found = index.search("Quartalsplanung Prüfbericht", 0, 20).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.items[0].source_id.as_deref(), Some("system-0.wav"));
        // The filled ownership table lets a later update replace the rows, not duplicate them.
        index
            .upsert(&data(id), &metadata::Metadata::default(), "fresh")
            .unwrap();
        assert_eq!(index.search("Prüfbericht", 0, 20).unwrap().total, 1);
        assert_eq!(index.search("Quartalsplanung", 0, 20).unwrap().total, 0);
    }

    #[test]
    fn version_two_index_is_converted_and_unknown_versions_are_rebuilt() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("library-index");
        fs::create_dir(&folder).unwrap();
        let id = "0123456789abcdef0123456789abcdef";
        {
            // Layout written by HandyMeeting up to 1.0.2 (schema version 2).
            let db = Connection::open(folder.join("library.sqlite3")).unwrap();
            db.execute_batch("CREATE TABLE meetings (
                id TEXT PRIMARY KEY, title TEXT NOT NULL, started TEXT NOT NULL, duration INTEGER NOT NULL,
                status TEXT NOT NULL, model TEXT NOT NULL, pinned INTEGER NOT NULL, audio_removed INTEGER NOT NULL,
                revision INTEGER NOT NULL, fingerprint TEXT NOT NULL);
              CREATE VIRTUAL TABLE transcript_search USING fts5(
                meeting_id UNINDEXED, source_id UNINDEXED, start_ms UNINDEXED, end_ms UNINDEXED,
                title, body, tokenize='unicode61 remove_diacritics 2');
              CREATE TABLE search_rows(row_id INTEGER PRIMARY KEY, meeting_id TEXT NOT NULL);
              PRAGMA user_version=2;").unwrap();
            db.execute(
                "INSERT INTO meetings VALUES(?1,'Quartalsplanung','2026-10-27T06:00:00Z',3000,'complete','synthetic',0,0,0,'v2')",
                [id],
            )
            .unwrap();
            for (source, start, end, title, body) in [
                ("", 0, 0, "Quartalsplanung", ""),
                (
                    "system-0.wav",
                    1000,
                    3000,
                    "",
                    "Anna bestätigt den Prüfbericht.",
                ),
            ] {
                db.execute(
                    "INSERT INTO transcript_search(meeting_id,source_id,start_ms,end_ms,title,body) VALUES(?1,?2,?3,?4,?5,?6)",
                    params![id, source, start, end, title, body],
                )
                .unwrap();
                db.execute(
                    "INSERT INTO search_rows VALUES(?1,?2)",
                    params![db.last_insert_rowid(), id],
                )
                .unwrap();
            }
        }
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let version: i64 = index
            .db
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, SCHEMA_VERSION);
        assert_eq!(
            index.current_fingerprint(id).unwrap().as_deref(),
            Some("v2")
        );
        let found = index.search("Quartalsplanung Prüfbericht", 0, 20).unwrap();
        assert_eq!(found.total, 1);
        assert_eq!(found.items[0].source_id.as_deref(), Some("system-0.wav"));
        assert_eq!(found.items[0].start_ms, Some(1000));
        assert_eq!(index.search("Quartalsplanung", 0, 20).unwrap().total, 1);
        // Converted rows stay attributable, so a later update replaces them instead of duplicating.
        index
            .upsert(&data(id), &metadata::Metadata::default(), "fresh")
            .unwrap();
        assert_eq!(index.search("Prüfbericht", 0, 20).unwrap().total, 1);
        assert_eq!(index.search("Quartalsplanung", 0, 20).unwrap().total, 0);
        drop(index);
        // A layout this version does not know is rebuilt, never misread.
        Connection::open(folder.join("library.sqlite3"))
            .unwrap()
            .execute_batch("PRAGMA user_version=99;")
            .unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        assert_eq!(index.search("", 0, 20).unwrap().total, 0);
        assert_eq!(index.current_fingerprint(id).unwrap(), None);
        index
            .upsert(&data(id), &metadata::Metadata::default(), "rebuilt")
            .unwrap();
        assert_eq!(
            index.search("Original Prüfbericht", 0, 20).unwrap().total,
            1
        );
    }
    #[test]
    fn failed_batch_rolls_back_search_and_labels_without_harming_prior_documents() {
        let root = tempfile::tempdir().unwrap();
        let mut index = LibraryIndex::open(root.path()).unwrap();
        let first = data("0123456789abcdef0123456789abcdef");
        let second = data("1123456789abcdef0123456789abcdef");
        index
            .upsert(&first, &metadata::Metadata::default(), "prior")
            .unwrap();
        let result = index.transaction_batch(|db| {
            db.remove(&first.id)?;
            db.upsert(&second, &metadata::Metadata::default(), "pending")?;
            Err("Synthetic indexing interruption".into())
        });
        assert!(result.is_err());
        let page = index.search("Prüfbericht", 0, 20).unwrap();
        assert_eq!(page.total, 1);
        assert_eq!(page.items[0].id, first.id);
        index
            .transaction_batch(|db| db.upsert(&second, &metadata::Metadata::default(), "confirmed"))
            .unwrap();
        assert_eq!(index.search("Prüfbericht", 0, 20).unwrap().total, 2);
    }
}
