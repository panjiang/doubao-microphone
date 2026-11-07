//! Read-only macOS environment adapter for the shared environment checklist.
use crate::{
    audio,
    config::Settings,
    environment::{Check, CheckState, Report},
};
use anyhow::{Context, Result, ensure};
use cpal::traits::{DeviceTrait, HostTrait};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

fn item(id: &'static str, title: &'static str, result: Result<(CheckState, String)>) -> Check {
    let (state, detail) =
        result.unwrap_or_else(|e| (CheckState::Unknown, format!("无法确认：{e:#}")));
    Check {
        id,
        title,
        state,
        detail,
    }
}
fn result(state: CheckState, detail: impl Into<String>) -> Result<(CheckState, String)> {
    Ok((state, detail.into()))
}

pub(crate) fn inspect(settings: &Settings, input_accessibility: Option<bool>) -> Report {
    let config = read_config();
    Report {
        supported: true,
        checks: vec![
            item(
                "virtual_microphone",
                "虚拟麦克风",
                virtual_microphone(settings),
            ),
            item(
                "doubao_installation",
                "豆包安装及运行",
                match crate::macos::installation() {
                    0 => result(
                        CheckState::Fail,
                        "未检测到豆包输入法，请安装官方 macOS 版并完成启用。",
                    ),
                    1 => result(
                        CheckState::Fail,
                        "已安装豆包输入法，但未运行；请切换到豆包输入法。",
                    ),
                    2 => result(CheckState::Pass, "已检测到豆包输入法文件及对应进程。"),
                    _ => result(
                        CheckState::Unknown,
                        "无法读取输入法进程状态，请确认豆包已启动。",
                    ),
                },
            ),
            item("doubao_microphone", "豆包麦克风配置", microphone(&config)),
            item(
                "doubao_trigger",
                "长按快捷键配置",
                trigger(&config),
            ),
            item(
                "doubao_voice_switch",
                "语音快捷键开关",
                voice_switch(&config),
            ),
            item("input_source", "当前输入法", input_source(&config)),
            item(
                "accessibility",
                "辅助功能权限",
                if !crate::platform::accessibility() || input_accessibility == Some(false) {
                    result(
                        CheckState::Fail,
                        "请在系统设置 → 隐私与安全性 → 辅助功能中允许豆包麦克风及其内置输入助手。",
                    )
                } else if input_accessibility == Some(true) {
                    result(CheckState::Pass, "主应用和独立输入代理均已获辅助功能权限。")
                } else {
                    result(
                        CheckState::Unknown,
                        "主应用权限已通过，但未能取得输入代理权限状态，请稍后重新检测。",
                    )
                },
            ),
        ],
    }
}
fn inputs() -> Result<Vec<(String, String)>> {
    cpal::default_host()
        .input_devices()?
        .map(|device| {
            Ok((
                device.id()?.id().to_owned(),
                device.description()?.name().to_owned(),
            ))
        })
        .collect()
}
fn virtual_microphone(settings: &Settings) -> Result<(CheckState, String)> {
    let outputs = audio::devices(false)?;
    if !outputs.iter().any(|d| d.name == "BlackHole 2ch")
        || !inputs()?.iter().any(|(_, name)| name == "BlackHole 2ch")
    {
        return result(
            CheckState::Fail,
            "未检测到可用的 BlackHole 2ch 输入和输出，请运行安装包修复虚拟麦克风；按安装器提示重启。",
        );
    }
    match audio::select_output_index(&settings.output_device, &outputs) {
        Ok(index) if outputs[index].name == "BlackHole 2ch" => result(
            CheckState::Pass,
            "BlackHole 2ch 输入和输出可用，程序已选择对应输出设备。",
        ),
        _ => result(
            CheckState::Fail,
            "程序未选择可唯一匹配的 BlackHole 2ch，请刷新设备、选择并保存设置。",
        ),
    }
}
fn config(config: &Result<Value>) -> Result<&Value> {
    config.as_ref().map_err(|e| anyhow::anyhow!("{e:#}"))
}
fn microphone(saved: &Result<Value>) -> Result<(CheckState, String)> {
    let id = config(saved)?
        .get("selectedMicrophoneId")
        .and_then(Value::as_str)
        .context("缺少可识别的麦克风设置，请在豆包中明确选择 BlackHole 2ch")?;
    if id.is_empty() || id == "ime_auto_detect_microphone_id" || id == "default" {
        return result(
            CheckState::Unknown,
            "豆包正在自动选择麦克风，请在豆包设置中明确选择 BlackHole 2ch 后重新检测。",
        );
    }
    match inputs()?
        .into_iter()
        .find(|(uid, _)| id == uid || id == format!("coreaudio:{uid}"))
    {
        Some((_, name)) if name == "BlackHole 2ch" => result(
            CheckState::Pass,
            "豆包明确选择了当前有效的 BlackHole 2ch 录音设备。",
        ),
        Some(_) => result(
            CheckState::Fail,
            "豆包选择了其他麦克风，请改为 BlackHole 2ch。",
        ),
        None => result(
            CheckState::Unknown,
            "已保存的麦克风 ID 无法匹配当前设备，请在豆包中重新选择 BlackHole 2ch。",
        ),
    }
}
fn trigger(saved: &Result<Value>) -> Result<(CheckState, String)> {
    let saved = config(saved)?;
    let code = saved
        .get("asrLongPressShortcutKeyCode")
        .and_then(Value::as_u64)
        .context("缺少可识别的长按快捷键编码，请手动核对豆包设置")?;
    let flags = saved
        .get("asrLongPressShortcutModifierFlags")
        .and_then(Value::as_u64)
        .context("缺少可识别的长按修饰键配置，请手动核对豆包设置")?;
    if code == 63 && flags == 0 {
        result(
            CheckState::Pass,
            "豆包长按快捷键为 Fn，与程序保存的触发方式一致。",
        )
    } else {
        result(
            CheckState::Fail,
            "豆包长按快捷键与程序固定使用的 Fn 不一致，请在豆包中将长按键设为 Fn / 地球仪。",
        )
    }
}
fn voice_switch(saved: &Result<Value>) -> Result<(CheckState, String)> {
    match config(saved)?
        .get("isStartASRShortcutEnable")
        .and_then(Value::as_bool)
    {
        Some(true) => result(CheckState::Pass, "豆包语音快捷键已开启。"),
        Some(false) => result(
            CheckState::Fail,
            "豆包语音快捷键已关闭，请在豆包设置中开启。",
        ),
        None => result(
            CheckState::Unknown,
            "无法识别豆包语音快捷键开关，请在豆包中手动确认。",
        ),
    }
}
fn input_source(saved: &Result<Value>) -> Result<(CheckState, String)> {
    if crate::macos::doubao_selected() {
        return result(CheckState::Pass, "当前已切换到豆包输入法。");
    }
    match config(saved)?
        .get("isGloableASRShortcutEnable")
        .and_then(Value::as_bool)
    {
        Some(true) => result(
            CheckState::Pass,
            "豆包已启用全局语音，可在其他输入法下触发；实际效果仍需实测。",
        ),
        Some(false) => result(
            CheckState::Fail,
            "当前未使用豆包且未开启全局语音，请先切换到豆包输入法，或在豆包设置中启用全局语音。",
        ),
        None => result(
            CheckState::Unknown,
            "当前未使用豆包，无法确认全局语音开关；请切换到豆包输入法后使用。",
        ),
    }
}

fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= limit, "设置文件超过读取上限");
    Ok(bytes)
}
fn read_config() -> Result<Value> {
    let path = PathBuf::from(std::env::var_os("HOME").context("无法定位用户设置目录")?).join(
        "Library/Application Support/DoubaoIme/.com.apple.xpc.activityd/com.apple.xpc.activity",
    );
    let meta = PathBuf::from(format!("{}.crc", path.display()));
    // No locks or writes to the vendor store. Reject a torn snapshot and retry once.
    let read = || -> Result<Value> {
        let before = bounded_read(&meta, 64 * 1024)?;
        let bytes = bounded_read(&path, 1024 * 1024)?;
        let after = bounded_read(&meta, 64 * 1024)?;
        ensure!(before == after, "豆包正在保存设置，请稍后重新检测");
        let archive = extract_archive(&bytes, &after)?;
        crate::macos::decode_settings(&archive)
    };
    read()
        .or_else(|_| read())
        .context("无法读取豆包保存的语音设置；可能尚未初始化、格式变化或权限不足")
}

// Minimal read-only MMKV framing. See Tencent/MMKV Core/MiniPBCoder.cpp and
// MMKVMetaInfo.hpp. CRC, bounds and latest-value semantics are mandatory.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= *byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}
fn extract_archive(bytes: &[u8], meta: &[u8]) -> Result<Vec<u8>> {
    ensure!(bytes.len() >= 4 && meta.len() >= 32, "设置存储头不完整");
    let word = |b: &[u8], i| u32::from_le_bytes(b[i..i + 4].try_into().unwrap());
    let count = word(bytes, 0) as usize;
    ensure!(
        count <= bytes.len() - 4 && count <= 1024 * 1024,
        "设置存储长度无效"
    );
    let payload = &bytes[4..4 + count];
    ensure!(
        word(meta, 0) == crc32(payload),
        "设置校验不一致，请稍后重新检测"
    );
    if word(meta, 4) >= 3 {
        ensure!(word(meta, 28) as usize == count, "设置长度正在变化");
    }
    if word(meta, 4) >= 4 {
        ensure!(
            meta.len() >= 112 && u64::from_le_bytes(meta[104..112].try_into().unwrap()) == 0,
            "无法识别设置存储标志"
        );
    }
    let mut input = payload;
    varint(&mut input)?; // MMKV's container placeholder is not the live length.
    let mut allowed = HashMap::new();
    while !input.is_empty() {
        let key = field(&mut input)?;
        // Empty keys have no value in MMKV's decoder.
        if key.is_empty() {
            continue;
        }
        let value = field(&mut input)?;
        if key == b"d" {
            if value.is_empty() {
                allowed.remove("d");
            } else {
                allowed.insert("d", value);
            }
        }
    }
    let value = allowed.get("d").context("缺少可识别的设置字典")?;
    ensure!(value.starts_with(b"bplist"), "设置字典格式无法识别");
    Ok(value.to_vec())
}
fn varint(input: &mut &[u8]) -> Result<usize> {
    let mut value = 0u32;
    for shift in [0, 7, 14, 21, 28] {
        let (&byte, rest) = input.split_first().context("设置字段不完整")?;
        *input = rest;
        ensure!(shift != 28 || byte < 16, "设置字段长度溢出");
        value |= ((byte & 127) as u32) << shift;
        if byte < 128 {
            return Ok(value as usize);
        }
    }
    anyhow::bail!("设置字段长度无效")
}
fn field<'a>(input: &mut &'a [u8]) -> Result<&'a [u8]> {
    let length = varint(input)?;
    ensure!(length <= input.len(), "设置字段越界");
    let (value, rest) = input.split_at(length);
    *input = rest;
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store(payload: &[u8]) -> (Vec<u8>, Vec<u8>) {
        let mut b = (payload.len() as u32).to_le_bytes().to_vec();
        b.extend(payload);
        let mut m = vec![0u8; 112];
        m[..4].copy_from_slice(&crc32(payload).to_le_bytes());
        m[4..8].copy_from_slice(&4u32.to_le_bytes());
        m[28..32].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        (b, m)
    }
    #[test]
    fn vendor_snapshot_uses_latest_value_and_rejects_corruption_or_tombstone() {
        let (mut b, m) = store(b"\0\x01d\x07bplist0\x01d\x07bplist1");
        assert_eq!(extract_archive(&b, &m).unwrap(), b"bplist1");
        b[8] ^= 1;
        assert!(extract_archive(&b, &m).is_err());
        let (b, m) = store(b"\0\x01d\x07bplist0\x01d\0");
        assert!(extract_archive(&b, &m).is_err());
        let (b, m) = store(b"\0\x01d\xff\xff\xff\xff\x7f");
        assert!(extract_archive(&b, &m).is_err());
    }
}
