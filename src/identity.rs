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
#[derive(Clone, Serialize)]
pub struct Request {
    pub id: Uuid,
    pub label: String,
}
struct Ticket {
    token: String,
    created: Instant,
}
struct Pending {
    request: Request,
    poll_hash: String,
    token: Option<String>,
    approved: bool,
    created: Instant,
}

pub struct Identities {
    path: PathBuf,
    devices: Vec<Device>,
    ticket: Option<Ticket>,
    pending: Option<Pending>,
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
            pending: None,
        })
    }
    pub fn issue(&mut self) -> String {
        let token = secret();
        self.ticket = Some(Ticket {
            token: token.clone(),
            created: Instant::now(),
        });
        self.pending = None;
        token
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
        let poll = secret();
        let id = Uuid::new_v4();
        self.pending = Some(Pending {
            request: Request { id, label },
            poll_hash: digest(&poll),
            token: None,
            approved: false,
            created: Instant::now(),
        });
        self.ticket = None;
        Ok((id, poll))
    }
    pub fn pending(&self) -> Option<Request> {
        self.pending
            .as_ref()
            .filter(|p| p.created.elapsed() < Duration::from_secs(300) && !p.approved)
            .map(|p| p.request.clone())
    }
    pub fn approve(&mut self, id: Uuid) -> Result<()> {
        let p = self
            .pending
            .as_mut()
            .filter(|p| p.request.id == id && p.created.elapsed() < Duration::from_secs(300))
            .ok_or_else(|| anyhow::anyhow!("配对请求不存在或已过期"))?;
        ensure!(!p.approved, "配对已确认");
        let token = secret();
        let d = Device {
            id,
            label: p.request.label.clone(),
            token_hash: digest(&token),
        };
        let mut next = self.devices.clone();
        next.push(d);
        crate::config::atomic_write(&self.path, &serde_json::to_vec_pretty(&next)?)?;
        self.devices = next;
        p.token = Some(token);
        p.approved = true;
        Ok(())
    }
    pub fn poll(&mut self, id: Uuid, poll: &str) -> Result<Option<String>> {
        let p = self
            .pending
            .as_mut()
            .filter(|p| p.request.id == id && p.created.elapsed() < Duration::from_secs(300))
            .ok_or_else(|| anyhow::anyhow!("配对已过期，请重新扫码"))?;
        ensure!(matches(&p.poll_hash, poll), "配对凭据无效");
        if !p.approved {
            return Ok(None);
        }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pairing_requires_approval_is_single_use_and_revocable() {
        let t = tempfile::tempdir().unwrap();
        let mut i = Identities::load(t.path()).unwrap();
        let ticket = i.issue();
        let (id, poll) = i.request(&ticket, "phone".into()).unwrap();
        assert!(!i.bootstrap_allowed(&ticket));
        assert!(i.poll(id, &poll).unwrap().is_none());
        assert!(i.poll(id, "bad").is_err());
        i.approve(id).unwrap();
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
