//! Native folder choice and launch for a verified local profile.
use crate::{blocking, operations::Kind, AppResult, Service};
use meeting_core::archive::protected_profile as storage;
use serde::Serialize;
use std::{ffi::OsString, path::PathBuf, sync::Arc};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use zeroize::Zeroizing;

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
        let recent = service
            .protected_profile_copy
            .lock()
            .map_err(|_| "Protected profile state is unavailable")?
            .clone();
        let path = match recent {
            Some(path) => path,
            None => {
                let Some(path) = app
                    .dialog()
                    .file()
                    .set_title("Geschütztes Profil auswählen / Choose the protected profile")
                    .blocking_pick_folder()
                else {
                    return Ok(false);
                };
                path.into_path().map_err(|_| "Choose a local folder.")?
            }
        };
        launch(path, passphrase)?;
        *service
            .protected_profile_copy
            .lock()
            .map_err(|_| "Protected profile state is unavailable")? = None;
        Ok(true)
    })
    .await
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
        use std::{io::Write, process::Stdio};
        let passphrase = required_passphrase(passphrase.as_ref())?;
        let mut child = command
            .arg("--password-stdin")
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|_| START_FAILED)?;
        let written = child
            .stdin
            .take()
            .ok_or(START_FAILED)?
            .write_all(passphrase.as_bytes());
        if written.is_err() {
            let _ = child.wait();
            return Err(START_FAILED.into());
        }
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        drop(passphrase);
        storage::verify(&path).map_err(|error| error.to_string())?;
        command.spawn().map_err(|_| START_FAILED)?;
    }
    Ok(())
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub(crate) fn unlock(path: &std::path::Path) -> AppResult<storage::MountedVolume> {
    use std::io::Read;
    let mut passphrase = Zeroizing::new(String::new());
    std::io::stdin()
        .take(1025)
        .read_to_string(&mut passphrase)
        .map_err(|_| "The passphrase could not be read.")?;
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
        [_, path, flag] if flag == "--password-stdin" && std::path::Path::new(path).is_absolute() => Ok(Some(PathBuf::from(path))),
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        [_, path] if std::path::Path::new(path).is_absolute() => Ok(Some(PathBuf::from(path))),
        _ => Err("Open the encrypted profile from Settings or specify its absolute path and the required unlock method. / Verschlüsseltes Profil über die Einstellungen öffnen oder den absoluten Pfad samt Entsperrmethode angeben.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_launch_requires_one_absolute_profile_path() {
        let flag = OsString::from("--protected-profile");
        let absolute = std::env::temp_dir().join("isolated-profile");
        let mut args = vec![flag.clone(), absolute.clone().into()];
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            args.push(OsString::from("--password-stdin"));
        }
        assert_eq!(launch_path(&args).unwrap(), Some(absolute));
        assert!(launch_path(std::slice::from_ref(&flag)).is_err());
        assert!(launch_path(&[flag, OsString::from("relative")]).is_err());
        assert_eq!(
            launch_path(&[OsString::from("--agent"), OsString::from("serve")]).unwrap(),
            None
        );
    }
}
