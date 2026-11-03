//! Optional workaround for Codex's first write into an empty composer.
//! Inspect at most one character for emptiness; never retain input text.
use anyhow::{Context, Result, ensure};
use windows::{
    Win32::{
        Foundation::*,
        System::{Com::*, Threading::*},
        UI::{Accessibility::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::Interface,
};

struct ComScope(bool);
impl ComScope {
    fn new() -> Result<Self> {
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        ensure!(
            hr.is_ok() || hr.0 as u32 == 0x80010106,
            "无法初始化 Codex 兼容检查"
        );
        Ok(Self(hr.is_ok()))
    }
}
impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            unsafe {
                CoUninitialize();
            }
        }
    }
}

fn is_codex(pid: u32) -> Result<bool> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let mut buffer = [0u16; 1024];
        let mut n = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            p,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut n,
        );
        let _ = CloseHandle(p);
        result?;
        let name = String::from_utf16_lossy(&buffer[..n as usize]).to_lowercase();
        let prefix = format!(
            "{}\\windowsapps\\openai.codex_",
            std::env::var("ProgramFiles")?.to_lowercase()
        );
        Ok(name.starts_with(&prefix)
            && (name.ends_with("\\app\\chatgpt.exe") || name.ends_with("\\app\\codex.exe")))
    }
}
fn empty_editor(automation: &IUIAutomation) -> Result<Option<bool>> {
    unsafe {
        let e = automation.GetFocusedElement()?;
        if e.CurrentControlType()? != UIA_EditControlTypeId
            || !e.CurrentHasKeyboardFocus()?.as_bool()
            || !e.CurrentIsEnabled()?.as_bool()
            || !is_codex(e.CurrentProcessId()? as u32)?
        {
            return Ok(None);
        }
        let class = e.CurrentClassName()?.to_string();
        if !class.split_whitespace().any(|s| s == "ProseMirror") {
            return Ok(None);
        }
        let range = e
            .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)?
            .DocumentRange()?;
        let text = range.GetText(1)?.to_string();
        if text.is_empty() {
            return Ok(Some(true));
        }
        if !text.chars().all(|c| c == '\r' || c == '\n') {
            return Ok(Some(false));
        }
        // A trailing paragraph marker can represent an empty rich editor.
        // Compare range endpoints to avoid treating a leading blank line in
        // an otherwise nonempty document as an empty composer.
        range.MoveEndpointByUnit(TextPatternRangeEndpoint_Start, TextUnit_Character, 1)?;
        Ok(Some(
            range.CompareEndpoints(
                TextPatternRangeEndpoint_Start,
                &range,
                TextPatternRangeEndpoint_End,
            )? == 0,
        ))
    }
}
fn automation() -> Result<IUIAutomation> {
    unsafe {
        // The legacy CUIAutomation class does not expose IUIAutomation2 on
        // the supported Windows 11 installation. Use the Windows 8+ class.
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER)
                .context("无法创建 Codex 兼容检查组件")?;
        let limits = automation
            .cast::<IUIAutomation2>()
            .context("无法配置 Codex 兼容检查超时")?;
        limits.SetConnectionTimeout(150)?;
        limits.SetTransactionTimeout(150)?;
        Ok(automation)
    }
}
pub(crate) fn prime_if_empty() -> Result<bool> {
    unsafe {
        let foreground = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(foreground, Some(&mut pid));
        if pid == 0 || !is_codex(pid).unwrap_or(false) {
            return Ok(false);
        }
        let _com = ComScope::new()?;
        let automation = automation()?;
        let empty =
            empty_editor(&automation).context("无法检查 Codex 输入框，请先手动输入一个空格")?;
        if empty != Some(true) {
            return Ok(false);
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
        ] {
            ensure!(GetAsyncKeyState(key.0 as i32) >= 0, "请先松开电脑修饰键");
        }
        ensure!(
            GetForegroundWindow() == foreground,
            "电脑焦点已变化，请重新按住"
        );
        let event = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0),
                    wScan: 0x20,
                    dwFlags: if up {
                        KEYEVENTF_UNICODE | KEYEVENTF_KEYUP
                    } else {
                        KEYEVENTF_UNICODE
                    },
                    time: 0,
                    dwExtraInfo: 0x50505432,
                },
            },
        };
        let sent = SendInput(
            &[event(false), event(true)],
            std::mem::size_of::<INPUT>() as i32,
        );
        if sent == 1 {
            let _ = SendInput(&[event(true)], std::mem::size_of::<INPUT>() as i32);
        }
        ensure!(sent == 2, "无法预热 Codex 输入框，请先手动输入空格");
        // Allow the initial editor/IME transition to settle before opening voice.
        std::thread::sleep(std::time::Duration::from_millis(80));
        ensure!(
            GetForegroundWindow() == foreground,
            "电脑焦点已变化，请重新按住"
        );
        ensure!(
            empty_editor(&automation)? == Some(false),
            "Codex 输入框预热未确认，请先手动输入空格"
        );
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn actual_uia_factory_supports_required_timeout_interface() {
        let _com = super::ComScope::new().unwrap();
        super::automation().unwrap();
    }
}
