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
    route_quit(event.id().as_ref(), || request_window_close(app));
}

#[cfg(target_os = "macos")]
fn request_window_close(app: &tauri::AppHandle) {
    use tauri::Manager;

    if let Some(window) = app
        .config()
        .app
        .windows
        .first()
        .and_then(|config| app.get_webview_window(&config.label))
    {
        // Tauri queues CloseRequested on the event loop, including from an AppKit callback.
        if let Err(error) = window.close() {
            eprintln!("Could not request protected window closure: {error}");
        }
    }
}

fn route_quit(id: &str, close_window: impl FnOnce()) {
    if id == QUIT_ID {
        close_window();
    }
}

#[cfg(target_os = "macos")]
pub(super) use native::{clear_native_quit, install_native_quit};

#[cfg(target_os = "macos")]
mod native {
    use super::{request_window_close, route_native_quit};
    use std::cell::RefCell;
    use std::ffi::{c_char, c_int, c_void};

    type Object = *mut c_void;
    type Selector = *const c_void;
    #[cfg(target_arch = "aarch64")]
    type ObjcBool = bool;
    #[cfg(target_arch = "x86_64")]
    type ObjcBool = std::ffi::c_schar;

    thread_local! {
        static APP: RefCell<Option<tauri::AppHandle>> = const { RefCell::new(None) };
    }

    #[link(name = "objc")]
    extern "C" {
        fn objc_getClass(name: *const c_char) -> Object;
        fn object_getClass(object: Object) -> Object;
        fn sel_registerName(name: *const c_char) -> Selector;
        fn objc_msgSend(receiver: Object, selector: Selector) -> Object;
        fn class_getInstanceMethod(class: Object, selector: Selector) -> Object;
        fn class_addMethod(
            class: Object,
            selector: Selector,
            implementation: *const c_void,
            types: *const c_char,
        ) -> ObjcBool;
    }

    extern "C" {
        fn pthread_main_np() -> c_int;
    }

    pub(crate) fn install_native_quit(app: &tauri::AppHandle) -> tauri::Result<()> {
        let unavailable = || {
            std::io::Error::other(
                "Could not guard macOS Quit for the protected profile. / \
                 Beenden unter macOS konnte für das geschützte Profil nicht abgesichert werden.",
            )
        };
        // SAFETY: AppKit objects are used only on the main thread. Tao owns the live
        // delegate and keeps it retained across the setDelegate: calls below.
        unsafe {
            if pthread_main_np() == 0 {
                return Err(unavailable().into());
            }
            let application_class = objc_getClass(c"NSApplication".as_ptr());
            let expected_class = objc_getClass(c"TaoAppDelegateParent".as_ptr());
            if application_class.is_null() || expected_class.is_null() {
                return Err(unavailable().into());
            }
            let application = objc_msgSend(
                application_class,
                sel_registerName(c"sharedApplication".as_ptr()),
            );
            let delegate = objc_msgSend(application, sel_registerName(c"delegate".as_ptr()));
            if delegate.is_null() || object_getClass(delegate) != expected_class {
                return Err(unavailable().into());
            }
            // The pinned Tao delegate handles applicationWillTerminate: only. Add
            // the earlier AppKit hook to the protected process without replacing it.
            if !register_termination_hook(expected_class) {
                return Err(unavailable().into());
            }
            APP.with(|handle| handle.replace(Some(app.clone())));
            // AppKit caches which optional methods its delegate implements. Refresh
            // the registration after adding this method, with the exact setDelegate: ABI.
            let set_delegate: unsafe extern "C" fn(Object, Selector, Object) =
                std::mem::transmute(objc_msgSend as *const ());
            let setter = sel_registerName(c"setDelegate:".as_ptr());
            set_delegate(application, setter, std::ptr::null_mut());
            set_delegate(application, setter, delegate);
        }
        Ok(())
    }

    // SAFETY: callers must supply a live Objective-C class.
    unsafe fn register_termination_hook(class: Object) -> bool {
        let selector = sel_registerName(c"applicationShouldTerminate:".as_ptr());
        // Preserve any implementation the toolkit might add in a future upgrade.
        if !class_getInstanceMethod(class, selector).is_null() {
            return false;
        }
        // NSApplicationTerminateReply is NSUInteger on both macOS targets.
        class_addMethod(
            class,
            selector,
            application_should_terminate as *const c_void,
            c"Q@:@".as_ptr(),
        ) as u8
            != 0
    }

    pub(crate) fn clear_native_quit() {
        APP.with(|app| app.take());
    }

    extern "C" fn application_should_terminate(
        _delegate: Object,
        _selector: Selector,
        _application: Object,
    ) -> usize {
        route_native_quit(|| {
            // Never unwind through AppKit. During shutdown there may be no window,
            // in which case the in-progress detach remains in control of the exit.
            let _ = std::panic::catch_unwind(|| {
                let app = APP.try_with(|app| app.borrow().clone()).ok().flatten();
                if let Some(app) = app {
                    request_window_close(&app);
                }
            });
        })
    }

    #[cfg(test)]
    #[test]
    fn registers_native_termination_selector_without_replacing_it() {
        #[link(name = "objc")]
        extern "C" {
            fn objc_allocateClassPair(base: Object, name: *const c_char, extra: usize) -> Object;
            fn objc_registerClassPair(class: Object);
            fn objc_disposeClassPair(class: Object);
            fn method_getImplementation(method: Object) -> *const c_void;
        }

        // SAFETY: this test owns a temporary class with no instances or subclasses.
        unsafe {
            let base = objc_getClass(c"NSObject".as_ptr());
            assert!(!base.is_null());
            let class = objc_allocateClassPair(base, c"HandyMeetingProtectedQuitTest".as_ptr(), 0);
            assert!(!class.is_null());
            objc_registerClassPair(class);
            let installed = register_termination_hook(class);
            let replaced = register_termination_hook(class);
            let selector = sel_registerName(c"applicationShouldTerminate:".as_ptr());
            let method = class_getInstanceMethod(class, selector);
            let implementation = (!method.is_null()).then(|| method_getImplementation(method));
            objc_disposeClassPair(class);

            assert!(installed);
            assert!(!replaced);
            let callback: extern "C" fn(Object, Selector, Object) -> usize =
                std::mem::transmute(implementation.unwrap());
            // No window exists in this test. The hook must still cancel native Quit.
            assert_eq!(
                callback(std::ptr::null_mut(), selector, std::ptr::null_mut()),
                0
            );
        }
    }
}

#[cfg(any(target_os = "macos", test))]
fn route_native_quit(close_window: impl FnOnce()) -> usize {
    close_window();
    // NSTerminateCancel. The normal window shutdown path owns completion.
    0
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

    #[test]
    fn native_quit_requests_guarded_close_and_cancels_appkit_termination() {
        let mut close_requests = 0;
        assert_eq!(route_native_quit(|| close_requests += 1), 0);
        assert_eq!(close_requests, 1);
        assert_eq!(route_native_quit(|| {}), 0);
    }
}
