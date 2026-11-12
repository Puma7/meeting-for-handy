//! Send macOS Quit through the protected window's recording and shutdown guards.

const QUIT_ID: &str = "quit-protected-profile";

#[cfg(target_os = "macos")]
pub(super) fn build(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};

    let menu = Menu::default(app)?;
    // The pinned Tauri default ends the application submenu with a native Quit
    // item, which calls AppKit terminate: without emitting CloseRequested.
    let unexpected_menu =
        || std::io::Error::other("Could not replace macOS Quit for the protected profile");
    let Some(MenuItemKind::Submenu(application)) = menu.items()?.into_iter().next() else {
        return Err(unexpected_menu().into());
    };
    let Some(MenuItemKind::Predefined(native_quit)) = application.items()?.pop() else {
        return Err(unexpected_menu().into());
    };
    let title = native_quit.text()?;
    if title != PredefinedMenuItem::quit(app, None)?.text()? {
        return Err(unexpected_menu().into());
    }
    let quit = MenuItem::with_id(app, QUIT_ID, title, true, Some("CmdOrCtrl+Q"))?;
    application.remove(&native_quit)?;
    application.append(&quit)?;
    Ok(menu)
}

#[cfg(target_os = "macos")]
pub(super) fn on_event(app: &tauri::AppHandle, event: tauri::menu::MenuEvent) {
    use tauri::Manager;

    route_quit(event.id().as_ref(), || {
        if let Some(window) = app
            .config()
            .app
            .windows
            .first()
            .and_then(|config| app.get_webview_window(&config.label))
        {
            // Tauri close emits CloseRequested, unlike destroy or native Quit.
            if let Err(error) = window.close() {
                eprintln!("Could not request protected window closure: {error}");
            }
        }
    });
}

fn route_quit(id: &str, close_window: impl FnOnce()) {
    if id == QUIT_ID {
        close_window();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_quit_dispatches_only_its_window_close_request() {
        route_quit("unrelated-menu-item", || {
            panic!("unrelated menu item closed the window")
        });
        let mut requested = false;
        route_quit(QUIT_ID, || requested = true);
        assert!(requested);
    }
}
