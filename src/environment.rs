//! Read-only inspection of saved settings and installed capabilities. No input/audio tests.
use crate::config::Settings;
use serde::Serialize;
use std::sync::Mutex;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckState {
    Pass,
    Fail,
    Unknown,
    NotApplicable,
}

#[derive(Serialize)]
pub struct Check {
    pub id: &'static str,
    pub title: &'static str,
    pub state: CheckState,
    pub detail: String,
}

pub const ITEMS: [(&str, &str); 5] = [
    ("virtual_microphone", "虚拟麦克风"),
    ("doubao_installation", "豆包安装及运行"),
    ("doubao_microphone", "豆包麦克风配置"),
    ("doubao_trigger", "长按快捷键配置"),
    ("doubao_voice_switch", "语音快捷键开关"),
];

#[derive(Serialize)]
pub struct Report {
    pub supported: bool,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn unavailable(message: &str) -> Self {
        Self {
            supported: cfg!(any(target_os = "windows", target_os = "macos")),
            checks: ITEMS
                .iter()
                .map(|&(id, title)| Check {
                    id,
                    title,
                    state: CheckState::Unknown,
                    detail: message.into(),
                })
                .collect(),
        }
    }
}

static INSPECTION: Mutex<()> = Mutex::new(());

pub fn inspect(settings: &Settings, mock: bool, _input_accessibility: Option<bool>) -> Report {
    if mock {
        return Report::unavailable("模拟模式不检查实际安装环境，请使用原生程序进行检测。");
    }
    // A timed-out HTTP request must not leave repeated native inspections accumulating.
    let Ok(_guard) = INSPECTION.try_lock() else {
        return Report::unavailable("上次检测尚未结束，请稍后重新检测。");
    };
    #[cfg(target_os = "windows")]
    {
        crate::windows_environment::inspect(settings)
    }
    #[cfg(target_os = "macos")]
    { crate::macos_environment::inspect(settings, _input_accessibility) }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = settings;
        Report {
            supported: false,
            checks: Vec::new(),
        }
    }
}
