//! Route macOS application termination through the document's ordinary save guard.
//!
//! GPUI 0.2.2 forwards `applicationWillTerminate:` after termination is irrevocable. Dock
//! Quit and logout need `applicationShouldTerminate:` instead: cancel that request, then let
//! the UI offer Save, Discard and Cancel. Only an explicitly approved exit may reach GPUI.

use gpui::App;

/// Bring a save question into view even when the native main window was minimized.
pub(crate) fn activate(window: &mut gpui::Window, cx: &mut App) {
    #[cfg(target_os = "macos")]
    if !cfg!(test) {
        // The headless GPUI platform cannot lend a native window handle.
        native::restore(window, cx.foreground_executor());
    }
    cx.activate(true);
    window.activate_window();
}

/// Finishes an exit that has already passed the document's save guard.
pub(crate) fn quit(cx: &mut App) {
    #[cfg(target_os = "macos")]
    native::GATE.approve();
    cx.quit();
}

/// Install the missing native delegate method after GPUI has created its application delegate.
pub(crate) fn install() {
    #[cfg(target_os = "macos")]
    native::install();
}

/// Consume a request from the native delegate on the next UI tick.
pub(crate) fn take_request() -> bool {
    #[cfg(target_os = "macos")]
    {
        native::GATE.take_request()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

#[cfg(any(target_os = "macos", test))]
#[derive(Default)]
struct ExitGate {
    requested: std::sync::atomic::AtomicBool,
    approved: std::sync::atomic::AtomicBool,
}

#[cfg(any(target_os = "macos", test))]
impl ExitGate {
    fn should_terminate(&self) -> bool {
        use std::sync::atomic::Ordering;
        if self.approved.load(Ordering::Relaxed) {
            return true;
        }
        self.requested.store(true, Ordering::Relaxed);
        false
    }

    fn take_request(&self) -> bool {
        self.requested
            .swap(false, std::sync::atomic::Ordering::Relaxed)
    }

    fn approve(&self) {
        self.approved
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(target_os = "macos")]
mod native {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{Encode, sel};
    use objc2_app_kit::{NSApplication, NSApplicationTerminateReply};
    use objc2_foundation::MainThreadMarker;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::ExitGate;

    /// Native requests and the UI's eventual permission to exit.
    pub(super) static GATE: ExitGate = ExitGate {
        requested: std::sync::atomic::AtomicBool::new(false),
        approved: std::sync::atomic::AtomicBool::new(false),
    };

    unsafe extern "C-unwind" fn should_terminate(
        _delegate: *mut AnyObject,
        _selector: Sel,
        _application: *mut NSApplication,
    ) -> NSApplicationTerminateReply {
        if GATE.should_terminate() {
            NSApplicationTerminateReply::TerminateNow
        } else {
            NSApplicationTerminateReply::TerminateCancel
        }
    }

    /// Add the veto callback to GPUI's existing delegate without replacing its other methods.
    pub(super) fn install() {
        let mtm = MainThreadMarker::new().expect("the desktop starts on the main thread");
        let application = NSApplication::sharedApplication(mtm);
        let delegate = application
            .delegate()
            .expect("GPUI has installed its delegate");
        let object: &AnyObject = (*delegate).as_ref();
        add_guard(object.class());
        // Refresh AppKit's cached optional-delegate-method capabilities.
        application.setDelegate(Some(&delegate));
    }

    /// Unminimize the document window before its save question is activated.
    pub(super) fn restore(window: &gpui::Window, executor: &gpui::ForegroundExecutor) {
        let Ok(handle) = HasWindowHandle::window_handle(window) else {
            return;
        };
        let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
            return;
        };
        // SAFETY: GPUI lends its live NSView for the lifetime of this window borrow. Native
        // window operations run on the same main thread and do not retain that borrowed view.
        let view = unsafe { handle.ns_view.cast::<objc2_app_kit::NSView>().as_ref() };
        if let Some(native) = view.window() {
            // Like GPUI's activation, defer AppKit callbacks until the current document and
            // App borrows have ended. Deminiaturization can synchronously notify its delegate.
            executor
                .spawn(async move {
                    if native.isMiniaturized() {
                        native.deminiaturize(None);
                    }
                })
                .detach();
        }
    }

    fn add_guard(class: &AnyClass) {
        let selector = sel!(applicationShouldTerminate:);
        // The pinned GPUI delegate has no veto callback. Never replace an upstream method
        // silently if a future GPUI release supplies one of its own.
        assert!(class.instance_method(selector).is_none());
        let encoding =
            std::ffi::CString::new(format!("{}@:@", NSApplicationTerminateReply::ENCODING))
                .expect("Objective-C type encodings contain no NUL");
        // SAFETY: the function has the selector's exact NSUInteger (id, SEL, id) ABI.
        // Objective-C erases function signatures to Imp when registering a method. The
        // runtime copies the encoding and keeps the process-lifetime function pointer.
        let added = unsafe {
            let implementation = std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    *mut AnyObject,
                    Sel,
                    *mut NSApplication,
                ) -> NSApplicationTerminateReply,
                Imp,
            >(should_terminate);
            objc2::ffi::class_addMethod(
                class as *const AnyClass as *mut AnyClass,
                selector,
                implementation,
                encoding.as_ptr(),
            )
        };
        assert!(added.as_bool(), "could not install the native save guard");
    }

    #[test]
    fn the_native_delegate_method_has_the_declared_return_type() {
        // Other window tests can approve their simulated exits. Keep the real Objective-C
        // callback's process-global gate isolated from those concurrent test windows.
        const CHILD: &str = "AURIS_TEST_TERMINATION_GATE";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "termination::native::the_native_delegate_method_has_the_declared_return_type",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        use objc2::runtime::ClassBuilder;
        let class =
            ClassBuilder::new(c"AurisTerminationTestDelegate", objc2::class!(NSObject)).unwrap();
        // Register a separate class so the test never modifies GPUI's delegate or exits.
        let class = class.register();
        add_guard(class);
        let method = class
            .instance_method(sel!(applicationShouldTerminate:))
            .unwrap();
        assert!(
            NSApplicationTerminateReply::ENCODING
                .equivalent_to_str(method.return_type().to_str().unwrap())
        );
        // SAFETY: this registered NSObject subclass inherits `new`; the installed method
        // accepts a nullable application pointer and reads only its atomic exit gate.
        unsafe {
            let object: objc2::rc::Retained<AnyObject> = objc2::msg_send![class, new];
            let request = || -> NSApplicationTerminateReply {
                objc2::msg_send![&*object, applicationShouldTerminate: std::ptr::null::<NSApplication>()]
            };
            assert_eq!(request(), NSApplicationTerminateReply::TerminateCancel);
            assert!(GATE.take_request());
            assert_eq!(request(), NSApplicationTerminateReply::TerminateCancel);
            assert!(GATE.take_request());
            GATE.approve();
            assert_eq!(request(), NSApplicationTerminateReply::TerminateNow);
            assert!(!GATE.take_request());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ExitGate;

    #[test]
    fn native_requests_wait_for_save_or_discard_and_cancel_does_not_approve() {
        let gate = ExitGate::default();
        assert!(!gate.should_terminate());
        assert!(gate.take_request());
        assert!(!gate.take_request());
        // Cancel leaves the next Dock quit guarded as well.
        assert!(!gate.should_terminate());
        assert!(gate.take_request());
        gate.approve();
        assert!(gate.should_terminate());
        assert!(!gate.take_request());
    }
}
