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
    #[serde(default)]
    pub network_interface: Option<String>,
    #[serde(default = "default_lan_ip")]
    pub lan_ip: Ipv4Addr,
    pub mobile_port: u16,
    pub admin_port: u16,
    pub bootstrap_port: u16,
    pub output_device: String,
    pub shortcut: crate::platform::Shortcut,
    #[serde(default)]
    pub shortcut_manual: bool,
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

fn default_lan_ip() -> Ipv4Addr {
    Ipv4Addr::LOCALHOST
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            computer_name: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "我的电脑".into()),
            network_interface: None,
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
            shortcut: crate::platform::locked_shortcut()
                .unwrap_or(crate::platform::Shortcut::DoubaoVoice),
            warmup_ms: 350,
            shortcut_manual: false,
            enter_delay_ms: default_enter_delay(),
            autostart: false,
            diagnostic_recording: false,
            codex_empty_compatibility: false,
        }
    }
}

impl Settings {
    /// Read the selected interface's current address; never use a saved IP to select it.
    pub fn resolve_lan_ip(&mut self, available: &[Interface]) -> bool {
        let selected = available.iter()
            .find(|interface| Some(interface.name.as_str()) == self.network_interface.as_deref())
            .or_else(|| available.first());
        let next = selected.map(|interface| interface.ip).unwrap_or(Ipv4Addr::LOCALHOST);
        let next_interface = selected.map(|interface| interface.name.clone())
            .or_else(|| self.network_interface.clone());
        let changed = next != self.lan_ip || next_interface != self.network_interface;
        self.network_interface = next_interface;
        self.lan_ip = next;
        changed
    }

    pub fn resolve_shortcut(&mut self) {
        if let Some(locked) = crate::platform::locked_shortcut() {
            self.shortcut = locked;
            self.shortcut_manual = false;
        } else if !self.shortcut_manual {
            self.shortcut = crate::platform::automatic_shortcut();
        }
    }

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
        let mut saved = serde_json::to_value(self)?;
        // The address is runtime state. Persist only the interface preference.
        saved.as_object_mut().unwrap().remove("lan_ip");
        atomic_write(
            &dir.join("settings.json"),
            &serde_json::to_vec_pretty(&saved)?,
        )
    }
}

#[derive(Serialize)]
pub struct Interface {
    pub name: String,
    pub ip: Ipv4Addr,
    pub ethernet: bool,
}

pub fn interfaces() -> Vec<Interface> {
    let mut interfaces: Vec<_> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(ip) if !ip.is_loopback() && !ip.is_link_local()
                && !ip.is_unspecified() && !ip.is_multicast() && !ip.is_broadcast() => {
                let rank = network_rank(i.index);
                Some((rank, Interface { name: i.name, ip, ethernet: rank == 0 }))
            }
            _ => None,
        })
        .collect();
    interfaces.sort_by_key(|(rank, _)| *rank);
    interfaces.into_iter().map(|(_, interface)| interface).collect()
}

fn network_rank(index: Option<u32>) -> u8 {
    #[cfg(target_os = "windows")]
    {
        unsafe extern "C" { fn ptt_network_rank(index: u32) -> u32; }
        index.map(|index| unsafe { ptt_network_rank(index) } as u8).unwrap_or(3)
    }
    #[cfg(target_os = "macos")]
    { index.map(crate::macos::network_rank).unwrap_or(3) }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    { let _ = index; 1 }
}

pub fn data_dir(override_path: Option<PathBuf>) -> Result<PathBuf> {
    let p = match override_path {
        Some(path) => path,
        None => project_data_dir()?,
    };
    fs::create_dir_all(&p)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?;
    }
    Ok(p)
}

fn project_data_dir() -> Result<PathBuf> {
    Ok(directories::ProjectDirs::from("local", "DoubaoMicrophone", "DoubaoMicrophone")
        .context("user data directory unavailable")?.data_local_dir().to_path_buf())
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
    use super::{Interface, Settings};
    use std::net::Ipv4Addr;

    fn interface(name: &str, ip: Ipv4Addr) -> Interface {
        Interface { name: name.into(), ip, ethernet: false }
    }

    #[test]
    fn startup_reads_a_new_ip_from_the_saved_interface() {
        let mut settings = Settings {
            network_interface: Some("en0".into()),
            lan_ip: Ipv4Addr::new(192, 168, 3, 51), ..Settings::default()
        };
        let current = Ipv4Addr::new(192, 168, 3, 8);
        assert!(settings.resolve_lan_ip(&[
            interface("en5", Ipv4Addr::new(192, 168, 3, 51)), interface("en0", current),
        ]));
        assert_eq!(settings.lan_ip, current);
        assert_eq!(settings.network_interface.as_deref(), Some("en0"));
    }

    #[test]
    fn missing_interface_falls_back_to_an_available_network() {
        let mut settings = Settings {
            network_interface: Some("en0".into()), ..Settings::default()
        };
        let current = Ipv4Addr::new(10, 0, 0, 2);
        assert!(settings.resolve_lan_ip(&[interface("en5", current)]));
        assert_eq!(settings.lan_ip, current);
        assert_eq!(settings.network_interface.as_deref(), Some("en5"));
    }

    #[test]
    fn offline_startup_preserves_interface_and_recovers_on_a_later_start() {
        let mut settings = Settings {
            network_interface: Some("en0".into()),
            lan_ip: Ipv4Addr::new(192, 168, 3, 51), ..Settings::default()
        };
        assert!(settings.resolve_lan_ip(&[]));
        assert_eq!(settings.lan_ip, Ipv4Addr::LOCALHOST);
        assert_eq!(settings.network_interface.as_deref(), Some("en0"));
        assert!(!settings.resolve_lan_ip(&[]));
        let current = Ipv4Addr::new(192, 168, 3, 8);
        assert!(settings.resolve_lan_ip(&[interface("en0", current)]));
        assert_eq!(settings.lan_ip, current);
    }

    #[test]
    fn settings_file_saves_interface_but_never_ip() {
        let dir = tempfile::tempdir().unwrap();
        let settings = Settings {
            network_interface: Some("en0".into()),
            lan_ip: Ipv4Addr::new(192, 168, 3, 8), ..Settings::default()
        };
        settings.save(dir.path()).unwrap();
        let saved: serde_json::Value = serde_json::from_slice(
            &std::fs::read(dir.path().join("settings.json")).unwrap()
        ).unwrap();
        assert_eq!(saved["network_interface"], "en0");
        assert!(saved.get("lan_ip").is_none());
        let mut loaded = Settings::load(dir.path()).unwrap();
        let current = Ipv4Addr::new(192, 168, 3, 9);
        loaded.resolve_lan_ip(&[interface("en0", current)]);
        assert_eq!(loaded.lan_ip, current);
        assert_eq!(serde_json::to_value(&loaded).unwrap()["lan_ip"], "192.168.3.9");
    }

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
