use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use rcgen::{
    BasicConstraints, CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, KeyUsagePurpose,
};
use std::{path::Path, sync::Arc};
use time::{Duration, OffsetDateTime};

pub struct TlsMaterial {
    pub server: Arc<axum_server::tls_rustls::RustlsConfig>,
    pub profile: String,
}

fn ca_params() -> CertificateParams {
    let mut p = CertificateParams::default();
    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "Doubao PTT Local CA");
    p.distinguished_name = dn;
    p.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    p.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    p.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    p.not_after = OffsetDateTime::now_utc() + Duration::days(3650);
    p
}

pub async fn prepare(dir: &Path, ip: std::net::Ipv4Addr, mock: bool) -> Result<TlsMaterial> {
    let key = load_or_create_ca(dir, mock)?;
    let ca = ca_params().self_signed(&key)?;
    let public = dir.join("ca.pem");
    if !public.exists() {
        crate::config::atomic_write(&public, ca.pem().as_bytes())?;
    }
    let mut p =
        CertificateParams::new(vec![ip.to_string(), "127.0.0.1".into(), "localhost".into()])?;
    p.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    p.not_after = OffsetDateTime::now_utc() + Duration::days(90);
    p.distinguished_name
        .push(DnType::CommonName, "Doubao PTT Local Bridge");
    p.extended_key_usages = vec![rcgen::ExtendedKeyUsagePurpose::ServerAuth];
    let leaf_key = KeyPair::generate()?;
    let leaf = p.signed_by(&leaf_key, &ca, &key)?;
    let saved_ca = std::fs::read_to_string(public)?;
    let certs = format!("{}{}", leaf.pem(), saved_ca);
    let server = axum_server::tls_rustls::RustlsConfig::from_pem(
        certs.into_bytes(),
        leaf_key.serialize_pem().into_bytes(),
    )
    .await?;
    let der = saved_ca
        .lines()
        .filter(|s| !s.starts_with("---"))
        .collect::<String>();
    let id = uuid::Uuid::new_v4();
    let cert_id = uuid::Uuid::new_v4();
    let profile = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd"><plist version="1.0"><dict><key>PayloadContent</key><array><dict><key>PayloadType</key><string>com.apple.security.root</string><key>PayloadVersion</key><integer>1</integer><key>PayloadIdentifier</key><string>local.doubao-ptt.ca</string><key>PayloadUUID</key><string>{cert_id}</string><key>PayloadDisplayName</key><string>Doubao PTT Local CA</string><key>PayloadContent</key><data>{der}</data></dict></array><key>PayloadType</key><string>Configuration</string><key>PayloadVersion</key><integer>1</integer><key>PayloadIdentifier</key><string>local.doubao-ptt.profile</string><key>PayloadUUID</key><string>{id}</string><key>PayloadDisplayName</key><string>豆包语音遥控 · 本机证书</string><key>PayloadDescription</key><string>仅用于信任这台电脑提供的局域网语音遥控网页。卸载时可在设置中移除此描述文件。</string></dict></plist>"#
    );
    Ok(TlsMaterial {
        server: Arc::new(server),
        profile,
    })
}

fn load_or_create_ca(dir: &Path, mock: bool) -> Result<KeyPair> {
    if mock {
        let path = dir.join("mock-ca-key.pem");
        if path.exists() {
            return Ok(KeyPair::from_pem(&std::fs::read_to_string(path)?)?);
        }
        ensure!(!dir.join("ca.pem").exists(), "mock CA key missing");
        let key = KeyPair::generate()?;
        crate::config::atomic_write(&path, key.serialize_pem().as_bytes())?;
        return Ok(key);
    }
    #[cfg(target_os = "macos")]
    {
        let account = format!("ca-{}", crate::identity::digest(&dir.to_string_lossy()));
        let entry = keyring::Entry::new("local.doubao-ptt", &account)?;
        match entry.get_password() {
            Ok(pem) => return Ok(KeyPair::from_pem(&pem)?),
            Err(keyring::Error::NoEntry) => {
                ensure!(
                    !dir.join("ca.pem").exists(),
                    "CA 私钥丢失，保留现有证书；请人工重置并重新配对"
                );
                let key = KeyPair::generate()?;
                entry.set_password(&key.serialize_pem())?;
                return Ok(key);
            }
            Err(e) => return Err(e.into()),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let path = dir.join("ca-key.dpapi");
        if path.exists() {
            let bytes = unprotect(&std::fs::read(path)?)?;
            return Ok(KeyPair::from_pem(std::str::from_utf8(&bytes)?)?);
        }
        ensure!(
            !dir.join("ca.pem").exists(),
            "CA 私钥丢失，保留现有证书；请人工重置并重新配对"
        );
        let key = KeyPair::generate()?;
        crate::config::atomic_write(&path, &protect(key.serialize_pem().as_bytes())?)?;
        return Ok(key);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        ensure!(mock, "本平台仅允许 --mock 模式");
        let path = dir.join("mock-ca-key.pem");
        if path.exists() {
            return Ok(KeyPair::from_pem(&std::fs::read_to_string(path)?)?);
        }
        ensure!(!dir.join("ca.pem").exists(), "mock CA key missing");
        let key = KeyPair::generate()?;
        crate::config::atomic_write(&path, key.serialize_pem().as_bytes())?;
        Ok(key)
    }
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct Blob {
    len: u32,
    data: *mut u8,
}
#[cfg(target_os = "windows")]
#[link(name = "crypt32")]
unsafe extern "system" {
    fn CryptProtectData(
        input: *const Blob,
        description: *const u16,
        entropy: *const Blob,
        reserved: *const std::ffi::c_void,
        prompt: *const std::ffi::c_void,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
    fn CryptUnprotectData(
        input: *const Blob,
        description: *mut *mut u16,
        entropy: *const Blob,
        reserved: *const std::ffi::c_void,
        prompt: *const std::ffi::c_void,
        flags: u32,
        output: *mut Blob,
    ) -> i32;
}
#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn LocalFree(p: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
}
#[cfg(target_os = "windows")]
fn crypt(bytes: &[u8], decrypt: bool) -> Result<Vec<u8>> {
    ensure!(bytes.len() <= u32::MAX as usize, "DPAPI input too large");
    let input = Blob {
        len: bytes.len() as u32,
        data: bytes.as_ptr() as *mut u8,
    };
    let mut out = Blob {
        len: 0,
        data: std::ptr::null_mut(),
    };
    let ok = unsafe {
        if decrypt {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut out,
            )
        } else {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut out,
            )
        }
    };
    ensure!(ok != 0, "DPAPI 失败：{}", std::io::Error::last_os_error());
    let result = unsafe { std::slice::from_raw_parts(out.data, out.len as usize).to_vec() };
    unsafe {
        LocalFree(out.data.cast());
    }
    Ok(result)
}
#[cfg(target_os = "windows")]
fn protect(b: &[u8]) -> Result<Vec<u8>> {
    crypt(b, false)
}
#[cfg(target_os = "windows")]
fn unprotect(b: &[u8]) -> Result<Vec<u8>> {
    crypt(b, true)
}

pub fn fingerprint(dir: &Path) -> Result<String> {
    let pem = std::fs::read_to_string(dir.join("ca.pem"))?;
    let b64 = pem
        .lines()
        .filter(|s| !s.starts_with("---"))
        .collect::<String>();
    let bytes = STANDARD.decode(b64).context("invalid CA PEM")?;
    use sha2::Digest;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}
