//! Fixed Enter action; never switches focus or reads editor text.
use anyhow::Result;

#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
pub(crate) struct Target {
    window: isize,
    focus: isize,
    process: u32,
    thread: u32,
}

#[cfg(target_os = "macos")]
pub(crate) use crate::macos::EnterTarget as Target;
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) struct Target;

pub(crate) fn supported() -> bool {
    cfg!(any(target_os = "windows", target_os = "macos"))
}

#[cfg(target_os = "windows")]
pub(crate) fn capture() -> Option<Target> {
    use windows::Win32::UI::WindowsAndMessaging::*;
    unsafe {
        let window = GetForegroundWindow();
        if window.0.is_null() {
            return None;
        }
        let mut process = 0;
        let thread = GetWindowThreadProcessId(window, Some(&mut process));
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if thread == 0
            || process == 0
            || GetGUIThreadInfo(thread, &mut info).is_err()
            || info.hwndFocus.0.is_null()
            || GetForegroundWindow() != window
        {
            return None;
        }
        Some(Target {
            window: window.0 as isize,
            focus: info.hwndFocus.0 as isize,
            process,
            thread,
        })
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn capture() -> Option<Target> {
    crate::macos::capture_enter_target()
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn capture() -> Option<Target> {
    None
}

// None means the down/up pair was accepted; a reason means deliberately skipped.
#[cfg(target_os = "windows")]
pub(crate) fn tap(target: Option<Target>) -> Result<Option<&'static str>> {
    use anyhow::ensure;
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let Some(target) = target else {
        return Ok(Some("target_unavailable"));
    };
    let Some(now) = capture() else {
        return Ok(Some("focus_changed"));
    };
    if (target.window, target.focus, target.process, target.thread)
        != (now.window, now.focus, now.process, now.thread)
    {
        return Ok(Some("focus_changed"));
    }
    for key in [
        VK_LMENU,
        VK_RMENU,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LWIN,
        VK_RWIN,
        VK_RETURN,
    ] {
        if unsafe { GetAsyncKeyState(key.0 as i32) } < 0 {
            return Ok(Some("key_held"));
        }
    }
    let input = |up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: 0x1c,
                dwFlags: if up {
                    KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_SCANCODE
                },
                time: 0,
                dwExtraInfo: 0x50505431,
            },
        },
    };
    let size = std::mem::size_of::<INPUT>() as i32;
    let accepted = unsafe { SendInput(&[input(false), input(true)], size) };
    if accepted == 1 {
        unsafe {
            SendInput(&[input(true)], size);
        }
    }
    ensure!(accepted == 2, "Windows 未完整接受 Enter；本次不重试");
    Ok(None)
}

#[cfg(target_os = "macos")]
pub(crate) fn tap(target: Option<Target>) -> Result<Option<&'static str>> {
    crate::macos::tap_enter(target)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub(crate) fn tap(_target: Option<Target>) -> Result<Option<&'static str>> {
    Ok(Some("unsupported"))
}
