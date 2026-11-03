use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    net::Ipv4Addr,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub computer_name: String,
    pub lan_ip: Ipv4Addr,
    pub mobile_port: u16,
    pub admin_port: u16,
    pub bootstrap_port: u16,
    pub output_device: String,
    pub shortcut: crate::platform::Shortcut,
    pub warmup_ms: u64,
    #[serde(default = "default_enter_delay")]
    pub enter_delay_ms: u64,
    pub autostart: bool,
    #[serde(default)]
    pub diagnostic_recording: bool,
    #[serde(default)]
    pub codex_empty_compatibility: bool,
}

fn default_enter_delay() -> u64 {
    1000
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            computer_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "我的电脑".into()),
            lan_ip: interfaces()
                .into_iter()
                .next()
                .map(|i| i.ip)
                .unwrap_or(Ipv4Addr::LOCALHOST),
            mobile_port: 38443,
            admin_port: 38440,
            bootstrap_port: 38441,
            output_device: if cfg!(target_os = "macos") {
                "BlackHole 2ch"
            } else {
                "CABLE Input"
            }
            .into(),
            shortcut: if cfg!(target_os = "macos") {
                crate::platform::Shortcut::Fn
            } else {
                crate::platform::Shortcut::DoubaoVoice
            },
            warmup_ms: 350,
            enter_delay_ms: default_enter_delay(),
            autostart: false,
            diagnostic_recording: false,
            codex_empty_compatibility: false,
        }
    }
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            (100..=450).contains(&self.warmup_ms),
            "启动等待必须为 100–450ms"
        );
        ensure!(
            !self.computer_name.trim().is_empty() && self.computer_name.chars().count() <= 64,
            "电脑名称必须为 1–64 个字符"
        );
        ensure!(
            !self.output_device.trim().is_empty() && self.output_device.len() <= 256,
            "请选择输出设备"
        );
        ensure!(self.enter_delay_ms <= 5000, "回车等待必须为 0–5000ms");
        let ports = [self.admin_port, self.mobile_port, self.bootstrap_port];
        ensure!(
            ports.iter().all(|p| *p >= 1024)
                && ports[0] != ports[1]
                && ports[1] != ports[2]
                && ports[0] != ports[2],
            "端口必须不同且不小于 1024"
        );
        ensure!(
            self.lan_ip.is_loopback() || interfaces().iter().any(|i| i.ip == self.lan_ip),
            "所选局域网 IP 不属于本机"
        );
        Ok(())
    }
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join("settings.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        serde_json::from_slice(&fs::read(path)?).context("配置损坏，保留原件；请修复 settings.json")
    }
    pub fn save(&self, dir: &Path) -> Result<()> {
        atomic_write(
            &dir.join("settings.json"),
            &serde_json::to_vec_pretty(self)?,
        )
    }
}

#[derive(Serialize)]
pub struct Interface {
    pub name: String,
    pub ip: Ipv4Addr,
}

pub fn interfaces() -> Vec<Interface> {
    if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local() => {
                Some(Interface { name: i.name, ip })
            }
            _ => None,
        })
        .collect()
}

pub fn data_dir(override_path: Option<PathBuf>) -> Result<PathBuf> {
    let p = override_path.unwrap_or(
        directories::ProjectDirs::from("local", "DoubaoPTT", "DoubaoPTT")
            .context("user data directory unavailable")?
            .data_local_dir()
            .to_path_buf(),
    );
    fs::create_dir_all(&p)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?;
    }
    Ok(p)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let mut t = tempfile::NamedTempFile::new_in(path.parent().context("missing parent")?)?;
    t.write_all(bytes)?;
    t.as_file().sync_all()?;
    t.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn old_settings_load_without_enabling_recording_or_codex_compatibility() {
        let mut old = serde_json::to_value(super::Settings::default()).unwrap();
        old.as_object_mut().unwrap().remove("diagnostic_recording");
        old.as_object_mut()
            .unwrap()
            .remove("codex_empty_compatibility");
        let settings: super::Settings = serde_json::from_value(old).unwrap();
        assert!(!settings.diagnostic_recording);
        assert!(!settings.codex_empty_compatibility);
        assert_eq!(settings.warmup_ms, 350);
    }
}
