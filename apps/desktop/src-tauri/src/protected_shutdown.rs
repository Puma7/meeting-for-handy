//! Keep the event loop alive until protected storage is detached or the user chooses to exit.

use crate::{operations, AppResult, Service};
use std::{path::PathBuf, sync::Arc};
use tauri::{AppHandle, Manager, RunEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const UNAVAILABLE: &str =
    "The protected profile is unavailable. / Das geschützte Profil ist nicht verfügbar.";

#[derive(Default)]
pub(super) struct Shutdown {
    admission: Option<operations::Lease>,
    started: bool,
    exit: Option<Exit>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Exit {
    Detached,
    LeaveMounted,
}

pub(super) fn prepare(service: &Service) -> AppResult<()> {
    let mut shutdown = service.protected_shutdown.lock().map_err(|_| UNAVAILABLE)?;
    if shutdown.admission.is_none() {
        let admission = service.reserve_operation(operations::Kind::Maintenance)?;
        service.lock()?.maintenance_reserved = true;
        shutdown.admission = Some(admission);
    }
    Ok(())
}

pub(super) fn leaving_mounted(service: &Service) -> bool {
    service
        .protected_shutdown
        .lock()
        .is_ok_and(|shutdown| shutdown.exit == Some(Exit::LeaveMounted))
}

pub(super) fn on_run_event(service: &Arc<Service>, app: &AppHandle, event: RunEvent) {
    let RunEvent::ExitRequested { api, code, .. } = event else {
        return;
    };
    let mut shutdown = service
        .protected_shutdown
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if shutdown.exit.is_some() {
        return;
    }
    api.prevent_exit();
    if shutdown.started {
        return;
    }
    if let Some(window) = code.and_then(|_| app.webview_windows().into_values().next()) {
        // Programmatic exit must also pass the recording and operation guards.
        drop(shutdown);
        let _ = window.close();
        return;
    }
    shutdown.started = true;
    drop(shutdown);
    detach(Arc::clone(service), app.clone(), code.unwrap_or(0));
}

fn detach(service: Arc<Service>, app: AppHandle, exit_code: i32) {
    tauri::async_runtime::spawn_blocking(move || match try_detach(&service) {
        Ok(()) => finish(&service, &app, exit_code, Exit::Detached),
        Err(error) => {
            let location = service
                .protected_volume
                .lock()
                .ok()
                .and_then(|volume| volume.as_ref()?.root().parent().map(PathBuf::from));
            let message = warning(&error, location);
            let next_app = app.clone();
            app.dialog()
                .message(message)
                .title("Meeting for Handy")
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::OkCancelCustom(
                    "Erneut versuchen / Retry".into(),
                    "Entsperrt beenden / Quit without unmounting".into(),
                ))
                .show(move |retry| {
                    if retry {
                        detach(service, next_app, exit_code);
                    } else {
                        finish(&service, &next_app, exit_code, Exit::LeaveMounted);
                    }
                });
        }
    });
}

fn try_detach(service: &Service) -> AppResult<()> {
    prepare(service)?;
    // Taking the last cached SessionStore releases its file leases and joins its text mirror.
    // The join may wait for disk I/O, so drop the store outside the application-state lock.
    let view = service.lock()?.view.take();
    drop(view);
    let mut protected = service.protected_volume.lock().map_err(|_| UNAVAILABLE)?;
    if let Some(volume) = protected.as_mut() {
        // The in-volume file handle would prevent a normal unmount. The outer vault lock
        // and operation admission stay held by this service throughout an unsuccessful retry.
        drop(
            service
                ._profile_lease
                .lock()
                .map_err(|_| UNAVAILABLE)?
                .take(),
        );
        volume.try_unmount().map_err(|error| error.to_string())?;
        *protected = None;
    }
    Ok(())
}

fn warning(error: &str, location: Option<PathBuf>) -> String {
    let recovery = if cfg!(target_os = "macos") {
        "Wirf bei Bedarf das Volume „HandyMeeting“ im Festplattendienstprogramm aus. / If needed, eject the HandyMeeting volume in Disk Utility."
    } else {
        "Hänge bei Bedarf das Volume mit deinem FUSE-Werkzeug aus. / If needed, unmount the volume with your system's FUSE tool."
    };
    let location = location.map_or_else(String::new, |path| format!("\n\n{}", path.display()));
    format!("{error}\n\nDas Profil kann weiterhin entsperrt sein. Schließe geöffnete Dateien aus dem Profil und versuche es erneut. Hänge das Profil vor dem Kopieren oder erneuten Öffnen aus.\n\nThe profile may still be unlocked. Close files opened from the profile and retry. Unmount it before copying or reopening it.\n\n{recovery}{location}")
}

fn finish(service: &Service, app: &AppHandle, code: i32, exit: Exit) {
    service
        .protected_shutdown
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .exit = Some(exit);
    app.exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_profile::TestProfile;

    #[test]
    fn preparing_shutdown_holds_admission_through_teardown() {
        let directory = TestProfile::new().unwrap();
        let service = Service::new(directory.path().to_owned()).unwrap();
        let existing = service.reserve_operation(operations::Kind::Index).unwrap();
        assert!(prepare(&service).is_err());
        drop(existing);
        prepare(&service).unwrap();
        prepare(&service).unwrap();
        assert!(service.reserve_operation(operations::Kind::Index).is_err());
        assert!(service
            .reserve_operation(operations::Kind::Recording)
            .is_err());
    }

    #[test]
    fn shutdown_releases_the_cached_session_and_its_mirror() {
        let directory = TestProfile::new().unwrap();
        let service = Service::new(directory.path().to_owned()).unwrap();
        let store = meeting_core::SessionStore::create(
            &service.root.join("sessions"),
            "Synthetic protected meeting",
            "synthetic-model",
            "Synthetic output",
            None,
        )
        .unwrap();
        let folder = store.folder().to_owned();
        let id = store.snapshot().unwrap().id;
        drop(store);
        service.data(&id).unwrap();
        assert!(meeting_core::SessionStore::open(&folder).is_err());
        service
            .lock()
            .unwrap()
            .view
            .as_ref()
            .unwrap()
            .set_status("complete")
            .unwrap();

        try_detach(&service).unwrap();

        assert!(folder.join("transcript.txt").is_file());
        assert!(meeting_core::SessionStore::open(&folder).is_ok());
        assert!(service.data(&id).is_err());
        assert!(service.lock().unwrap().view.is_none());
    }
}
