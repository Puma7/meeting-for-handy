//! Native folder choice and launch for a verified local profile.
use crate::{blocking, operations::Kind, AppResult, Service};
use meeting_core::archive::protected_profile as storage;
use serde::Serialize;
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
mod startup;
#[cfg(any(windows, target_os = "linux", target_os = "macos"))]
pub(super) use startup::{acknowledge_window, managed_startup, report_failure};

#[derive(Default)]
pub(super) struct CopyControl(Mutex<Option<Arc<storage::MigrationControl>>>);

impl CopyControl {
    fn begin(&self) -> AppResult<ActiveCopy<'_>> {
        let mut active = self
            .0
            .lock()
            .map_err(|_| "Profile copy control is unavailable")?;
        if active.is_some() {
            return Err(
                "Eine Profilkopie läuft bereits. / A profile copy is already running.".into(),
            );
        }
        let migration = Arc::new(storage::MigrationControl::default());
        *active = Some(Arc::clone(&migration));
        Ok(ActiveCopy {
            control: self,
            migration,
        })
    }

    fn cancel(&self) -> AppResult<bool> {
        let active = self
            .0
            .lock()
            .map_err(|_| "Profile copy control is unavailable")?;
        Ok(active.as_ref().is_some_and(|migration| migration.cancel()))
    }

    fn state(&self) -> AppResult<Option<MigrationState>> {
        let active = self
            .0
            .lock()
            .map_err(|_| "Profile copy control is unavailable")?;
        Ok(active.as_ref().map(|migration| {
            if migration.cancellation_requested() {
                MigrationState::Cancelling
            } else {
                MigrationState::Copying
            }
        }))
    }
}

struct ActiveCopy<'a> {
    control: &'a CopyControl,
    migration: Arc<storage::MigrationControl>,
}

impl Drop for ActiveCopy<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.control.0.lock() {
            *active = None;
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
enum MigrationState {
    Copying,
    Cancelling,
}

#[derive(Serialize)]
pub(crate) struct Status {
    supported: bool,
    active: bool,
    provider: Option<&'static str>,
    migration: Option<MigrationState>,
}

#[tauri::command]
pub(crate) fn protected_profile_status(service: State<'_, Arc<Service>>) -> AppResult<Status> {
    Ok(Status {
        supported: storage::supported(),
        active: service.protected_profile,
        provider: storage::provider_name(),
        migration: service.protected_profile_migration.state()?,
    })
}

#[tauri::command]
pub(crate) async fn create_protected_profile(
    service: State<'_, Arc<Service>>,
    app: AppHandle,
    passphrase: Option<String>,
) -> AppResult<Option<storage::Receipt>> {
    let service = Arc::clone(service.inner());
    let passphrase = passphrase.map(Zeroizing::new);
    blocking(move || {
        validate_create(&service)?;
        copy_after_selection(&service, || {
            app.dialog().file()
                .set_title("Ordner für das geschützte Profil / Parent folder for the protected profile")
                .blocking_pick_folder()
                .map(|parent| parent.into_path().map_err(|_| "Choose a local folder.".into()))
                .transpose()
        }, |parent, control| {
            validate_create(&service)?;
            #[cfg(any(target_os = "linux", target_os = "macos"))]
            let receipt = storage::migrate_volume_cancellable(&service.root, parent, required_passphrase(passphrase.as_ref())?, control);
            #[cfg(not(any(target_os = "linux", target_os = "macos")))]
            let receipt = {
                drop(passphrase);
                storage::migrate_cancellable(&service.root, parent, control)
            };
            receipt.map_err(|error| error.to_string())
        })
    }).await
}

fn validate_create(service: &Service) -> AppResult<()> {
    if !storage::supported() || service.protected_profile {
        return Err("Ein neues verschlüsseltes Profil ist hier nicht verfügbar. / A new encrypted profile is unavailable here.".into());
    }
    Ok(())
}

fn copy_after_selection(
    service: &Arc<Service>,
    choose: impl FnOnce() -> AppResult<Option<PathBuf>>,
    migrate: impl FnOnce(&std::path::Path, &storage::MigrationControl) -> AppResult<storage::Receipt>,
) -> AppResult<Option<storage::Receipt>> {
    let operation = service.protected_profile_migration.begin()?;
    let Some(parent) = choose()? else {
        return Ok(None);
    };
    let _admission = crate::retention_commands::reserve(service)?;
    let receipt = migrate(&parent, &operation.migration)?;
    *service
        .protected_profile_copy
        .lock()
        .map_err(|_| "Protected profile state is unavailable")? = Some(receipt.path.clone());
    Ok(Some(receipt))
}

#[tauri::command]
pub(crate) fn cancel_protected_profile_migration(
    service: State<'_, Arc<Service>>,
) -> AppResult<bool> {
    service.protected_profile_migration.cancel()
}

#[tauri::command]
pub(crate) async fn open_protected_profile(
    service: State<'_, Arc<Service>>,
    app: AppHandle,
    passphrase: Option<String>,
) -> AppResult<bool> {
    let service = Arc::clone(service.inner());
    let passphrase = passphrase.map(Zeroizing::new);
    blocking(move || {
        open_after_selection(
            &service,
            || {
                app.dialog()
                    .file()
                    .set_title("Geschütztes Profil auswählen / Choose the protected profile")
                    .blocking_pick_folder()
                    .map(|path| {
                        path.into_path()
                            .map_err(|_| "Choose a local folder.".into())
                    })
                    .transpose()
            },
            |path| launch(path, passphrase),
        )
    })
    .await
}

fn open_after_selection(
    service: &Service,
    choose: impl FnOnce() -> AppResult<Option<PathBuf>>,
    launch: impl FnOnce(PathBuf) -> AppResult<()>,
) -> AppResult<bool> {
    let recent = service
        .protected_profile_copy
        .lock()
        .map_err(|_| "Protected profile state is unavailable")?
        .clone();
    let path = match &recent {
        Some(path) => path.clone(),
        None => match choose()? {
            Some(path) => path,
            None => return Ok(false),
        },
    };
    let _admission = service.reserve_operation(Kind::Maintenance)?;
    if recent.is_some() {
        // Consume the automatic copy only after admission. An unsuccessful launch then lets
        // the user choose another folder, while a busy recording preserves the choice.
        let mut current = service
            .protected_profile_copy
            .lock()
            .map_err(|_| "Protected profile state is unavailable")?;
        if *current != recent {
            return Err("Die Profilauswahl hat sich geändert. Bitte erneut versuchen. / The selected profile changed. Please try again.".into());
        }
        current.take();
    }
    launch(path)?;
    Ok(true)
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn required_passphrase(passphrase: Option<&Zeroizing<String>>) -> AppResult<&str> {
    passphrase
        .map(|value| value.as_str())
        .filter(|value| !value.is_empty() && value.len() <= 1024 && !value.contains(['\0', '\r', '\n']))
        .ok_or_else(|| "Gib eine gültige Passphrase für das Profil ein. / Enter a valid passphrase for this profile.".into())
}

fn launch(path: PathBuf, passphrase: Option<Zeroizing<String>>) -> AppResult<()> {
    use std::process::Command;
    const START_FAILED: &str = "Das geschützte Profil konnte nicht gestartet werden. / The protected profile could not start.";
    let mut command = Command::new(std::env::current_exe().map_err(|_| START_FAILED)?);
    command.arg("--protected-profile").arg(&path);
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        command.arg("--password-stdin");
        startup::launch(command, Some(required_passphrase(passphrase.as_ref())?))
    }
    #[cfg(windows)]
    {
        drop(passphrase);
        storage::verify(&path).map_err(|error| error.to_string())?;
        startup::launch(command, None)
    }
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        drop(passphrase);
        Err(START_FAILED.into())
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn unlock(path: &std::path::Path, managed: bool) -> AppResult<storage::MountedVolume> {
    let passphrase = startup::read_passphrase(std::io::stdin().lock(), managed)?;
    storage::open_volume(path, &passphrase).map_err(|error| error.to_string())
}

/// An explicit protected launch fails if its profile cannot be verified.
pub(crate) fn launch_path(args: &[OsString]) -> AppResult<Option<PathBuf>> {
    if args
        .first()
        .is_none_or(|flag| flag != "--protected-profile")
    {
        return Ok(None);
    }
    match args {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        [_, path, flag, rest @ ..] if flag == "--password-stdin"
            && (rest.is_empty() || matches!(rest, [ack] if ack == startup::FLAG))
            && std::path::Path::new(path).is_absolute() => Ok(Some(PathBuf::from(path))),
        #[cfg(windows)]
        [_, path, flag] if flag == startup::FLAG && std::path::Path::new(path).is_absolute() => {
            Ok(Some(PathBuf::from(path)))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        [_, path] if std::path::Path::new(path).is_absolute() => Ok(Some(PathBuf::from(path))),
        _ => Err("Open the encrypted profile from Settings or specify its absolute path and the required unlock method. / Verschlüsseltes Profil über die Einstellungen öffnen oder den absoluten Pfad samt Entsperrmethode angeben.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_profile::TestProfile;

    #[test]
    fn both_profile_pickers_allow_recording_and_recheck_before_working() {
        let fixture = TestProfile::new().unwrap();
        let service = Arc::new(Service::new(fixture.path().join("profile")).unwrap());
        let chosen = fixture.path().join("chosen-profile");
        let mut recording = None;
        let result = copy_after_selection(
            &service,
            || {
                recording = Some(service.reserve_operation(Kind::Recording).unwrap());
                Ok(Some(chosen.clone()))
            },
            |_, _| panic!("a recording started before the folder was selected"),
        );
        assert!(result.is_err());
        drop(recording.take());
        let result = open_after_selection(
            &service,
            || {
                recording = Some(service.reserve_operation(Kind::Recording).unwrap());
                Ok(Some(chosen.clone()))
            },
            |_| panic!("a recording started before the folder was selected"),
        );
        assert!(result.is_err());
        drop(recording.take());
        assert!(open_after_selection(&service, || Ok(None), |_| unreachable!()).is_ok());
    }

    #[test]
    fn migration_keeps_admission_until_cancellation_cleanup_finishes() {
        let fixture = TestProfile::new().unwrap();
        let service = Arc::new(Service::new(fixture.path().join("profile")).unwrap());
        let result = copy_after_selection(
            &service,
            || Ok(Some(fixture.path().to_owned())),
            |_, _| {
                assert!(matches!(
                    service.protected_profile_migration.state().unwrap(),
                    Some(MigrationState::Copying)
                ));
                assert!(service.protected_profile_migration.cancel().unwrap());
                assert!(matches!(
                    service.protected_profile_migration.state().unwrap(),
                    Some(MigrationState::Cancelling)
                ));
                assert!(service.reserve_operation(Kind::Recording).is_err());
                Err("synthetic cancellation cleanup complete".into())
            },
        );
        assert_eq!(
            result.err().unwrap(),
            "synthetic cancellation cleanup complete"
        );
        assert!(!service.protected_profile_migration.cancel().unwrap());
        assert!(service
            .protected_profile_migration
            .state()
            .unwrap()
            .is_none());
        assert!(service.reserve_operation(Kind::Recording).is_ok());
        assert!(service.protected_profile_copy.lock().unwrap().is_none());
    }

    #[test]
    fn an_automatic_copy_is_offered_once_even_without_a_successful_launch() {
        let fixture = TestProfile::new().unwrap();
        let service = Service::new(fixture.path().join("profile")).unwrap();
        let stale = fixture.path().join("moved-protected-copy");
        let recovered = fixture.path().join("available-protected-copy");
        *service.protected_profile_copy.lock().unwrap() = Some(stale.clone());
        let recording = service.reserve_operation(Kind::Recording).unwrap();
        assert!(open_after_selection(&service, || unreachable!(), |_| unreachable!()).is_err());
        drop(recording);
        let first = open_after_selection(
            &service,
            || unreachable!(),
            |selected| {
                assert_eq!(selected, stale);
                Err("synthetic missing copy".into())
            },
        );
        assert_eq!(first.unwrap_err(), "synthetic missing copy");
        assert!(open_after_selection(
            &service,
            || Ok(Some(recovered.clone())),
            |selected| {
                assert_eq!(selected, recovered);
                Ok(())
            }
        )
        .unwrap());
    }

    #[test]
    fn explicit_launch_requires_one_absolute_profile_path() {
        let flag = OsString::from("--protected-profile");
        let absolute = std::env::temp_dir().join("isolated-profile");
        let mut args = vec![flag.clone(), absolute.clone().into()];
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            args.push(OsString::from("--password-stdin"));
        }
        assert_eq!(
            launch_path(&args).unwrap().as_deref(),
            Some(absolute.as_path())
        );
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        {
            assert!(!managed_startup(&args));
            args.push(startup::FLAG.into());
            assert!(managed_startup(&args));
            assert_eq!(launch_path(&args).unwrap(), Some(absolute));
            args.push("unexpected".into());
            assert!(launch_path(&args).is_err());
        }
        assert!(launch_path(std::slice::from_ref(&flag)).is_err());
        assert!(launch_path(&[flag, OsString::from("relative")]).is_err());
        assert_eq!(
            launch_path(&[OsString::from("--agent"), OsString::from("serve")]).unwrap(),
            None
        );
    }
}
