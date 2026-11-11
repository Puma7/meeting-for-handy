//! Native folder choice and launch for a verified local profile.
use crate::{blocking, operations::Kind, AppResult, Service};
use meeting_core::archive::protected_profile as storage;
use serde::Serialize;
use std::{ffi::OsString, path::PathBuf, sync::Arc};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

#[derive(Serialize)]
pub(crate) struct Status {
    supported: bool,
    active: bool,
}

#[tauri::command]
pub(crate) fn protected_profile_status(service: State<'_, Arc<Service>>) -> AppResult<Status> {
    Ok(Status {
        supported: storage::supported(),
        active: service.protected_profile,
    })
}

#[tauri::command]
pub(crate) async fn create_protected_profile(
    service: State<'_, Arc<Service>>,
    app: AppHandle,
) -> AppResult<Option<storage::Receipt>> {
    let service = Arc::clone(service.inner());
    blocking(move || {
        if !storage::supported() || service.protected_profile {
            return Err("Ein neues EFS-Profil ist hier nicht verfügbar. / A new EFS profile is unavailable here.".into());
        }
        let _admission = crate::retention_commands::reserve(&service)?;
        let Some(parent) = app.dialog().file()
            .set_title("Ordner für das geschützte Profil / Parent folder for the protected profile")
            .blocking_pick_folder() else { return Ok(None); };
        let parent = parent.into_path().map_err(|_| "Choose a local folder.")?;
        let receipt = storage::migrate(&service.root, &parent).map_err(|error| error.to_string())?;
        *service.protected_profile_copy.lock().map_err(|_| "Protected profile state is unavailable")? = Some(receipt.path.clone());
        Ok(Some(receipt))
    }).await
}

#[tauri::command]
pub(crate) async fn open_protected_profile(
    service: State<'_, Arc<Service>>,
    app: AppHandle,
) -> AppResult<bool> {
    let service = Arc::clone(service.inner());
    blocking(move || {
        let _admission = service.reserve_operation(Kind::Maintenance)?;
        let recent = service.protected_profile_copy.lock()
            .map_err(|_| "Protected profile state is unavailable")?.clone();
        let path = match recent {
            Some(path) => path,
            None => {
                let Some(path) = app.dialog().file()
                    .set_title("Geschütztes Profil auswählen / Choose the protected profile")
                    .blocking_pick_folder() else { return Ok(false); };
                path.into_path().map_err(|_| "Choose a local folder.")?
            }
        };
        // Startup verifies it before opening any writers. The renderer never supplies a path.
        storage::verify(&path).map_err(|error| error.to_string())?;
        std::process::Command::new(std::env::current_exe().map_err(|error| error.to_string())?)
            .arg("--protected-profile")
            .arg(path)
            .spawn()
            .map_err(|_| "Das geschützte Profil konnte nicht gestartet werden. / The protected profile could not start.")?;
        *service.protected_profile_copy.lock().map_err(|_| "Protected profile state is unavailable")? = None;
        Ok(true)
    }).await
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
        [_, path] if std::path::Path::new(path).is_absolute() => Ok(Some(PathBuf::from(path))),
        _ => Err("Use --protected-profile with the absolute path to a migrated profile.".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_launch_requires_one_absolute_profile_path() {
        let flag = OsString::from("--protected-profile");
        let absolute = std::env::temp_dir().join("isolated-profile");
        assert_eq!(
            launch_path(&[flag.clone(), absolute.clone().into()]).unwrap(),
            Some(absolute)
        );
        assert!(launch_path(std::slice::from_ref(&flag)).is_err());
        assert!(launch_path(&[flag, OsString::from("relative")]).is_err());
        assert_eq!(
            launch_path(&[OsString::from("--agent"), OsString::from("serve")]).unwrap(),
            None
        );
    }
}
