use anyhow::{Result, ensure};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub fn secret() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    URL_SAFE_NO_PAD.encode(b)
}
pub fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn matches(hash: &str, token: &str) -> bool {
    hash.as_bytes().ct_eq(digest(token).as_bytes()).into()
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: Uuid,
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token_hash: String,
}
struct Ticket {
    id: Uuid,
    token: String,
    created: Instant,
}
struct PairReceipt {
    id: Uuid,
    ticket_id: Uuid,
    poll_hash: String,
    token: Option<String>,
    created: Instant,
}

pub struct Identities {
    path: PathBuf,
    devices: Vec<Device>,
    ticket: Option<Ticket>,
    receipt: Option<PairReceipt>,
}

impl Identities {
    pub fn load(dir: &std::path::Path) -> Result<Self> {
        let path = dir.join("devices.json");
        let devices = if path.exists() {
            serde_json::from_slice(&std::fs::read(&path)?)?
        } else {
            Vec::new()
        };
        Ok(Self {
            path,
            devices,
            ticket: None,
            receipt: None,
        })
    }
    pub fn issue(&mut self) -> String {
        let token = secret();
        self.ticket = Some(Ticket {
            id: Uuid::new_v4(),
            token: token.clone(),
            created: Instant::now(),
        });
        self.receipt = None;
        token
    }
    pub fn ticket_id(&self) -> Option<Uuid> {
        self.ticket.as_ref().map(|t| t.id)
    }
    pub fn completed_ticket_id(&self) -> Option<Uuid> {
        self.receipt.as_ref().filter(|p| p.token.is_none()).map(|p| p.ticket_id)
    }
    pub fn bootstrap_allowed(&self, token: &str) -> bool {
        self.ticket.as_ref().is_some_and(|t| {
            t.created.elapsed() < Duration::from_secs(300) && digest(&t.token) == digest(token)
        })
    }
    pub fn request(&mut self, ticket: &str, label: String) -> Result<(Uuid, String)> {
        ensure!(
            self.bootstrap_allowed(ticket),
            "配对二维码无效或已超过 5 分钟"
        );
        ensure!(
            !label.trim().is_empty() && label.chars().count() <= 48,
            "设备名称必须为 1–48 个字符"
        );
        let ticket_id = self.ticket.as_ref().expect("validated ticket").id;
        let poll = secret();
        let id = Uuid::new_v4();
        let token = secret();
        let mut next = self.devices.clone();
        next.push(Device { id, label, token_hash: digest(&token) });
        // Persist before consuming the ticket so a disk failure can be retried.
        crate::config::atomic_write(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        self.devices = next;
        self.receipt = Some(PairReceipt {
            id,
            ticket_id,
            poll_hash: digest(&poll),
            token: Some(token),
            created: Instant::now(),
        });
        self.ticket = None;
        Ok((id, poll))
    }
    pub fn poll(&mut self, id: Uuid, poll: &str) -> Result<Option<String>> {
        let p = self
            .receipt
            .as_mut()
            .filter(|p| p.id == id && p.created.elapsed() < Duration::from_secs(300))
            .ok_or_else(|| anyhow::anyhow!("配对已过期，请重新扫码"))?;
        ensure!(matches(&p.poll_hash, poll), "配对凭据无效");
        let token = p
            .token
            .take()
            .ok_or_else(|| anyhow::anyhow!("凭据已经领取，请重新配对"))?;
        Ok(Some(token))
    }
    pub fn authenticate(&self, token: &str) -> Option<Uuid> {
        self.devices
            .iter()
            .find(|d| matches(&d.token_hash, token))
            .map(|d| d.id)
    }
    pub fn devices(&self) -> Vec<Device> {
        self.devices
            .iter()
            .map(|d| Device {
                id: d.id,
                label: d.label.clone(),
                token_hash: String::new(),
            })
            .collect()
    }
    pub fn revoke(&mut self, id: Uuid) -> Result<()> {
        let next: Vec<_> = self
            .devices
            .iter()
            .filter(|d| d.id != id)
            .cloned()
            .collect();
        crate::config::atomic_write(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        self.devices = next;
        Ok(())
    }
    pub fn rename(&mut self, id: Uuid, label: &str) -> Result<()> {
        let label = label.trim();
        ensure!(!label.is_empty() && label.chars().count() <= 48
            && !label.chars().any(char::is_control), "设备名称必须为 1–48 个字符，且不能含换行等控制字符");
        let mut next = self.devices.clone();
        let device = next.iter_mut().find(|device| device.id == id)
            .ok_or_else(|| anyhow::anyhow!("设备不存在或已撤销配对"))?;
        device.label = label.to_owned();
        crate::config::atomic_write(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        self.devices = next;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_preserves_pairing_credentials_and_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let mut identities = Identities::load(dir.path()).unwrap();
        let ticket = identities.issue();
        let (id, poll) = identities.request(&ticket, "iPhone App".into()).unwrap();
        let token = identities.poll(id, &poll).unwrap().unwrap();
        identities.rename(id, "  工作手机  ").unwrap();
        let reloaded = Identities::load(dir.path()).unwrap();
        assert_eq!(reloaded.authenticate(&token), Some(id));
        assert_eq!(reloaded.devices()[0].label, "工作手机");
        assert!(reloaded.devices()[0].token_hash.is_empty());
        for invalid in [" ", "a\nb", &"名".repeat(49)] {
            assert!(identities.rename(id, invalid).is_err());
        }
        assert!(identities.rename(Uuid::new_v4(), "手机").is_err());
        assert_eq!(Identities::load(dir.path()).unwrap().devices()[0].label, "工作手机");
    }
    #[test]
    fn pairing_is_automatic_single_use_and_revocable() {
        let t = tempfile::tempdir().unwrap();
        let mut i = Identities::load(t.path()).unwrap();
        let ticket = i.issue();
        let (id, poll) = i.request(&ticket, "phone".into()).unwrap();
        assert!(!i.bootstrap_allowed(&ticket));
        assert!(i.poll(id, "bad").is_err());
        let token = i.poll(id, &poll).unwrap().unwrap();
        assert_eq!(i.authenticate(&token), Some(id));
        assert!(i.poll(id, &poll).is_err());
        assert!(
            !std::fs::read_to_string(t.path().join("devices.json"))
                .unwrap()
                .contains(&token)
        );
        let mut i = Identities::load(t.path()).unwrap();
        assert_eq!(i.authenticate(&token), Some(id));
        i.revoke(id).unwrap();
        assert!(i.authenticate(&token).is_none());
    }
    #[test]
    fn constant_time_token_comparison() {
        let t = secret();
        assert!(matches(&digest(&t), &t));
        assert!(!matches(&digest(&t), "wrong"));
    }
}
