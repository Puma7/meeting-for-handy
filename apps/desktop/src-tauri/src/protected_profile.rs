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

#[derive(Serialize)]
pub(crate) struct Status {
    supported: bool,
    active: bool,
    provider: Option<&'static str>,
}

#[tauri::command]
pub(crate) fn protected_profile_status(service: State<'_, Arc<Service>>) -> AppResult<Status> {
    Ok(Status {
        supported: storage::supported(),
        active: service.protected_profile,
        provider: storage::provider_name(),
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
        if !storage::supported() || service.protected_profile {
            return Err("Ein neues verschlüsseltes Profil ist hier nicht verfügbar. / A new encrypted profile is unavailable here.".into());
        }
        let _admission = crate::retention_commands::reserve(&service)?;
        let Some(parent) = app.dialog().file()
            .set_title("Ordner für das geschützte Profil / Parent folder for the protected profile")
            .blocking_pick_folder() else { return Ok(None); };
        let parent = parent.into_path().map_err(|_| "Choose a local folder.")?;
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        let receipt = storage::migrate_volume(&service.root, &parent, required_passphrase(passphrase.as_ref())?);
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let receipt = {
            drop(passphrase);
            storage::migrate(&service.root, &parent)
        };
        let receipt = receipt.map_err(|error| error.to_string())?;
        *service.protected_profile_copy.lock().map_err(|_| "Protected profile state is unavailable")? = Some(receipt.path.clone());
        Ok(Some(receipt))
    }).await
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
        let _admission = service.reserve_operation(Kind::Maintenance)?;
        let Some(path) = profile_to_open(&service.protected_profile_copy, || {
            let Some(path) = app
                .dialog()
                .file()
                .set_title("Geschütztes Profil auswählen / Choose the protected profile")
                .blocking_pick_folder()
            else {
                return Ok(None);
            };
            Ok(Some(
                path.into_path().map_err(|_| "Choose a local folder.")?,
            ))
        })?
        else {
            return Ok(false);
        };
        launch(path, passphrase)?;
        Ok(true)
    })
    .await
}

fn profile_to_open(
    recent_copy: &Mutex<Option<PathBuf>>,
    choose: impl FnOnce() -> AppResult<Option<PathBuf>>,
) -> AppResult<Option<PathBuf>> {
    // Offer the newly created copy once. A failed launch must let the user choose again.
    let recent = recent_copy
        .lock()
        .map_err(|_| "Protected profile state is unavailable")?
        .take();
    match recent {
        Some(path) => Ok(Some(path)),
        None => choose(),
    }
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

    #[test]
    fn an_automatic_copy_is_offered_once_even_without_a_successful_launch() {
        let stale = PathBuf::from("moved-protected-copy");
        let recovered = PathBuf::from("available-protected-copy");
        let recent = Mutex::new(Some(stale.clone()));
        let first = profile_to_open(&recent, || panic!("only offer the automatic copy"));
        assert_eq!(first.unwrap(), Some(stale));
        assert_eq!(
            profile_to_open(&recent, || Ok(Some(recovered.clone()))).unwrap(),
            Some(recovered)
        );
        assert_eq!(profile_to_open(&recent, || Ok(None)).unwrap(), None);
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
