//! Opt-in native VST3 window lifecycle checks with an installed plugin.

#![cfg(target_os = "windows")]

use std::path::PathBuf;

use auris_session::{PluginWindow, Session, SessionOptions};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GetClassNameW, IsWindow, SendMessageW, WM_CLOSE,
};
use windows::core::BOOL;

fn editor_window() -> HWND {
    unsafe extern "system" fn collect(hwnd: HWND, context: LPARAM) -> BOOL {
        let mut class = [0_u16; 64];
        // SAFETY: Windows supplies a live window and the buffer is writable for this call.
        let length = unsafe { GetClassNameW(hwnd, &mut class) } as usize;
        if String::from_utf16_lossy(&class[..length]) == "VST3PluginWindow" {
            // SAFETY: EnumThreadWindows calls synchronously with the Vec borrowed below.
            unsafe { &mut *(context.0 as *mut Vec<HWND>) }.push(hwnd);
        }
        true.into()
    }

    let mut windows = Vec::<HWND>::new();
    // SAFETY: The callback only writes to this Vec while the synchronous enumeration runs.
    // Restricting it to this test's thread avoids touching editors in other tests or apps.
    unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(collect),
            LPARAM(&mut windows as *mut Vec<HWND> as isize),
        )
        .ok()
        .unwrap();
    }
    assert_eq!(windows.len(), 1, "expected one editor on the test thread");
    windows[0]
}

fn session_with_editor() -> (Session, PluginWindow) {
    let path = PathBuf::from(std::env::var_os("AURIS_VST3_SMOKE_PLUGIN").unwrap());
    let mut session =
        Session::new(SessionOptions::headless().with_plugin_paths(vec![path.clone()])).unwrap();
    let info = session
        .vst3_plugins_in(&path)
        .unwrap()
        .into_iter()
        .find(|info| info.has_gui)
        .expect("the bundle must expose a native editor");
    let track = session.add_default_instrument_track("Window test").unwrap();
    session
        .set_vst3_instrument(track, &path, &info.class_id)
        .unwrap();
    session.poll();
    (session, PluginWindow::Instrument(track))
}

#[test]
#[ignore = "requires AURIS_VST3_SMOKE_PLUGIN to name an installed VST3 with a native editor"]
fn session_poll_destroys_a_dismissed_vst3_window_and_allows_reopening() {
    let (mut session, which) = session_with_editor();

    for _ in 0..2 {
        session.set_plugin_window_open(which, true).unwrap();
        let hwnd = editor_window();
        assert!(session.plugin_window_is_open(which));
        // SAFETY: This is this test's own editor on the current thread. WM_CLOSE is the same
        // request sent by the title-bar close button; no user-owned window is targeted.
        unsafe { SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0))) };
        session.poll();
        assert!(!session.plugin_window_is_open(which));
        // The host's logical flag alone is insufficient: Windows must release the real window.
        // SAFETY: IsWindow explicitly accepts handles whose windows have been destroyed.
        assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    }
}

#[test]
#[ignore = "requires AURIS_VST3_SMOKE_PLUGIN to name an installed VST3 with a native editor"]
fn a_dismissed_vst3_editor_can_reopen_before_the_next_poll() {
    let (mut session, which) = session_with_editor();
    session.set_plugin_window_open(which, true).unwrap();
    let hwnd = editor_window();
    // SAFETY: This test owns the editor and dispatches on the thread that created it.
    unsafe { SendMessageW(hwnd, WM_CLOSE, Some(WPARAM(0)), Some(LPARAM(0))) };
    session.set_plugin_window_open(which, true).unwrap();
    let reopened = editor_window();
    session.poll();
    assert!(session.plugin_window_is_open(which));
    session.set_plugin_window_open(which, false).unwrap();
    // SAFETY: IsWindow accepts destroyed handles, including this test's closed editor.
    assert!(!unsafe { IsWindow(Some(reopened)) }.as_bool());
}
