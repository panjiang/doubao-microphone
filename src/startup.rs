// Startup failures must remain visible when the desktop executable has no console.
pub fn show_error(reason: &str) {
    let message =
        format!("程序未能正常启动。\n\n原因：\n{reason}\n\n请根据以上原因处理后重新启动。");
    show_native_error(&message);
}

#[cfg(target_os = "windows")]
fn show_native_error(message: &str) {
    use windows::{
        Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW},
        core::{PCWSTR, w},
    };
    let message: Vec<u16> = message
        .replace('\0', "�")
        .encode_utf16()
        .chain(Some(0))
        .collect();
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            w!("豆包麦克风启动失败"),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

#[cfg(target_os = "macos")]
fn show_native_error(message: &str) {
    unsafe extern "C" {
        fn ptt_mac_show_startup_error(message: *const std::ffi::c_char);
    }
    let message =
        std::ffi::CString::new(message.replace('\0', "�")).expect("NUL characters replaced");
    unsafe {
        ptt_mac_show_startup_error(message.as_ptr());
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn show_native_error(_message: &str) {}
