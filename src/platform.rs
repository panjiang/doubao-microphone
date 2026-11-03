use anyhow::{Result, bail, ensure};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shortcut {
    DoubaoVoice,
    HidRightAlt,
    HidRightControl,
    LeftAlt,
    RightAlt,
    LeftControl,
    RightControl,
    LeftCommand,
    RightCommand,
    Fn,
}

pub fn supported(key: Shortcut) -> bool {
    if cfg!(target_os = "windows") {
        !matches!(
            key,
            Shortcut::Fn | Shortcut::LeftCommand | Shortcut::RightCommand
        )
    } else {
        cfg!(target_os = "macos")
            && !matches!(
                key,
                Shortcut::DoubaoVoice | Shortcut::HidRightAlt | Shortcut::HidRightControl
            )
    }
}

#[derive(Serialize)]
pub struct SetupStatus {
    pub platform: &'static str,
    pub native_supported: bool,
    pub accessibility: bool,
    pub shortcuts: Vec<Shortcut>,
}
pub fn setup_status() -> SetupStatus {
    SetupStatus {
        platform: std::env::consts::OS,
        native_supported: cfg!(any(target_os = "windows", target_os = "macos")),
        accessibility: accessibility(),
        shortcuts: [
            Shortcut::DoubaoVoice,
            Shortcut::HidRightAlt,
            Shortcut::HidRightControl,
            Shortcut::LeftAlt,
            Shortcut::RightAlt,
            Shortcut::LeftControl,
            Shortcut::RightControl,
            Shortcut::LeftCommand,
            Shortcut::RightCommand,
            Shortcut::Fn,
        ]
        .into_iter()
        .filter(|k| supported(*k))
        .collect(),
    }
}
pub fn accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        unsafe { AXIsProcessTrusted() }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub struct Keyboard {
    mock: bool,
    held: Option<Shortcut>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoiceState {
    pub recording: bool,
    pub panel_visible: bool,
    #[serde(default)]
    pub capture_sessions: u32,
}
impl Keyboard {
    pub fn new(mock: bool) -> Self {
        Self { mock, held: None }
    }
    pub fn press(&mut self, key: Shortcut) -> Result<()> {
        self.press_configured(key, false).map(|_| ())
    }
    pub fn press_configured(
        &mut self,
        key: Shortcut,
        codex_empty_compatibility: bool,
    ) -> Result<bool> {
        ensure!(self.held.is_none(), "shortcut already held");
        #[allow(unused_mut)]
        let mut primed = false;
        if !self.mock {
            ensure!(supported(key), "shortcut unsupported on this platform");
            ensure!(accessibility(), "请为应用及输入代理授予辅助功能权限");
            #[cfg(target_os = "windows")]
            if key == Shortcut::DoubaoVoice {
                primed = crate::windows_voice::start_configured(
                    &std::env::var("DOUBAO_PTT_VOICE_OWNER")?,
                    codex_empty_compatibility,
                )?;
            } else {
                native_key(key, true)?;
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = codex_empty_compatibility;
                native_key(key, true)?;
            }
        }
        self.held = Some(key);
        Ok(primed)
    }
    pub fn release(&mut self) -> Result<()> {
        if let Some(key) = self.held {
            if !self.mock {
                native_key(key, false)?;
            }
            self.held = None;
        }
        Ok(())
    }
    pub fn voice_state(&self) -> Result<Option<VoiceState>> {
        #[cfg(target_os = "windows")]
        if !self.mock
            && matches!(
                self.held,
                Some(Shortcut::HidRightAlt | Shortcut::HidRightControl)
            )
        {
            crate::windows_umdf::renew()?;
            return Ok(None);
        }
        #[cfg(target_os = "windows")]
        if !self.mock && self.held == Some(Shortcut::DoubaoVoice) {
            return crate::windows_voice::state(&std::env::var("DOUBAO_PTT_VOICE_OWNER")?)
                .map(Some);
        }
        Ok(None)
    }
    pub fn emergency(key: Shortcut, mock: bool) -> Result<()> {
        if !mock {
            native_key(key, false)?;
        }
        Ok(())
    }
    pub fn emergency_owned(key: Shortcut, mock: bool, owner: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        if key == Shortcut::DoubaoVoice {
            if !mock {
                return crate::windows_voice::release(owner);
            }
            return Ok(());
        }
        let _ = owner;
        Self::emergency(key, mock)
    }
}
impl Drop for Keyboard {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

#[cfg(target_os = "windows")]
fn native_key(key: Shortcut, down: bool) -> Result<()> {
    if matches!(key, Shortcut::HidRightAlt | Shortcut::HidRightControl) {
        return if down {
            crate::windows_umdf::press(if key == Shortcut::HidRightAlt { 1 } else { 2 })
        } else {
            crate::windows_umdf::release()
        };
    }
    if key == Shortcut::DoubaoVoice {
        let owner = std::env::var("DOUBAO_PTT_VOICE_OWNER")?;
        return if down {
            crate::windows_voice::start(&owner)
        } else {
            crate::windows_voice::release(&owner)
        };
    }
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    let vk = match key {
        Shortcut::LeftAlt => VK_LMENU,
        Shortcut::RightAlt => VK_RMENU,
        Shortcut::LeftControl => VK_LCONTROL,
        Shortcut::RightControl => VK_RCONTROL,
        _ => bail!("unsupported Windows shortcut"),
    };
    if down {
        for k in [
            VK_LMENU,
            VK_RMENU,
            VK_LCONTROL,
            VK_RCONTROL,
            VK_LSHIFT,
            VK_RSHIFT,
            VK_LWIN,
            VK_RWIN,
        ] {
            ensure!(
                unsafe { GetAsyncKeyState(k.0 as i32) } >= 0,
                "请先松开电脑上的修饰键"
            );
        }
    }
    // Supply the physical scan code as well as the extended-key flag. A VK-only
    // event with scan code zero is not equivalent to a physical modifier event
    // for applications that inspect keyboard hook / key-message scan codes.
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC_EX) };
    ensure!(scan != 0, "Windows 无法映射快捷键扫描码");
    let mut flags = KEYEVENTF_SCANCODE;
    if !down {
        flags |= KEYEVENTF_KEYUP;
    }
    if scan & 0xff00 == 0xe000 {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: (scan & 0xff) as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0x50505431,
            },
        },
    };
    ensure!(
        unsafe { SendInput(&[input], std::mem::size_of::<INPUT>() as i32) } == 1,
        "Windows 拒绝模拟输入；检查权限和桌面会话"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn CGEventCreateKeyboardEvent(
        source: *const std::ffi::c_void,
        key: u16,
        down: bool,
    ) -> *mut std::ffi::c_void;
    fn CGEventSetFlags(event: *mut std::ffi::c_void, flags: u64);
    fn CGEventSetType(event: *mut std::ffi::c_void, kind: u32);
    fn CGEventPost(location: u32, event: *mut std::ffi::c_void);
    fn CGEventSourceFlagsState(state: i32) -> u64;
}
#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const std::ffi::c_void);
}
#[cfg(target_os = "macos")]
fn native_key(key: Shortcut, down: bool) -> Result<()> {
    let (code, mask) = match key {
        Shortcut::DoubaoVoice | Shortcut::HidRightAlt | Shortcut::HidRightControl => {
            bail!("此输入方式仅支持 Windows")
        }
        Shortcut::Fn => (63, 1 << 23),
        Shortcut::LeftAlt => (58, 1 << 19),
        Shortcut::RightAlt => (61, 1 << 19),
        Shortcut::LeftControl => (59, 1 << 18),
        Shortcut::RightControl => (62, 1 << 18),
        Shortcut::LeftCommand => (55, 1 << 20),
        Shortcut::RightCommand => (54, 1 << 20),
    };
    unsafe {
        let flags = CGEventSourceFlagsState(0);
        if down {
            ensure!(
                flags & ((1 << 17) | (1 << 18) | (1 << 19) | (1 << 20) | (1 << 23)) == 0,
                "请先松开电脑上的修饰键"
            );
        }
        let e = CGEventCreateKeyboardEvent(std::ptr::null(), code, down);
        ensure!(!e.is_null(), "无法创建系统按键事件");
        CGEventSetType(e, 12);
        CGEventSetFlags(e, if down { flags | mask } else { flags & !mask });
        CGEventPost(0, e);
        CFRelease(e);
    }
    Ok(())
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn native_key(_key: Shortcut, _down: bool) -> Result<()> {
    bail!("native input is supported on Windows/macOS only; use --mock for development")
}

pub fn set_autostart(enabled: bool) -> Result<()> {
    let exe = std::env::current_exe()?;
    #[cfg(target_os = "windows")]
    {
        let key = "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run";
        let status = if enabled {
            std::process::Command::new("reg.exe")
                .args([
                    "add",
                    key,
                    "/v",
                    "DoubaoPTT",
                    "/t",
                    "REG_SZ",
                    "/d",
                    &format!("\"{}\" serve", exe.display()),
                    "/f",
                ])
                .status()?
        } else {
            let out = std::process::Command::new("reg.exe")
                .args(["query", key, "/v", "DoubaoPTT"])
                .output()?;
            if !out.status.success() {
                return Ok(());
            }
            std::process::Command::new("reg.exe")
                .args(["delete", key, "/v", "DoubaoPTT", "/f"])
                .status()?
        };
        ensure!(status.success(), "无法修改当前用户登录启动项");
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME")?;
        let dir = std::path::PathBuf::from(home).join("Library/LaunchAgents");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("local.doubao-ptt.plist");
        if enabled {
            let e = exe
                .to_string_lossy()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            let xml = format!(
                "<?xml version=\"1.0\"?><!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\"><plist version=\"1.0\"><dict><key>Label</key><string>local.doubao-ptt</string><key>ProgramArguments</key><array><string>{e}</string><string>serve</string></array><key>RunAtLoad</key><true/></dict></plist>"
            );
            crate::config::atomic_write(&path, xml.as_bytes())?;
        } else if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (enabled, exe);
        bail!("login startup is supported on Windows/macOS only")
    }
}
