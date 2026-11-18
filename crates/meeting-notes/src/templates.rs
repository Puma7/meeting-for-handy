//! Immutable, revisioned template libraries. This store receives no transcripts
//! or connection settings; a failed write never replaces a previous revision.
use crate::{validate_template, Template};
use anyhow::{ensure, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
};

const MAX_BYTES: u64 = 8_000_000;
const MAX_REVISIONS: usize = 10_000;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateLibrary {
    pub revision: u64,
    pub templates: Vec<Template>,
}
pub struct TemplateStore<'a>(&'a Path);
impl<'a> TemplateStore<'a> {
    pub fn new(root: &'a Path) -> Self {
        Self(root)
    }
    fn lock(&self) -> Result<std::fs::File> {
        fs::create_dir_all(self.0)?;
        let path = self.0.join("library.lock");
        if let Ok(meta) = fs::symlink_metadata(&path) {
            ensure!(
                meta.file_type().is_file(),
                "Ungültige Vorlagensperre; vorhandene Dateien bleiben erhalten."
            );
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        file.try_lock_exclusive().context(
            "Vorlagen werden gerade in einem anderen Fenster bearbeitet. Bitte erneut versuchen.",
        )?;
        Ok(file)
    }
    pub fn list(&self) -> Result<TemplateLibrary> {
        let _lease = self.lock()?;
        self.read_current()
    }
    fn read_current(&self) -> Result<TemplateLibrary> {
        let mut revisions = Vec::new();
        for entry in fs::read_dir(self.0)? {
            let entry = entry?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .context("Ungültiger Dateiname im Vorlagenarchiv.")?;
            if name.starts_with("revision-") {
                let value=name.strip_prefix("revision-").and_then(|s|s.strip_suffix(".json")).filter(|s|s.len()==16&&s.bytes().all(|b|b.is_ascii_digit())).context("Beschädigter Dateiname im Vorlagenarchiv. Vorhandene Dateien bleiben erhalten.")?;
                let revision = value.parse::<u64>()?;
                ensure!(
                    revision > 0 && entry.file_type()?.is_file(),
                    "Ungültiger Vorlagenstand."
                );
                revisions.push((revision, entry.path()));
                ensure!(
                    revisions.len() <= MAX_REVISIONS,
                    "Das Vorlagenarchiv ist zu groß; vorhandene Fassungen bleiben erhalten."
                );
            }
        }
        revisions.sort_by_key(|r| r.0);
        ensure!(revisions.iter().enumerate().all(|(i,r)|r.0==i as u64+1),"Im Vorlagenarchiv fehlt ein bestätigter Stand; es wird nicht automatisch zurückgesetzt.");
        let Some((revision, path)) = revisions.last() else {
            return Ok(TemplateLibrary {
                revision: 0,
                templates: vec![],
            });
        };
        ensure!(
            fs::metadata(path)?.len() <= MAX_BYTES,
            "Gespeicherte Vorlagen sind zu groß."
        );
        let library:TemplateLibrary=serde_json::from_slice(&fs::read(path)?).context("Gespeicherte Vorlagen können nicht gelesen werden. Vorhandene Fassungen bleiben erhalten.")?;
        ensure!(
            library.revision == *revision,
            "Vorlagenkennung und Dateistand passen nicht zusammen."
        );
        validate_library(&library.templates)?;
        Ok(library)
    }
    pub fn save(&self, library: TemplateLibrary) -> Result<TemplateLibrary> {
        validate_library(&library.templates)?;
        let _lease = self.lock()?;
        let current = self.read_current()?;
        ensure!(current.revision==library.revision,"Die Vorlagen wurden zwischenzeitlich geändert. Bitte neu laden; Ihre Änderungen werden nicht über einen neueren Stand geschrieben.");
        self.commit(current.revision, library.templates)
    }
    pub fn import(&self, templates: Vec<Template>) -> Result<TemplateLibrary> {
        validate_library(&templates)?;
        let _lease = self.lock()?;
        let mut current = self.read_current()?;
        let before = serde_json::to_vec(&current.templates)?;
        for mut incoming in templates {
            if let Some(existing) = current.templates.iter().find(|t| t.id == incoming.id) {
                if serde_json::to_vec(existing)? == serde_json::to_vec(&incoming)? {
                    continue;
                }
                incoming.id = uuid::Uuid::new_v4().simple().to_string();
                incoming.name = format!(
                    "{} (Import)",
                    incoming.name.chars().take(91).collect::<String>()
                );
                for step in &mut incoming.steps {
                    step.id = uuid::Uuid::new_v4().simple().to_string();
                }
            }
            current.templates.push(incoming);
        }
        validate_library(&current.templates)?;
        if before == serde_json::to_vec(&current.templates)? {
            return Ok(current);
        }
        self.commit(current.revision, current.templates)
    }
    fn commit(&self, previous: u64, templates: Vec<Template>) -> Result<TemplateLibrary> {
        ensure!(
            previous < MAX_REVISIONS as u64,
            "Das Vorlagenarchiv ist voll; vorhandene Fassungen bleiben erhalten."
        );
        let library = TemplateLibrary {
            revision: previous + 1,
            templates,
        };
        let bytes = serde_json::to_vec_pretty(&library)?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Vorlagenbibliothek überschreitet 8 MB."
        );
        let path = self
            .0
            .join(format!("revision-{:016}.json", library.revision));
        let (mut file, path) = crate::service::pending_publication(&path, "pending-template-")?;
        file.write_all(&bytes)?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path).map_err(|_| {
            anyhow::anyhow!(
                "Vorlagenstand konnte nicht bestätigt werden; frühere Fassungen bleiben erhalten."
            )
        })?;
        #[cfg(unix)]
        std::fs::File::open(self.0)?.sync_all()?;
        Ok(library)
    }
}
pub fn validate_library(templates: &[Template]) -> Result<()> {
    ensure!(
        (1..=100).contains(&templates.len()),
        "Eine Bibliothek benötigt 1–100 Vorlagen."
    );
    let mut ids = HashSet::new();
    for template in templates {
        ensure!(
            !template.id.is_empty()
                && template.id.len() <= 128
                && !template.id.chars().any(char::is_control)
                && ids.insert(&template.id),
            "Ungültige oder doppelte Vorlagenkennung."
        );
        validate_template(template)?;
        ensure!(
            template
                .steps
                .iter()
                .all(|s| s.id.len() <= 128 && !s.id.chars().any(char::is_control)),
            "Ungültige Schrittkennung."
        );
    }
    ensure!(
        serde_json::to_vec(templates)?.len() as u64 <= MAX_BYTES,
        "Vorlagenbibliothek überschreitet 8 MB."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Template {
        Template {
            id: "work".into(),
            name: "Work".into(),
            steps: vec![crate::Step {
                id: "summary".into(),
                name: "Summary".into(),
                kind: crate::Kind::Summary,
                prompt: "Summarize with exact original quotes.".into(),
            }],
        }
    }
    #[test]
    fn stale_agent_save_cannot_replace_new_ui_revision_and_history_is_immutable() {
        let root = tempfile::tempdir().unwrap();
        let store = TemplateStore::new(root.path());
        assert_eq!(store.list().unwrap().revision, 0);
        let first = store
            .save(TemplateLibrary {
                revision: 0,
                templates: vec![fixture()],
            })
            .unwrap();
        let path = root.path().join("revision-0000000000000001.json");
        let bytes = fs::read(&path).unwrap();
        let mut changed = first.clone();
        changed.templates[0].steps[0].prompt = "A different explicit prompt.".into();
        let second = store.save(changed).unwrap();
        assert_eq!(second.revision, 2);
        assert!(store.save(first).is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
        assert_eq!(
            store.list().unwrap().templates[0].steps[0].prompt,
            "A different explicit prompt."
        );
    }
    #[test]
    fn unreadable_or_missing_revision_blocks_reset_and_retains_original_bytes() {
        let root = tempfile::tempdir().unwrap();
        let store = TemplateStore::new(root.path());
        store
            .save(TemplateLibrary {
                revision: 0,
                templates: vec![fixture()],
            })
            .unwrap();
        let path = root.path().join("revision-0000000000000001.json");
        let broken = b"unreadable original template bytes";
        fs::write(&path, broken).unwrap();
        assert!(store.list().is_err());
        assert!(store
            .save(TemplateLibrary {
                revision: 0,
                templates: vec![fixture()]
            })
            .is_err());
        assert!(store.import(vec![fixture()]).is_err());
        assert_eq!(fs::read(path).unwrap(), broken);
        let missing = tempfile::tempdir().unwrap();
        fs::write(missing.path().join("revision-0000000000000002.json"), b"{}").unwrap();
        assert!(TemplateStore::new(missing.path()).list().is_err());
    }
    #[test]
    fn explicit_import_preserves_colliding_changed_templates_as_separate_copies() {
        let root = tempfile::tempdir().unwrap();
        let store = TemplateStore::new(root.path());
        let original = store.import(vec![fixture()]).unwrap();
        assert_eq!(original.revision, 1);
        assert_eq!(store.import(vec![fixture()]).unwrap().revision, 1);
        let mut changed = fixture();
        changed.steps[0].prompt = "Keep a second original prompt.".into();
        let imported = store.import(vec![changed]).unwrap();
        assert_eq!(imported.revision, 2);
        assert_eq!(imported.templates.len(), 2);
        assert_eq!(imported.templates[0].id, "work");
        assert_ne!(imported.templates[1].id, "work");
        assert_ne!(
            imported.templates[0].steps[0].id,
            imported.templates[1].steps[0].id
        );
        assert_eq!(
            store.list().unwrap().templates[0].steps[0].prompt,
            fixture().steps[0].prompt
        );
    }
    #[test]
    fn active_cross_process_lease_refuses_writes_without_creating_a_revision() {
        let root = tempfile::tempdir().unwrap();
        let store = TemplateStore::new(root.path());
        let _lease = store.lock().unwrap();
        assert!(store
            .save(TemplateLibrary {
                revision: 0,
                templates: vec![fixture()]
            })
            .is_err());
        assert!(!root.path().join("revision-0000000000000001.json").exists());
    }
}
