use crate::{
    agent::Agent,
    audio::Audio,
    config::Settings,
    protocol::{ActionStatus, AudioFrame, EndAction, ServerMessage},
    session::{Phase, Session, Transition},
};
use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, mpsc, oneshot, watch};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Status {
    pub phase: Phase,
    pub connected: bool,
    pub paused: bool,
    pub mock: bool,
    pub queued_ms: u64,
    pub error: Option<String>,
    pub session_id: Option<Uuid>,
    pub testing: bool,
}

enum Command {
    Connect {
        id: Uuid,
        connection: Uuid,
        rate: u32,
        events: mpsc::Sender<ServerMessage>,
        reply: oneshot::Sender<Result<()>>,
    },
    Start(Uuid, Uuid, oneshot::Sender<Result<()>>),
    Frame(Uuid, AudioFrame, oneshot::Sender<Result<()>>),
    End(
        Uuid,
        Uuid,
        Option<u32>,
        EndAction,
        oneshot::Sender<Result<()>>,
    ),
    Cancel(Uuid, Uuid),
    Heartbeat(Uuid),
    Disconnect(Uuid),
    Revoke(Uuid),
    Pause(bool),
    TestShortcut(oneshot::Sender<Result<()>>),
    InspectInput(oneshot::Sender<Result<bool>>),
    Configure(Settings, std::path::PathBuf, oneshot::Sender<Result<()>>),
    Shutdown(oneshot::Sender<()>),
}

#[derive(Clone)]
pub struct Controller {
    tx: mpsc::Sender<Command>,
    pub status: watch::Receiver<Status>,
    pub settings: Arc<Mutex<Settings>>,
}
impl Controller {
    pub async fn spawn(
        settings: Settings,
        mock: bool,
        data_dir: std::path::PathBuf,
    ) -> Result<Self> {
        let agent = Agent::spawn(mock).await?;
        let audio = Audio::new(mock);
        let (tx, rx) = mpsc::channel(16);
        let (status_tx, status) = watch::channel(Status {
            phase: Phase::Disconnected,
            connected: false,
            paused: false,
            mock,
            queued_ms: 0,
            error: None,
            session_id: None,
            testing: false,
        });
        let settings = Arc::new(Mutex::new(settings));
        let a = Actor {
            session: Session::new(Duration::from_millis(settings.lock().await.warmup_ms)),
            settings: settings.clone(),
            agent,
            audio,
            rx,
            status: status_tx,
            events: None,
            device: None,
            connection: None,
            epoch: Instant::now(),
            error: None,
            paused: false,
            mock,
            key_test: None,
            diagnostic: None,
            data_dir,
            phone_rate: 48000,
            last_voice: None,
            end_action: EndAction::Finish,
            pending_enter: None,
        };
        tokio::spawn(a.run());
        Ok(Self {
            tx,
            status,
            settings,
        })
    }
    async fn call(&self, build: impl FnOnce(oneshot::Sender<Result<()>>) -> Command) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(build(tx)).await?;
        tokio::time::timeout(Duration::from_secs(5), rx).await??
    }
    pub async fn connect(
        &self,
        id: Uuid,
        connection: Uuid,
        rate: u32,
        events: mpsc::Sender<ServerMessage>,
    ) -> Result<()> {
        self.call(|reply| Command::Connect {
            id,
            connection,
            rate,
            events,
            reply,
        })
        .await
    }
    pub async fn start(&self, connection: Uuid, id: Uuid) -> Result<()> {
        self.call(|r| Command::Start(connection, id, r)).await
    }
    pub async fn frame(&self, connection: Uuid, f: AudioFrame) -> Result<()> {
        self.call(|r| Command::Frame(connection, f, r)).await
    }
    pub async fn end(
        &self,
        connection: Uuid,
        id: Uuid,
        last: Option<u32>,
        action: EndAction,
    ) -> Result<()> {
        self.call(|r| Command::End(connection, id, last, action, r))
            .await
    }
    pub async fn cancel(&self, connection: Uuid, id: Uuid) {
        let _ = self.tx.send(Command::Cancel(connection, id)).await;
    }
    pub async fn heartbeat(&self, connection: Uuid) {
        let _ = self.tx.send(Command::Heartbeat(connection)).await;
    }
    pub async fn disconnect(&self, id: Uuid) {
        let _ = self.tx.send(Command::Disconnect(id)).await;
    }
    pub async fn revoke(&self, id: Uuid) {
        let _ = self.tx.send(Command::Revoke(id)).await;
    }
    pub async fn pause(&self, p: bool) {
        let _ = self.tx.send(Command::Pause(p)).await;
    }
    pub async fn configure(&self, s: Settings, dir: std::path::PathBuf) -> Result<()> {
        self.call(|r| Command::Configure(s, dir, r)).await
    }
    pub async fn test_shortcut(&self) -> Result<()> {
        self.call(Command::TestShortcut).await
    }
    pub async fn input_accessibility(&self) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Command::InspectInput(tx)).await?;
        tokio::time::timeout(Duration::from_secs(1), rx).await??
    }
    pub async fn shutdown(&self) {
        let (tx, rx) = oneshot::channel();
        if self.tx.send(Command::Shutdown(tx)).await.is_ok() {
            let _ = tokio::time::timeout(Duration::from_secs(3), rx).await;
        }
    }
}

struct PendingEnter {
    id: Uuid,
    connection: Uuid,
    deadline: Instant,
}

struct Actor {
    session: Session,
    settings: Arc<Mutex<Settings>>,
    agent: Agent,
    audio: Audio,
    rx: mpsc::Receiver<Command>,
    status: watch::Sender<Status>,
    events: Option<mpsc::Sender<ServerMessage>>,
    device: Option<Uuid>,
    connection: Option<Uuid>,
    epoch: Instant,
    error: Option<String>,
    paused: bool,
    mock: bool,
    key_test: Option<(Instant, bool)>,
    diagnostic: Option<crate::diagnostics::Recorder>,
    data_dir: std::path::PathBuf,
    phone_rate: u32,
    last_voice: Option<crate::platform::VoiceState>,
    end_action: EndAction,
    pending_enter: Option<PendingEnter>,
}
impl Actor {
    async fn run(mut self) {
        let mut ticks = tokio::time::interval(Duration::from_millis(10));
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut renewal = Instant::now();
        let mut last_tick = Instant::now();
        loop {
            tokio::select! {
                biased;
                command=self.rx.recv()=>{
                    let Some(command)=command else{break};
                    match command {
                        Command::Connect{id,connection,rate,events,reply}=>{
                            let r=async{
                                ensure!(self.device.is_none(),"已有一部手机连接，请先断开");ensure!(!self.paused,"电脑已暂停接收");ensure!(self.key_test.is_none(),"快捷键测试中，请稍后连接");
                                self.audio.open(self.settings.lock().await.output_device.clone(),rate).await?;
                                self.phone_rate=rate;
                                self.device=Some(id);self.connection=Some(connection);self.events=Some(events);self.session.connect(self.epoch.elapsed());self.error=None;Ok(())
                            }.await;let _=reply.send(r);
                        }
                        Command::Start(connection,id,reply)=>{
                            let r=async {
                                ensure!(!self.paused && self.connection==Some(connection),"接收已暂停或未连接");
                                if self.session.id==Some(id){return Ok(());}
                                self.cancel_enter("new_session");
                                ensure!(!self.audio.failed(),"音频设备异常，请重新连接");
                                self.session.begin(id,self.epoch.elapsed())?;
                                self.end_action=EndAction::Finish;
                                self.error=None;self.last_voice=None;
                                let settings=self.settings.lock().await.clone();
                                if settings.diagnostic_recording && !self.mock {
                                    match crate::diagnostics::Recorder::start(self.data_dir.join("diagnostics"),id,self.phone_rate) {
                                        Ok(recorder)=>self.diagnostic=Some(recorder),
                                        Err(e)=>tracing::error!(error=%e,"diagnostic recorder unavailable"),
                                    }
                                }
                                let key=settings.shortcut;
                                if let Err(e)=self.agent.press_configured(key,settings.codex_empty_compatibility).await{
                                    self.error=Some(format!("{e:#}"));
                                    if let Some(d)=&self.diagnostic {d.event(format!("trigger_failed error={e:#}"));}
                                    self.abort("shortcut_failed").await;return Err(e);
                                }
                                self.session.shortcut_pressed(self.epoch.elapsed());
                                if let Some(d)=&self.diagnostic {d.event(format!("trigger_started codex_primed={}",self.agent.codex_primed));}
                                Ok(())
                            }.await;let _=reply.send(r);
                        }
                        Command::Frame(connection,f,reply)=>{
                            if self.connection!=Some(connection){let _=reply.send(Err(anyhow::anyhow!("stale connection")));continue;}
                            let r=async {
                                if self.session.frame(f.session_id,f.sequence,self.epoch.elapsed())? {
                                    if let Some(d)=&self.diagnostic {d.phone(&f.samples);}
                                    self.audio.push(f.samples).await?;
                                }
                                Ok(())
                            }.await;
                            if let Err(e)=&r {self.error=Some(format!("{e:#}"));tracing::warn!(error=%e,"audio frame rejected");self.abort("audio_invalid").await;}let _=reply.send(r);
                        }
                        Command::End(connection,id,last,action,reply)=>{
                            let ending=self.epoch.elapsed();
                            if self.connection!=Some(connection){let _=reply.send(Err(anyhow::anyhow!("stale connection")));continue;}
                            if self.session.id==Some(id) && matches!(self.session.phase,Phase::Priming|Phase::Streaming) {
                                self.end_action=action;
                            }
                            if self.session.id==Some(id) && let Some(d)=&self.diagnostic {d.event(format!("end_received queued_ms={}",self.audio.queued_ms()));}
                            if self.session.id==Some(id)&&self.session.phase==Phase::Streaming
                                && let Err(e)=self.audio.flush().await{
                                    self.error=Some(format!("{e:#}"));
                                    self.abort("audio_tail_failed").await;let _=reply.send(Err(e));continue;
                            }
                            let r=self.session.end(id,last,ending);
                            match r {Ok(Some(t))=>{self.apply(t).await;let _=reply.send(Ok(()));},Ok(None)=>{let _=reply.send(Ok(()));},Err(e)=>{self.abort("missing_tail").await;let _=reply.send(Err(e));}}
                        }
                        Command::Cancel(connection,id)=>{
                            if self.connection==Some(connection) {
                                if self.pending_enter.as_ref().is_some_and(|p|p.id==id){self.cancel_enter("cancelled");}
                                if self.session.id==Some(id){self.abort("cancelled").await;}
                            }
                        }
                        Command::Heartbeat(connection)=>{if self.connection==Some(connection){self.session.heartbeat=self.epoch.elapsed();}}
                        Command::Disconnect(id)=>{if self.connection==Some(id){self.drop_connection("disconnected").await;}}
                        Command::Revoke(id)=>{if self.device==Some(id){self.drop_connection("revoked").await;}}
                        Command::Pause(p)=>{self.paused=p;if p{self.cancel_enter("paused");self.abort("paused").await;self.key_test=None;let _=self.agent.release().await;}}
                        Command::TestShortcut(reply)=>{
                            let r=if self.device.is_some()||self.session.active()||self.key_test.is_some()||self.paused {Err(anyhow::anyhow!("请先断开手机并恢复接收，再测试快捷键"))}else{self.key_test=Some((Instant::now()+Duration::from_secs(3),false));self.error=None;Ok(())};
                            let _=reply.send(r);
                        }
                        Command::InspectInput(reply)=>{let _=reply.send(self.agent.inspect_accessibility().await);}
                        Command::Configure(s,dir,reply)=>{
                            self.cancel_enter("configuration_changed");
                            let r=async {
                                ensure!(!self.session.active() && self.key_test.is_none(),"听写或测试中不能修改配置");s.validate()?;
                                let old=self.settings.lock().await.clone();
                                if s.autostart!=old.autostart {crate::platform::set_autostart(s.autostart)?;}
                                if let Err(e)=s.save(&dir){if s.autostart!=old.autostart{let _=crate::platform::set_autostart(old.autostart);}return Err(e);}
                                self.session.set_warmup(s.warmup_ms)?;*self.settings.lock().await=s;Ok(())
                            }.await;
                            if r.is_ok() && self.device.is_some(){self.drop_connection("configuration_changed").await;}
                            let _=reply.send(r);
                        }
                        Command::Shutdown(reply)=>{self.cancel_enter("shutdown");self.abort("shutdown").await;self.audio.close().await;self.agent.shutdown().await;let _=reply.send(());return;}
                    }
                }
                _=ticks.tick()=>{
                    if last_tick.elapsed()>=Duration::from_secs(1) {
                        self.drop_connection("resume_required").await;
                        self.key_test=None;let _=self.agent.release().await;
                    }
                    last_tick=Instant::now();
                    if let Some((deadline,pressed))=self.key_test
                        && Instant::now()>=deadline {
                            if pressed {if let Err(e)=self.agent.release().await{self.error=Some(e.to_string());self.paused=true;}self.key_test=None;}
                            else {
                                let key=self.settings.lock().await.shortcut;
                                match self.agent.press(key).await {Ok(())=>self.key_test=Some((Instant::now()+Duration::from_secs(3),true)),Err(e)=>{self.error=Some(e.to_string());self.key_test=None;}}
                            }
                    }
                    if self.session.active() && self.audio.failed(){self.abort("audio_device_failed").await;}
                    if let Some(t)=self.session.tick(self.epoch.elapsed(),self.audio.queued_ms()){self.apply(t).await;}
                    if self.device.is_some() && self.epoch.elapsed().saturating_sub(self.session.heartbeat)>=Duration::from_secs(1){
                        self.drop_connection("heartbeat_timeout").await;
                    }
                    if self.pending_enter.as_ref().is_some_and(|p|Instant::now()>=p.deadline) {
                        self.finish_enter().await;
                    }
                    if renewal.elapsed()>=Duration::from_millis(250){
                        match self.agent.renew().await {
                            Ok(Some(voice))=>{
                                if self.last_voice.as_ref()!=Some(&voice) {
                                    if let Some(d)=&self.diagnostic {d.event(format!("doubao recording={} panel_visible={} capture_sessions={}",voice.recording,voice.panel_visible,voice.capture_sessions));}
                                    self.last_voice=Some(voice.clone());
                                }
                                if let Some(t)=self.session.target_state(voice.recording,self.epoch.elapsed()) {
                                    self.error=Some("豆包提前结束录音，请重新按住；已启用诊断时会保存本次样本".into());
                                    self.apply(t).await;
                                }
                            }
                            Ok(None)=>{},
                            Err(e)=>{self.error=Some(e.to_string());self.paused=true;self.abort("input_agent_failed").await;}
                        }
                        renewal=Instant::now();
                    }
                }
            }
            let state = Status {
                phase: self.session.phase,
                connected: self.device.is_some(),
                paused: self.paused,
                mock: self.mock,
                queued_ms: self.audio.queued_ms(),
                error: self.error.clone(),
                session_id: self.session.id,
                testing: self.key_test.is_some(),
            };
            self.status.send_if_modified(|old| {
                if *old == state {
                    false
                } else {
                    *old = state;
                    true
                }
            });
        }
        self.cancel_enter("shutdown");
        self.abort("shutdown").await;
        self.audio.close().await;
        self.agent.shutdown().await;
    }
    async fn drop_connection(&mut self, reason: &'static str) {
        self.cancel_enter(reason);
        self.abort(reason).await;
        self.device = None;
        self.connection = None;
        self.events = None;
        self.session.phase = Phase::Disconnected;
        self.audio.close().await;
    }
    async fn abort(&mut self, reason: &'static str) {
        if let Some(t) = self.session.abort(reason) {
            self.apply(t).await;
        }
    }
    async fn apply(&mut self, t: Transition) {
        match t {
            Transition::Ready(id) => {
                self.audio.activate();
                if let Some(d) = &self.diagnostic {
                    d.event(format!("streaming queued_ms={}", self.audio.queued_ms()));
                }
                self.emit(ServerMessage::Ready { session_id: id });
            }
            Transition::Ended {
                id,
                complete,
                reason,
            } => {
                if let Some(d) = &self.diagnostic {
                    d.event(format!(
                        "ending reason={reason} queued_ms={}",
                        self.audio.queued_ms()
                    ));
                }
                // Stop output and release the shortcut concurrently; no new session
                // is acknowledged until old audio has actually been cleared.
                let (cleared, released) = tokio::join!(self.audio.clear(), self.agent.release());
                let complete = complete && cleared.is_ok() && released.is_ok();
                if let Err(e) = cleared {
                    self.error = Some(format!("{e:#}"));
                    self.paused = true;
                }
                if let Err(e) = released {
                    self.error = Some(e.to_string());
                    self.paused = true;
                }
                let release_time = Instant::now();
                let wants_enter = std::mem::take(&mut self.end_action) == EndAction::Enter;
                let enter_pending = wants_enter
                    && complete
                    && reason == "completed"
                    && crate::enter::supported()
                    && !self.paused
                    && self.connection.is_some();
                if enter_pending {
                    let delay = self.settings.lock().await.enter_delay_ms;
                    self.pending_enter = Some(PendingEnter {
                        id,
                        connection: self.connection.unwrap(),
                        deadline: release_time + Duration::from_millis(delay),
                    });
                }
                tracing::info!(session=%id,complete,reason,"dictation ended");
                if let Some(d) = self.diagnostic.take() {
                    d.finish(complete, reason);
                }
                self.emit(ServerMessage::Ended {
                    session_id: id,
                    complete,
                    reason: reason.into(),
                    enter_pending,
                });
                if wants_enter && !enter_pending {
                    self.emit(ServerMessage::ActionResult {
                        session_id: id,
                        status: ActionStatus::Skipped,
                        reason: if complete { "unsupported" } else { reason }.into(),
                    });
                }
            }
        }
    }
    fn cancel_enter(&mut self, reason: &str) {
        if let Some(pending) = self.pending_enter.take() {
            self.emit(ServerMessage::ActionResult {
                session_id: pending.id,
                status: ActionStatus::Skipped,
                reason: reason.into(),
            });
        }
    }
    async fn finish_enter(&mut self) {
        let Some(pending) = self.pending_enter.take() else {
            return;
        };
        let blocked = self.paused
            || self.connection != Some(pending.connection)
            || self.session.active()
            || self.audio.failed()
            || self.epoch.elapsed().saturating_sub(self.session.heartbeat) >= crate::session::LEASE;
        let (status, reason) = if blocked {
            (ActionStatus::Skipped, "session_changed".into())
        } else {
            match self.agent.tap_enter().await {
                Ok(result) => result,
                Err(e) => {
                    self.error = Some(format!("回车未完成：{e:#}"));
                    self.paused = true;
                    (ActionStatus::Failed, "input_failed".into())
                }
            }
        };
        tracing::info!(session=%pending.id,?status,reason,"Enter action completed");
        self.emit(ServerMessage::ActionResult {
            session_id: pending.id,
            status,
            reason,
        });
    }
    fn emit(&mut self, e: ServerMessage) {
        if let Some(tx) = &self.events
            && tx.try_send(e).is_err()
        {
            self.error = Some("手机未及时接收状态，请重新连接".into());
            self.paused = true;
        }
    }
}
