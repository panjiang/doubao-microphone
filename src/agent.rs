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
    Press {
        key: Shortcut,
        #[serde(default)]
        codex_empty_compatibility: bool,
    },
    Renew,
    Release,
    TapEnter,
    Inspect,
    Exit,
}
#[derive(Serialize, Deserialize)]
struct Envelope {
    id: u64,
    request: Request,
}
#[derive(Serialize, Deserialize)]
struct Reply {
    id: u64,
    ok: bool,
    error: Option<String>,
    #[serde(default)]
    voice: Option<crate::platform::VoiceState>,
    #[serde(default)]
    codex_primed: bool,
    #[serde(default)]
    action_status: Option<crate::protocol::ActionStatus>,
    #[serde(default)]
    action_reason: Option<String>,
    #[serde(default)]
    accessibility: Option<bool>,
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
            let Ok(req) = serde_json::from_str::<Envelope>(&line) else {
                break;
            };
            if tx.send(req).is_err() {
                break;
            }
        }
    });
    let mut keyboard = Keyboard::new(mock);
    let mut deadline = None;
    let mut enter_target = None;
    loop {
        let wait = deadline
            .map(|d: Instant| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_secs(1));
        let req = match rx.recv_timeout(wait) {
            Ok(r) => r,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                keyboard.release()?;
                if deadline.is_some() {
                    enter_target = None;
                }
                deadline = None;
                continue;
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                keyboard.release()?;
                return Ok(());
            }
        };
        let mut voice = None;
        let mut codex_primed = false;
        let mut action_status = None;
        let mut action_reason = None;
        let mut accessibility = None;
        let result = match req.request {
            Request::Press {
                key,
                codex_empty_compatibility,
            } => {
                let target = crate::enter::capture();
                enter_target = None;
                let r = keyboard
                    .press_configured(key, codex_empty_compatibility)
                    .map(|primed| codex_primed = primed);
                if r.is_ok() {
                    enter_target = target;
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
            Request::Inspect => {
                accessibility = Some(crate::platform::accessibility());
                Ok(())
            }
            Request::TapEnter => {
                if deadline.is_some() {
                    enter_target = None;
                    Err(anyhow::anyhow!("语音快捷键尚未释放，拒绝 Enter"))
                } else {
                    let target = enter_target.take();
                    let result = if mock {
                        Ok(Some("mock"))
                    } else {
                        crate::enter::tap(target)
                    };
                    match result {
                        Ok(reason) => {
                            action_status = Some(if reason.is_some() {
                                crate::protocol::ActionStatus::Skipped
                            } else {
                                crate::protocol::ActionStatus::Executed
                            });
                            action_reason = Some(reason.unwrap_or("enter_sent").into());
                            Ok(())
                        }
                        Err(e) => Err(e),
                    }
                }
            }
            Request::Exit => {
                keyboard.release()?;
                return Ok(());
            }
        };
        let reply = Reply {
            id: req.id,
            ok: result.is_ok(),
            error: result.err().map(|e| format!("{e:#}")),
            voice,
            codex_primed,
            action_status,
            action_reason,
            accessibility,
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
    pub codex_primed: bool,
    next_request: u64,
    ipc_failed: bool,
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
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true);
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
            codex_primed: false,
            next_request: 1,
            ipc_failed: false,
        })
    }
    async fn send(&mut self, id: u64, r: Request) -> Result<()> {
        let mut b = serde_json::to_vec(&Envelope { id, request: r })?;
        b.push(b'\n');
        self.input.write_all(&b).await?;
        self.input.flush().await?;
        Ok(())
    }
    async fn request(&mut self, r: Request) -> Result<Reply> {
        ensure!(!self.ipc_failed, "输入代理通信已中止");
        let id = self.next_request;
        self.next_request = id.checked_add(1).context("输入代理请求编号已耗尽")?;
        let expects_enter = matches!(r, Request::TapEnter);
        let expects_accessibility = matches!(r, Request::Inspect);
        let timeout = if matches!(r, Request::Press { .. }) {
            750
        } else {
            500
        };
        // A timed-out read may leave a partial or late reply in the pipe. Never
        // reuse that channel: otherwise a Release/Inspect reply can be mistaken
        // for the acknowledgement of TapEnter or the next Press.
        let result: Result<Reply> = async {
            tokio::time::timeout(Duration::from_millis(timeout), async {
                self.send(id, r).await?;
                let mut line = String::new();
                let n = self.output.read_line(&mut line).await?;
                ensure!(n > 0, "输入代理已退出");
                let reply: Reply = serde_json::from_str(&line)?;
                ensure!(reply.id == id, "输入代理回复编号不匹配");
                if !reply.ok {
                    return Err(
                        Rejected(reply.error.unwrap_or_else(|| "输入代理拒绝操作".into())).into(),
                    );
                }
                if expects_enter {
                    ensure!(
                        reply.action_status.is_some() && reply.action_reason.is_some(),
                        "输入代理未返回完整 Enter 结果"
                    );
                }
                if expects_accessibility {
                    ensure!(reply.accessibility.is_some(), "输入代理未返回权限状态");
                }
                Ok(reply)
            })
            .await
            .context("输入代理响应超时")?
        }
        .await;
        if let Err(error) = &result
            && !error.is::<Rejected>()
        {
            tracing::error!(request_id=id,error=%format!("{error:#}"),"input agent IPC invalidated");
            self.ipc_failed = true;
            // Stop pending work before emergency release, so a delayed Press or
            // Enter cannot execute after we have reported the channel as failed.
            let _ = self.stop_process().await;
            self.release_emergency();
        }
        result
    }
    pub async fn press(&mut self, key: Shortcut) -> Result<()> {
        self.press_configured(key, false).await
    }
    pub async fn press_configured(
        &mut self,
        key: Shortcut,
        codex_empty_compatibility: bool,
    ) -> Result<()> {
        // Remember before IPC: a lost acknowledgement must still trigger emergency release.
        self.held = Some(key);
        self.codex_primed = false;
        let result = self
            .request(Request::Press {
                key,
                codex_empty_compatibility,
            })
            .await;
        if let Ok(reply) = &result {
            self.codex_primed = reply.codex_primed;
        }
        if let Err(e) = result {
            // A positively rejected press did not inject anything: do not send
            // key-up for a modifier the person may be physically holding.
            if e.is::<Rejected>()
                && !matches!(
                    key,
                    Shortcut::DoubaoVoice | Shortcut::HidRightAlt | Shortcut::HidRightControl
                )
            {
                self.held = None;
            } else {
                self.release_emergency();
            }
            return Err(e);
        }
        Ok(())
    }
    pub async fn renew(&mut self) -> Result<Option<crate::platform::VoiceState>> {
        ensure!(!self.ipc_failed, "输入代理通信已中止");
        if self.child.try_wait()?.is_some() {
            self.release_emergency();
            anyhow::bail!("输入代理异常退出");
        }
        if self.held.is_some() {
            return Ok(self.request(Request::Renew).await?.voice);
        }
        Ok(None)
    }
    pub fn failed(&mut self) -> bool {
        self.ipc_failed || !matches!(self.child.try_wait(), Ok(None))
    }
    async fn stop_process(&mut self) -> Result<()> {
        if self.child.try_wait()?.is_none() {
            self.child.start_kill()?;
            tokio::time::timeout(Duration::from_millis(500), self.child.wait())
                .await
                .context("旧输入代理未能及时退出")??;
        }
        Ok(())
    }
    fn release_emergency_checked(&mut self) -> Result<()> {
        if let Some(key) = self.held {
            Keyboard::emergency_owned(key, self.mock, &self.voice_owner)?;
            // If killing timed out, the old process could still press later.
            // Keep ownership until a subsequent retry confirms its exit.
            if self.ipc_failed {
                ensure!(
                    self.child.try_wait()?.is_some(),
                    "旧输入代理尚未退出，按键释放仍需复核"
                );
            }
            self.held = None;
        }
        Ok(())
    }
    pub async fn recover(&mut self) -> Result<()> {
        self.ipc_failed = true;
        // No replacement may exist until both old-process exit and release have
        // succeeded. Keep held on a failed emergency release for the next retry.
        self.stop_process().await?;
        self.release_emergency_checked()?;
        let mut replacement = Self::spawn(self.mock).await?;
        if let Err(error) = replacement.inspect_accessibility().await {
            // Retain this object if cleanup fails, so a later retry must stop
            // this process before it can create yet another replacement.
            *self = replacement;
            return Err(error.context("新输入代理握手失败"));
        }
        *self = replacement;
        Ok(())
    }
    pub async fn tap_enter(&mut self) -> Result<(crate::protocol::ActionStatus, String)> {
        ensure!(self.held.is_none(), "语音快捷键尚未释放");
        let reply = self.request(Request::TapEnter).await?;
        Ok((
            reply.action_status.context("输入代理未返回 Enter 结果")?,
            reply.action_reason.unwrap_or_default(),
        ))
    }
    pub async fn inspect_accessibility(&mut self) -> Result<bool> {
        self.request(Request::Inspect)
            .await?
            .accessibility
            .context("输入代理未返回权限状态")
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
        if let Err(error) = self.release_emergency_checked() {
            tracing::error!(error=%error,"emergency shortcut release failed");
        }
    }
    pub async fn shutdown(&mut self) {
        let _ = self.release().await;
        if !self.ipc_failed {
            let _ = self.send(0, Request::Exit).await;
        }
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        self.release_emergency(); /* Closing stdin makes the agent release and exit. */
    }
}
