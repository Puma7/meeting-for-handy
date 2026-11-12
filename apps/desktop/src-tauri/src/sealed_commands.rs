//! Explicit user-selected encrypted backup and recovery. The renderer supplies
//! only the session identity and unlock credential, never a read or write path.
use crate::{blocking, operations::Kind, AppResult, Service};
use meeting_core::archive::sealed;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

#[derive(Default)]
pub(crate) struct Control(Mutex<Option<Arc<AtomicBool>>>);

impl Control {
    fn begin(&self) -> AppResult<Active<'_>> {
        let mut current = self.0.lock().map_err(|_| "Backup control is unavailable")?;
        if current.is_some() {
            return Err("An encrypted backup operation is already running.".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *current = Some(Arc::clone(&cancel));
        Ok(Active {
            control: self,
            cancel,
        })
    }
}

struct Active<'a> {
    control: &'a Control,
    cancel: Arc<AtomicBool>,
}

impl Drop for Active<'_> {
    fn drop(&mut self) {
        if let Ok(mut current) = self.control.0.lock() {
            *current = None;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BackupReceipt {
    file: String,
    summary: sealed::Summary,
    #[serde(serialize_with = "serialize_recovery_key")]
    recovery_key: Zeroizing<String>,
}

fn serialize_recovery_key<S: serde::Serializer>(
    key: &Zeroizing<String>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(key.as_str())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum CredentialKind {
    Passphrase,
    RecoveryKey,
}

#[tauri::command]
pub(crate) async fn create_encrypted_backup(
    service: State<'_, Arc<Service>>,
    app: AppHandle,
    meeting_id: String,
    passphrase: String,
) -> AppResult<Option<BackupReceipt>> {
    let service = Arc::clone(service.inner());
    let passphrase = Zeroizing::new(passphrase);
    blocking(move || {
        create_backup(&service, &meeting_id, passphrase.as_bytes(), || {
            app.dialog()
                .file()
                .set_title("Verschlüsselte Sitzungskopie speichern / Save encrypted meeting copy")
                .add_filter("HandyMeeting", &["hmvault"])
                .set_file_name("HandyMeeting.hmvault")
                .blocking_save_file()
                .map(|path| {
                    path.into_path()
                        .map_err(|_| "Only a local backup file is supported".into())
                })
                .transpose()
        })
    })
    .await
}

fn create_backup(
    service: &Arc<Service>,
    meeting_id: &str,
    passphrase: &[u8],
    choose_destination: impl FnOnce() -> AppResult<Option<PathBuf>>,
) -> AppResult<Option<BackupReceipt>> {
    crate::validate_id(meeting_id)?;
    // The detail view retains the store's writer lease. Drop it and fence reopens
    // until the encrypted backup releases its own session lease.
    let _admission = crate::retention_commands::reserve(service)?;
    let operation = service.sealed_backups.begin()?;
    let Some(mut path) = choose_destination()? else {
        return Ok(None);
    };
    if path.extension().is_none() {
        path.set_extension("hmvault");
    }
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("hmvault"))
    {
        return Err(
            "Bitte die Dateiendung .hmvault verwenden. / Use the .hmvault extension.".into(),
        );
    }
    let (summary, recovery_key) = sealed::create(
        &service.root,
        meeting_id,
        &path,
        passphrase,
        &operation.cancel,
    )
    .map_err(|e| e.to_string())?;
    Ok(Some(BackupReceipt {
        file: path.to_string_lossy().into_owned(),
        summary,
        recovery_key: recovery_key.encode(),
    }))
}

#[tauri::command]
pub(crate) async fn recover_encrypted_backup(
    service: State<'_, Arc<Service>>,
    app: AppHandle,
    credential: String,
    kind: CredentialKind,
) -> AppResult<Option<String>> {
    let service = Arc::clone(service.inner());
    let credential = Zeroizing::new(credential);
    blocking(move || {
        let recovery_key;
        let unlock = match kind {
            CredentialKind::Passphrase => sealed::Unlock::Passphrase(credential.as_bytes()),
            CredentialKind::RecoveryKey => {
                recovery_key = sealed::RecoveryKey::parse(&credential).map_err(|e| e.to_string())?;
                sealed::Unlock::RecoveryKey(&recovery_key)
            }
        };
        let _admission = service.reserve_operation(Kind::Maintenance)?;
        let operation = service.sealed_backups.begin()?;
        let Some(input) = app.dialog().file()
            .set_title("Verschlüsselte Sicherung auswählen / Choose encrypted backup")
            .add_filter("HandyMeeting", &["hmvault"])
            .blocking_pick_file() else { return Ok(None); };
        let input = input.into_path().map_err(|_| "Only a local backup file is supported")?;
        let Some(directory) = app.dialog().file()
            .set_title("Ordner für separates Wiederherstellungsprofil / Folder for separate recovery profile")
            .blocking_pick_folder() else { return Ok(None); };
        let directory = directory.into_path().map_err(|_| "Only a local recovery directory is supported")?;
        let profile = sealed::recover(&service.root, &input, &directory, unlock, &operation.cancel)
            .map_err(|e| e.to_string())?;
        Ok(Some(profile.to_string_lossy().into_owned()))
    }).await
}

#[tauri::command]
pub(crate) fn cancel_encrypted_backup(service: State<'_, Arc<Service>>) -> AppResult<bool> {
    let current = service
        .sealed_backups
        .0
        .lock()
        .map_err(|_| "Backup control is unavailable")?;
    if let Some(cancel) = current.as_ref() {
        cancel.store(true, Ordering::Release);
    }
    Ok(current.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_profile::TestProfile;
    use meeting_core::SessionStore;

    #[test]
    fn creates_encrypted_backup_from_an_open_meeting_detail() {
        let fixture = TestProfile::new().unwrap();
        let root = fixture.path().join("profile");
        let service = Arc::new(Service::new(root.clone()).unwrap());
        let store =
            SessionStore::create(&root.join("sessions"), "Synthetic meeting", "m", "s", None)
                .unwrap();
        let id = store.snapshot().unwrap().id;
        store.set_status("complete").unwrap();
        drop(store);

        assert_eq!(service.data(&id).unwrap().id, id);
        assert!(SessionStore::open(&root.join("sessions").join(&id)).is_err());
        let output = fixture.path().join("selected-backup.hmvault");
        let passphrase = Zeroizing::new(meeting_notes::new_job_id());
        let receipt = create_backup(&service, &id, passphrase.as_bytes(), || {
            assert!(
                service.data(&id).is_err(),
                "Backup must fence detail reopens"
            );
            Ok(Some(output.clone()))
        })
        .unwrap()
        .unwrap();

        assert_eq!(receipt.summary.meeting_id, id);
        assert!(output.is_file());
        assert_eq!(service.data(&id).unwrap().id, id);
    }
}
