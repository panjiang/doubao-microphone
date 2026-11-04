//! HID feature transport to the separately installed UMDF P0 component.
//! No SendInput fallback. Agent and driver both hold independent leases.
use anyhow::{Context, Result, ensure};
use std::sync::Mutex;

unsafe extern "C" {
    fn ptt_umdf_open() -> isize;
    fn ptt_umdf_command(handle: isize, operation: u8, key: u32) -> u32;
    fn ptt_umdf_close(handle: isize);
}

struct Control(isize, u32);
impl Control {
    fn open() -> Result<Self> {
        let handle = unsafe { ptt_umdf_open() };
        ensure!(
            handle != -1 && handle != 0,
            "找不到 UMDF 键盘组件或无访问权限；请先安装组件并检查权限"
        );
        Ok(Self(handle, 0))
    }
    fn command(&self, operation: u8, key: u32) -> Result<()> {
        let error = unsafe { ptt_umdf_command(self.0, operation, key) };
        ensure!(error == 0, "UMDF 输入请求失败，Windows 错误码 {error}");
        Ok(())
    }
}
impl Drop for Control {
    fn drop(&mut self) {
        unsafe { ptt_umdf_close(self.0) };
    }
}

static CONTROL: Mutex<Option<Control>> = Mutex::new(None);
static LAST_RELEASE: Mutex<Option<(u32, std::time::Instant)>> = Mutex::new(None);

// Probe access only: no feature report, key event or lease operation is sent.
pub fn available() -> bool {
    Control::open().is_ok()
}

pub fn press(key: u32) -> Result<()> {
    ensure!(key == 1 || key == 2, "UMDF 只允许右 Alt 或右 Ctrl");
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    for modifier in [
        VK_LMENU,
        VK_RMENU,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_LWIN,
        VK_RWIN,
    ] {
        // A successful HID release queues the report; Windows may still expose
        // our previous key-down briefly. Wait only for a recently owned key,
        // without releasing or ignoring any physical modifier.
        if unsafe { GetAsyncKeyState(modifier.0 as i32) } < 0 {
            let recently_owned = LAST_RELEASE.lock().ok().and_then(|r| *r).is_some_and(|(key, at)| {
                at.elapsed() <= std::time::Duration::from_millis(500)
                    && ((key == 1 && modifier == VK_RMENU) || (key == 2 && modifier == VK_RCONTROL))
            });
            if recently_owned {
                let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
                while unsafe { GetAsyncKeyState(modifier.0 as i32) } < 0 && std::time::Instant::now() < deadline {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
        ensure!(unsafe { GetAsyncKeyState(modifier.0 as i32) } >= 0, "请先松开电脑上的修饰键");
    }
    let mut slot = CONTROL
        .lock()
        .map_err(|_| anyhow::anyhow!("UMDF 输入锁异常"))?;
    ensure!(slot.is_none(), "UMDF 输入已在进行中");
    let mut device = Control::open()?;
    if let Err(error) = device.command(1, key) {
        let _ = device.command(3, 0);
        return Err(error);
    }
    device.1 = key;
    *slot = Some(device);
    Ok(())
}

pub fn renew() -> Result<()> {
    let slot = CONTROL
        .lock()
        .map_err(|_| anyhow::anyhow!("UMDF 输入锁异常"))?;
    slot.as_ref().context("UMDF 输入租约不存在")?.command(2, 0)
}

pub fn release() -> Result<()> {
    let mut slot = CONTROL
        .lock()
        .map_err(|_| anyhow::anyhow!("UMDF 输入锁异常"))?;
    if let Some(device) = slot.take() {
        device.command(3, 0)?;
        if let Ok(mut recent) = LAST_RELEASE.lock() { *recent = Some((device.1, std::time::Instant::now())); }
        Ok(())
    } else {
        // Parent-process emergency release has its own handle. This prototype
        // permits only one application/session; driver lease remains the fallback.
        Control::open()?.command(3, 0)
    }
}
