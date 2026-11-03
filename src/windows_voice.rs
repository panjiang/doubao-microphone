//! Version-guarded compatibility adapter using Doubao's existing microphone UI.
//! Only this agent's marked voice session may be stopped by emergency cleanup.
use anyhow::{Context, Result, ensure};
use std::time::{Duration, Instant};
use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HWND, LPARAM, RECT, WPARAM},
        Media::Audio::*,
        System::{Com::*, Threading::*},
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{BOOL, Interface, PCWSTR},
};

const BAR: &str = "OimeDirectUIWindow";
const WAVE: &str = "OimeVoiceWaveWindow";
struct DpiScope(DPI_AWARENESS_CONTEXT);
impl DpiScope {
    fn new() -> Self {
        Self(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE) })
    }
}
impl Drop for DpiScope {
    fn drop(&mut self) {
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}
#[derive(Clone)]
struct Window {
    hwnd: HWND,
    pid: u32,
    class: String,
    rect: RECT,
}
unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> BOOL {
    let windows = unsafe { &mut *(data.0 as *mut Vec<Window>) };
    let mut name = [0u16; 96];
    let n = unsafe { GetClassNameW(hwnd, &mut name) }.max(0) as usize;
    let class = String::from_utf16_lossy(&name[..n]);
    if class == BAR || class == WAVE {
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) }.is_ok() {
            windows.push(Window {
                hwnd,
                pid,
                class,
                rect,
            });
        }
    }
    true.into()
}
fn windows() -> Result<Vec<Window>> {
    let mut windows = Vec::new();
    unsafe { EnumWindows(Some(collect), LPARAM(&mut windows as *mut _ as isize))? };
    Ok(windows)
}
fn trusted_process(pid: u32) -> Result<()> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid)?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_FORMAT(0),
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result?;
        let path = String::from_utf16_lossy(&buffer[..size as usize]).to_lowercase();
        let expected = format!(
            "{}\\doubaoime\\versions\\v1.0.0.23\\imeservice.exe",
            std::env::var("ProgramFiles")?.to_lowercase()
        );
        ensure!(
            path == expected,
            "豆包兼容模式目前仅验证官方默认安装位置的 Windows 版 1.0.0.23"
        );
    }
    Ok(())
}
fn bar_for(windows: &[Window], pid: Option<u32>) -> Result<Window> {
    let mut candidates = windows.iter().filter(|w| {
        w.class == BAR
            && pid.is_none_or(|pid| w.pid == pid)
            && w.rect.right - w.rect.left == 156
            && w.rect.bottom - w.rect.top == 45
    });
    let bar = candidates
        .next()
        .context("找不到豆包悬浮工具栏，请在豆包设置中开启展示工具栏")?;
    ensure!(
        candidates.next().is_none(),
        "找到多个豆包工具栏，无法安全确定目标"
    );
    trusted_process(bar.pid)?;
    Ok(bar.clone())
}
fn property(owner: &str) -> Result<Vec<u16>> {
    uuid::Uuid::parse_str(owner)?;
    Ok(format!("DoubaoPTT.VoiceOwner.{owner}")
        .encode_utf16()
        .chain(Some(0))
        .collect())
}
fn microphone_click(bar: &Window) -> Result<()> {
    // The installed version's 156x45 layout places the mic at (54,20).
    // Keep the same DPI context used when resolving this layout; no cursor warp.
    let point = LPARAM(54 | (20 << 16));
    unsafe {
        if !IsWindowVisible(bar.hwnd).as_bool() {
            let _ = ShowWindow(bar.hwnd, SW_SHOWNOACTIVATE);
        }
        for (message, key) in [(WM_MOUSEMOVE, 0), (WM_LBUTTONDOWN, 1), (WM_LBUTTONUP, 0)] {
            PostMessageW(Some(bar.hwnd), message, WPARAM(key), point)?;
        }
    }
    Ok(())
}
fn capture_state(pid: u32) -> Result<(bool, u32)> {
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        ensure!(
            initialized.is_ok() || initialized.0 as u32 == 0x80010106,
            "无法初始化音频会话查询"
        );
        let result = (|| -> Result<(bool, u32)> {
            let mut active = false;
            let mut count = 0;
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let devices = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE)?;
            for n in 0..devices.GetCount()? {
                let device = devices.Item(n)?;
                let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                let sessions = manager.GetSessionEnumerator()?;
                for i in 0..sessions.GetCount()? {
                    let session = sessions.GetSession(i)?;
                    let details: IAudioSessionControl2 = session.cast()?;
                    if details.GetProcessId()? == pid {
                        count += 1;
                        active |= session.GetState()? == AudioSessionStateActive;
                    }
                }
            }
            Ok((active, count))
        })();
        if initialized.is_ok() {
            CoUninitialize();
        }
        result
    }
}
fn recording(pid: u32) -> Result<bool> {
    Ok(capture_state(pid)?.0)
}
pub(crate) fn start(owner: &str) -> Result<()> {
    start_configured(owner, false).map(|_| ())
}
pub(crate) fn start_configured(owner: &str, codex_empty_compatibility: bool) -> Result<bool> {
    let _dpi = DpiScope::new();
    let property = property(owner)?;
    let snapshot = windows()?;
    let bar = bar_for(&snapshot, None)?;
    ensure!(!recording(bar.pid)?, "豆包已经在录音，请先结束当前语音输入");
    let primed = codex_empty_compatibility && crate::codex_input::prime_if_empty()?;
    let panels: Vec<_> = snapshot
        .iter()
        .filter(|w| w.pid == bar.pid && w.class == WAVE)
        .collect();
    ensure!(
        panels.len() == 1,
        "豆包语音面板尚未初始化，请重新打开豆包后重试"
    );
    let panel = panels[0];
    unsafe {
        let shown = if IsWindowVisible(bar.hwnd).as_bool() {
            1
        } else {
            2
        };
        SetPropW(
            bar.hwnd,
            PCWSTR(property.as_ptr()),
            Some(HANDLE(shown as *mut _)),
        )?;
        SetPropW(
            panel.hwnd,
            PCWSTR(property.as_ptr()),
            Some(HANDLE(1usize as *mut _)),
        )?;
    }
    microphone_click(&bar)?;
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        if recording(bar.pid)? {
            return Ok(primed);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    anyhow::bail!("豆包语音未进入录音状态，请查看豆包提示")
}
pub(crate) fn release(owner: &str) -> Result<()> {
    let _dpi = DpiScope::new();
    let property = property(owner)?;
    let snapshot = windows()?;
    let panel = snapshot.iter().find(|w| {
        w.class == WAVE && unsafe { !GetPropW(w.hwnd, PCWSTR(property.as_ptr())).is_invalid() }
    });
    if let Some(panel) = panel {
        trusted_process(panel.pid)?;
        if recording(panel.pid)? {
            let bar = bar_for(&snapshot, Some(panel.pid))?;
            microphone_click(&bar)?;
            let deadline = Instant::now() + Duration::from_millis(200);
            while Instant::now() < deadline {
                if !recording(panel.pid)? {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            ensure!(
                !recording(panel.pid)?,
                "豆包仍在录音，请手动结束；已停止发送手机声音"
            );
        }
    }
    for window in snapshot {
        unsafe {
            let marker = GetPropW(window.hwnd, PCWSTR(property.as_ptr()));
            if !marker.is_invalid() {
                if window.class == BAR && marker.0 as usize == 2 {
                    let _ = ShowWindow(window.hwnd, SW_HIDE);
                }
                let _ = RemovePropW(window.hwnd, PCWSTR(property.as_ptr()));
            }
        }
    }
    Ok(())
}

pub(crate) fn state(owner: &str) -> Result<crate::platform::VoiceState> {
    let property = property(owner)?;
    let snapshot = windows()?;
    let panel = snapshot.iter().find(|w| {
        w.class == WAVE && unsafe { !GetPropW(w.hwnd, PCWSTR(property.as_ptr())).is_invalid() }
    });
    if let Some(panel) = panel {
        trusted_process(panel.pid)?;
        let (recording, capture_sessions) = capture_state(panel.pid)?;
        return Ok(crate::platform::VoiceState {
            recording,
            panel_visible: unsafe { IsWindowVisible(panel.hwnd) }.as_bool(),
            capture_sessions,
        });
    }
    Ok(crate::platform::VoiceState {
        recording: false,
        panel_visible: false,
        capture_sessions: 0,
    })
}
