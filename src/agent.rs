//! Private stdin/stdout IPC; this protocol is never exposed to the network.
use crate::platform::{Keyboard, Shortcut};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    io::{BufRead, Write},
    time::{Duration, Instant},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Press { key: Shortcut },
    Renew,
    Release,
    Exit,
}
#[derive(Serialize, Deserialize)]
struct Reply {
    ok: bool,
    error: Option<String>,
    #[serde(default)]
    voice: Option<crate::platform::VoiceState>,
}
#[derive(Debug)]
struct Rejected(String);
impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for Rejected {}

pub fn run(mock: bool) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::sync_channel(16);
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if line.len() > 1024 {
                break;
            }
            let Ok(req) = serde_json::from_str::<Request>(&line) else {
                break;
            };
            if tx.send(req).is_err() {
                break;
            }
        }
    });
    let mut keyboard = Keyboard::new(mock);
    let mut deadline = None;
    loop {
        let wait = deadline
            .map(|d: Instant| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(1));
        let req = match rx.recv_timeout(wait) {
            Ok(r) => r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                keyboard.release()?;
                deadline = None;
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                keyboard.release()?;
                return Ok(());
            }
        };
        let mut voice = None;
        let result = match req {
            Request::Press { key } => {
                let r = keyboard.press(key);
                if r.is_ok() {
                    deadline = Some(Instant::now() + Duration::from_secs(1));
                }
                r
            }
            Request::Renew => {
                if deadline.is_some() {
                    deadline = Some(Instant::now() + Duration::from_secs(1));
                    keyboard.voice_state().map(|state| voice = state)
                } else {
                    Err(anyhow::anyhow!(
                        "input lease expired; shortcut was released"
                    ))
                }
            }
            Request::Release => {
                deadline = None;
                keyboard.release()
            }
            Request::Exit => {
                keyboard.release()?;
                return Ok(());
            }
        };
        let reply = Reply {
            ok: result.is_ok(),
            error: result.err().map(|e| e.to_string()),
            voice,
        };
        let mut out = std::io::stdout().lock();
        serde_json::to_writer(&mut out, &reply)?;
        out.write_all(b"\n")?;
        out.flush()?;
    }
}

pub struct Agent {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    pub held: Option<Shortcut>,
    mock: bool,
    voice_owner: String,
}
impl Agent {
    pub async fn spawn(mock: bool) -> Result<Self> {
        let exe = std::env::current_exe()?;
        #[cfg(target_os = "macos")]
        let exe = {
            let helper = exe
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.join("Resources/DoubaoPTTInput.app/Contents/MacOS/doubao-ptt"));
            helper.filter(|p| p.is_file()).unwrap_or(exe)
        };
        let mut cmd = tokio::process::Command::new(exe);
        let voice_owner = uuid::Uuid::new_v4().to_string();
        cmd.env("DOUBAO_PTT_VOICE_OWNER", &voice_owner);
        cmd.arg("input-agent");
        if mock {
            cmd.arg("--mock");
        }
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit());
        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().context("无法启动输入代理")?;
        let input = child.stdin.take().context("agent stdin")?;
        let output = BufReader::new(child.stdout.take().context("agent stdout")?);
        Ok(Self {
            child,
            input,
            output,
            held: None,
            mock,
            voice_owner,
        })
    }
    async fn send(&mut self, r: Request) -> Result<()> {
        let mut b = serde_json::to_vec(&r)?;
        b.push(b'\n');
        self.input.write_all(&b).await?;
        self.input.flush().await?;
        Ok(())
    }
    async fn request(&mut self, r: Request) -> Result<Reply> {
        self.send(r).await?;
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_millis(500), self.output.read_line(&mut line))
            .await??;
        ensure!(n > 0, "输入代理已退出");
        let reply: Reply = serde_json::from_str(&line)?;
        if !reply.ok {
            return Err(Rejected(reply.error.unwrap_or_else(|| "输入代理拒绝操作".into())).into());
        }
        Ok(reply)
    }
    pub async fn press(&mut self, key: Shortcut) -> Result<()> {
        // Remember before IPC: a lost acknowledgement must still trigger emergency release.
        self.held = Some(key);
        if let Err(e) = self.request(Request::Press { key }).await {
            // A positively rejected press did not inject anything: do not send
            // key-up for a modifier the person may be physically holding.
            if e.is::<Rejected>() && key != Shortcut::DoubaoVoice {
                self.held = None;
            } else {
                self.release_emergency();
            }
            return Err(e);
        }
        Ok(())
    }
    pub async fn renew(&mut self) -> Result<Option<crate::platform::VoiceState>> {
        if self.child.try_wait()?.is_some() {
            self.release_emergency();
            anyhow::bail!("输入代理异常退出");
        }
        if self.held.is_some() {
            return Ok(self.request(Request::Renew).await?.voice);
        }
        Ok(None)
    }
    pub async fn release(&mut self) -> Result<()> {
        if self.held.is_none() {
            return Ok(());
        }
        let r = self.request(Request::Release).await.map(|_| ());
        if r.is_err() {
            self.release_emergency();
        } else {
            self.held = None;
        }
        r
    }
    pub fn release_emergency(&mut self) {
        if let Some(key) = self.held.take() {
            let _ = Keyboard::emergency_owned(key, self.mock, &self.voice_owner);
        }
    }
    pub async fn shutdown(&mut self) {
        let _ = self.release().await;
        let _ = self.send(Request::Exit).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        self.release_emergency(); /* Closing stdin makes the agent release and exit. */
    }
}
