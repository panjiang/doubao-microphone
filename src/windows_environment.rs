//! Capability checks; no version/schema allowlist, recording, key events or config writes.
use crate::{
    audio,
    config::Settings,
    environment::{Check, CheckState, Report},
    platform::Shortcut,
};
use anyhow::{Context, Result, bail, ensure};
use cpal::traits::{DeviceTrait, HostTrait};
use serde_json::Value;
use std::{fs, io::Read, path::PathBuf};
use windows::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND,
        },
        System::{
            Diagnostics::ToolHelp::*,
            Registry::*,
            Threading::{
                OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
                QueryFullProcessImageNameW,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

struct Outcome(CheckState, String);
impl Outcome {
    fn pass(message: impl Into<String>) -> Self {
        Self(CheckState::Pass, message.into())
    }
    fn fail(message: impl Into<String>) -> Self {
        Self(CheckState::Fail, message.into())
    }
    fn unknown(message: impl Into<String>) -> Self {
        Self(CheckState::Unknown, message.into())
    }
}
fn check(id: &'static str, title: &'static str, result: Result<Outcome>) -> Check {
    let Outcome(state, detail) =
        result.unwrap_or_else(|e| Outcome::unknown(format!("无法确认：{e:#}")));
    Check {
        id,
        title,
        state,
        detail,
    }
}

struct Capture {
    id: String,
    raw_id: String,
    name: String,
}
fn captures() -> Result<Vec<Capture>> {
    let mut result = Vec::new();
    for device in cpal::default_host().input_devices()? {
        let id = device.id()?;
        result.push(Capture {
            raw_id: id.id().into(),
            id: id.to_string(),
            name: device.description()?.name().into(),
        });
    }
    Ok(result)
}
fn named(name: &str, expected: &str) -> bool {
    name == expected || name.starts_with(&format!("{expected} ("))
}
fn borrowed<T>(result: &Result<T>) -> Result<&T> {
    result.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))
}

pub(crate) fn inspect(settings: &Settings) -> Report {
    let outputs = audio::devices(false).context("无法读取播放设备列表");
    let inputs = captures().context("无法读取录音设备列表");
    let config = read_config();
    let toolbar = settings.shortcut == Shortcut::DoubaoVoice;
    Report {
        supported: true,
        checks: vec![
            check(
                "virtual_microphone",
                "虚拟麦克风",
                virtual_microphone(settings, &outputs, &inputs),
            ),
            check("doubao_installation", "豆包安装及运行", installation()),
            check(
                "doubao_microphone",
                "豆包麦克风配置",
                microphone(&config, &inputs),
            ),
            check(
                "doubao_trigger",
                if toolbar {
                    "豆包工具栏配置"
                } else {
                    "长按快捷键配置"
                },
                trigger(settings.shortcut, &config),
            ),
            check(
                "doubao_voice_switch",
                "语音快捷键开关",
                voice_switch(toolbar, &config),
            ),
        ],
    }
}

fn virtual_microphone(
    settings: &Settings,
    outputs: &Result<Vec<audio::Device>>,
    inputs: &Result<Vec<Capture>>,
) -> Result<Outcome> {
    let outputs = borrowed(outputs)?;
    let inputs = borrowed(inputs)?;
    if !outputs.iter().any(|d| named(&d.name, "CABLE Input"))
        || !inputs.iter().any(|d| named(&d.name, "CABLE Output"))
    {
        return Ok(Outcome::fail(
            "未检测到启用的 CABLE Input 和 CABLE Output，请运行安装包修复 VB-CABLE；如提示重启，请先重启。",
        ));
    }
    match audio::select_output_index(&settings.output_device, outputs) {
        Ok(index) if named(&outputs[index].name, "CABLE Input") => Ok(Outcome::pass(
            "CABLE Input 和 CABLE Output 已启用，程序输出设置匹配 CABLE Input。",
        )),
        Ok(_) => Ok(Outcome::fail(
            "程序输出设置未选择 CABLE Input，请在输入设置中选择并保存。",
        )),
        Err(e) => Ok(Outcome::fail(format!(
            "程序输出设置无法匹配设备：{e}。请刷新设备并保存设置。"
        ))),
    }
}

fn read_config() -> Result<Value> {
    let path =
        PathBuf::from(std::env::var_os("APPDATA").context("无法定位当前用户的豆包配置目录")?)
            .join("DoubaoIME/conf/config.json");
    // Bound memory even if the vendor file changes during the read. Never log its contents.
    let file =
        fs::File::open(path).context("无法读取豆包已保存的配置，请先启动豆包并检查访问权限")?;
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= 1024 * 1024, "豆包配置文件过大，无法确认");
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes);
    serde_json::from_slice(bytes).context("豆包配置暂时无法解析，请稍后重新检测")
}

fn microphone(config: &Result<Value>, inputs: &Result<Vec<Capture>>) -> Result<Outcome> {
    let config = borrowed(config)?;
    let selected = config
        .pointer("/voice/selectedMicrophoneId")
        .and_then(Value::as_str)
        .context("缺少可识别的麦克风配置字段，请在豆包中明确选择 CABLE Output")?;
    if selected.trim().is_empty() || selected.eq_ignore_ascii_case("default") {
        return Ok(Outcome::unknown(
            "豆包未明确选择录音设备或正在跟随默认设备，请在豆包中选择 CABLE Output 后重新检测。",
        ));
    }
    let inputs = borrowed(inputs)?;
    let selected_device = inputs
        .iter()
        .find(|device| selected == device.raw_id || selected == device.id);
    match selected_device {
        Some(device) if named(&device.name, "CABLE Output") => Ok(Outcome::pass(
            "豆包保存的麦克风为 CABLE Output，设备当前已启用。",
        )),
        Some(device) => Ok(Outcome::fail(format!(
            "豆包当前选择“{}”，请在豆包设置中改为 CABLE Output。",
            device.name
        ))),
        None => Ok(Outcome::fail(
            "豆包保存的麦克风已失效或未启用，请在豆包设置中重新选择 CABLE Output。",
        )),
    }
}

fn trigger(shortcut: Shortcut, config: &Result<Value>) -> Result<Outcome> {
    if crate::platform::windows_umdf_trial() && !crate::windows_umdf::available() {
        return Ok(Outcome::fail(
            "当前账户无法访问 UMDF 虚拟键盘。请按安装提示重启，仍失败时重新运行试用安装包。",
        ));
    }
    let config = borrowed(config)?;
    if shortcut == Shortcut::DoubaoVoice {
        return match config
            .pointer("/status_bar/enabled")
            .and_then(Value::as_bool)
        {
            Some(true) => Ok(Outcome::pass(
                "豆包已保存开启悬浮工具栏；工具栏实际触发效果仍需手动确认。",
            )),
            Some(false) => Ok(Outcome::fail(
                "当前使用工具栏触发，请在豆包设置中开启展示悬浮工具栏。",
            )),
            None => Ok(Outcome::unknown(
                "无法识别豆包工具栏开关，请在豆包设置中确认已开启悬浮工具栏。",
            )),
        };
    }
    if !matches!(shortcut, Shortcut::HidRightAlt | Shortcut::RightAlt) {
        return Ok(Outcome::unknown(
            "当前诊断快捷键的豆包配置编码尚未确认，请手动核对；本次检测不发送按键。",
        ));
    }
    let key = config
        .pointer("/voice/voiceLongPressShortcut/keyCode")
        .and_then(Value::as_u64)
        .context("无法识别豆包长按快捷键配置，请手动核对")?;
    let modifiers = config
        .pointer("/voice/voiceLongPressShortcut/modifierFlags")
        .and_then(Value::as_u64)
        .context("无法识别豆包长按修饰键配置，请手动核对")?;
    if (key == 0 && modifiers == 0) || (modifiers == 2049 && (1..=254).contains(&key)) {
        return Ok(Outcome::fail(
            "豆包长按快捷键与程序的右 Alt 不一致，请在豆包中将长按快捷键设为右 Alt。",
        ));
    }
    if key != 0 || modifiers != 2049 {
        return Ok(Outcome::unknown(
            "无法识别当前长按快捷键编码，请在豆包中确认长按快捷键设为右 Alt 后重新检测。",
        ));
    }
    if shortcut == Shortcut::HidRightAlt
        && !crate::platform::windows_umdf_trial()
        && !crate::windows_umdf::available()
    {
        return Ok(Outcome::fail(
            "豆包右 Alt 配置一致，但程序无法访问 UMDF 组件，请检查组件安装及访问权限。",
        ));
    }
    Ok(Outcome::pass(
        "豆包长按快捷键为右 Alt，与程序保存的触发方式一致；未自动触发按键。",
    ))
}

fn voice_switch(toolbar: bool, config: &Result<Value>) -> Result<Outcome> {
    if toolbar {
        return Ok(Outcome(
            CheckState::NotApplicable,
            "当前使用工具栏触发，无需开启键盘语音快捷键。".into(),
        ));
    }
    match borrowed(config)?
        .pointer("/voice/enableVoiceShortcut")
        .and_then(Value::as_bool)
    {
        Some(true) => Ok(Outcome::pass("豆包语音快捷键已开启。")),
        Some(false) => Ok(Outcome::fail("豆包语音快捷键已关闭，请在豆包设置中开启。")),
        None => Ok(Outcome::unknown(
            "缺少可识别的语音快捷键开关，请在豆包设置中手动确认。",
        )),
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
struct RegistryKey(HKEY);
impl Drop for RegistryKey {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}
impl RegistryKey {
    fn open(parent: HKEY, name: &str, flags: REG_SAM_FLAGS) -> Result<Option<Self>> {
        let name = wide(name);
        let mut key = HKEY::default();
        let error = unsafe {
            RegOpenKeyExW(
                parent,
                PCWSTR(name.as_ptr()),
                None,
                KEY_READ | flags,
                &mut key,
            )
        };
        if error == ERROR_FILE_NOT_FOUND || error == ERROR_PATH_NOT_FOUND {
            return Ok(None);
        }
        error.ok()?;
        Ok(Some(Self(key)))
    }
    fn string(&self, name: &str) -> Result<Option<String>> {
        let name = wide(name);
        let mut size = 0;
        let code = unsafe {
            RegGetValueW(
                self.0,
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if code == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        code.ok()?;
        ensure!(size <= 65536, "安装记录字段过大");
        let mut buffer = vec![0u16; size.div_ceil(2) as usize];
        unsafe {
            RegGetValueW(
                self.0,
                PCWSTR::null(),
                PCWSTR(name.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                Some(buffer.as_mut_ptr().cast()),
                Some(&mut size),
            )
            .ok()?;
        }
        let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        Ok(Some(String::from_utf16_lossy(&buffer[..length])))
    }
}

fn registry_roots() -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    let mut incomplete = false;
    for parent in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let key = match RegistryKey::open(
                parent,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
                view,
            ) {
                Ok(Some(key)) => key,
                Ok(None) => continue,
                Err(_) => {
                    incomplete = true;
                    continue;
                }
            };
            for index in 0..8192 {
                let mut name = [0u16; 256];
                let mut length = name.len() as u32;
                let code = unsafe {
                    RegEnumKeyExW(
                        key.0,
                        index,
                        Some(PWSTR(name.as_mut_ptr())),
                        &mut length,
                        None,
                        None,
                        None,
                        None,
                    )
                };
                if code == ERROR_NO_MORE_ITEMS {
                    break;
                }
                if code.is_err() {
                    incomplete = true;
                    continue;
                }
                let name = String::from_utf16_lossy(&name[..length as usize]);
                let item = match RegistryKey::open(key.0, &name, view) {
                    Ok(Some(item)) => item,
                    Ok(None) => continue,
                    Err(_) => {
                        incomplete = true;
                        continue;
                    }
                };
                match item.string("DisplayName") {
                    Ok(Some(display)) => {
                        if display == "豆包输入法"
                            || display.to_lowercase() == "doubaoime"
                            || display.starts_with("豆包输入法 ")
                        {
                            match item.string("InstallLocation") {
                                Ok(Some(path)) if !path.trim().is_empty() => {
                                    roots.push(PathBuf::from(path))
                                }
                                _ => incomplete = true,
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(_) => incomplete = true,
                }
                if index == 8191 {
                    incomplete = true;
                }
            }
        }
    }
    // Known roots below can still prove installation even when registry access is incomplete.
    if incomplete && roots.is_empty() {
        bail!("部分安装记录无法读取");
    }
    Ok(roots)
}

fn service_paths() -> Result<(Vec<PathBuf>, bool)> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)? };
    let result = (|| -> Result<_> {
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut paths = Vec::new();
        let mut inaccessible = false;
        let first = unsafe { Process32FirstW(snapshot, &mut entry) };
        if let Err(error) = first {
            if error.code() == ERROR_NO_MORE_ITEMS.to_hresult() {
                return Ok((paths, false));
            }
            return Err(error.into());
        }
        loop {
            let length = entry
                .szExeFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..length])
                .eq_ignore_ascii_case("ImeService.exe")
            {
                match unsafe {
                    OpenProcess(
                        PROCESS_QUERY_LIMITED_INFORMATION,
                        false,
                        entry.th32ProcessID,
                    )
                } {
                    Ok(process) => {
                        let mut buffer = vec![0u16; 32768];
                        let mut size = buffer.len() as u32;
                        let query = unsafe {
                            QueryFullProcessImageNameW(
                                process,
                                PROCESS_NAME_FORMAT(0),
                                PWSTR(buffer.as_mut_ptr()),
                                &mut size,
                            )
                        };
                        unsafe {
                            let _ = CloseHandle(process);
                        }
                        if query.is_ok() {
                            paths.push(PathBuf::from(String::from_utf16_lossy(
                                &buffer[..size as usize],
                            )));
                        } else {
                            inaccessible = true;
                        }
                    }
                    Err(_) => inaccessible = true,
                }
            }
            if let Err(error) = unsafe { Process32NextW(snapshot, &mut entry) } {
                if error.code() != ERROR_NO_MORE_ITEMS.to_hresult() {
                    inaccessible = true;
                }
                break;
            }
        }
        Ok((paths, inaccessible))
    })();
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    result
}

fn installation() -> Result<Outcome> {
    let registered = registry_roots();
    let mut incomplete = registered.is_err();
    let mut roots = registered.unwrap_or_default();
    for key in ["ProgramFiles", "ProgramW6432", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(key) {
            roots.push(PathBuf::from(root).join("DoubaoIME"));
        }
    }
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(root).join("Programs/DoubaoIME"));
    }
    roots.sort();
    roots.dedup();
    let mut installed = false;
    for root in &roots {
        installed |= root.join("ImeService.exe").is_file();
        match fs::read_dir(root.join("versions")) {
            Ok(versions) => {
                for item in versions.take(256) {
                    match item {
                        Ok(item) => installed |= item.path().join("ImeService.exe").is_file(),
                        Err(_) => incomplete = true,
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => incomplete = true,
        }
    }
    let (processes, inaccessible) = service_paths().context("无法读取豆包运行状态")?;
    incomplete |= inaccessible;
    for process in &processes {
        let path = process.to_string_lossy().replace('/', "\\").to_lowercase();
        let recognized = roots.iter().any(|root| {
            let prefix = format!(
                "{}\\",
                root.to_string_lossy()
                    .replace('/', "\\")
                    .trim_end_matches('\\')
                    .to_lowercase()
            );
            path.starts_with(&prefix)
        });
        if recognized && process.is_file() {
            return Ok(Outcome::pass(
                "已检测到豆包输入法程序，且豆包服务正在运行。",
            ));
        }
        incomplete = true;
    }
    if incomplete {
        return Ok(Outcome::unknown(
            "安装或进程信息不完整，请确认已安装并启动豆包，或检查访问权限后重新检测。",
        ));
    }
    if installed {
        Ok(Outcome::fail(
            "豆包已安装但服务未运行，请启动豆包输入法后重新检测。",
        ))
    } else {
        Ok(Outcome::fail(
            "未检测到豆包输入法程序，请先安装并启动豆包输入法。",
        ))
    }
}
