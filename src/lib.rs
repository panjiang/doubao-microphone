pub mod agent;
pub mod audio;
#[cfg(target_os = "windows")]
mod codex_input;
pub mod config;
pub mod controller;
pub mod diagnostics;
pub mod environment;
mod enter;
pub mod identity;
pub mod platform;
pub mod protocol;
pub mod server;
pub mod session;
pub mod tls;
pub mod tray;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
mod macos_environment;
#[cfg(target_os = "windows")]
mod windows_umdf;
#[cfg(target_os = "windows")]
mod windows_voice;
#[cfg(target_os = "windows")]
mod windows_environment;
