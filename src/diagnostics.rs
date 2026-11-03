//! Explicitly enabled, local-only rolling diagnostic recordings.
//! The real-time input callback only writes to a bounded ring; disk I/O is on
//! this recorder's worker, never on the output callback or session actor.
use anyhow::Result;
use serde::Serialize;
#[cfg(any(target_os = "windows", target_os = "macos"))]
use std::sync::atomic::AtomicBool;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SECONDS: usize = 30;
const KEEP: usize = 3; // Three failures and three recent normal sessions.

struct Rolling {
    rate: u32,
    samples: VecDeque<f32>,
    total: u64,
}
impl Rolling {
    fn new(rate: u32) -> Self {
        Self {
            rate,
            samples: VecDeque::new(),
            total: 0,
        }
    }
    fn push(&mut self, sample: f32) {
        self.total += 1;
        if self.samples.len() == self.rate as usize * SECONDS {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
    }
    fn save(&self, path: &Path) -> Result<()> {
        let mut wav = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels: 1,
                sample_rate: self.rate,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )?;
        for s in &self.samples {
            wav.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16)?;
        }
        wav.finalize()?;
        Ok(())
    }
}

enum Message {
    Phone(Vec<f32>),
    Event(String),
}
#[derive(Serialize)]
struct Level {
    at_ms: u64,
    phone_rms: f64,
    virtual_rms: f64,
}

pub struct Recorder {
    tx: mpsc::SyncSender<Message>,
    finish: Option<mpsc::SyncSender<(bool, String)>>,
    dropped: Arc<AtomicU64>,
}
impl Recorder {
    pub fn start(root: PathBuf, id: Uuid, rate: u32) -> Result<Self> {
        let (tx, rx) = mpsc::sync_channel(32);
        let (finish, end) = mpsc::sync_channel(1);
        let dropped = Arc::new(AtomicU64::new(0));
        let drops = dropped.clone();
        std::thread::Builder::new()
            .name("diagnostic-recorder".into())
            .spawn(move || {
                if let Err(e) = record(root, id, rate, rx, end, drops, NativeInput::open) {
                    tracing::error!(error=%e, "diagnostic save failed");
                }
            })?;
        Ok(Self {
            tx,
            finish: Some(finish),
            dropped,
        })
    }
    pub fn phone(&self, samples: &[f32]) {
        if self.tx.try_send(Message::Phone(samples.to_vec())).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn event(&self, event: impl Into<String>) {
        let _ = self.tx.try_send(Message::Event(event.into()));
    }
    pub fn finish(mut self, complete: bool, reason: &str) {
        if let Some(tx) = self.finish.take() {
            let _ = tx.try_send((complete, reason.into()));
        }
    }
}
impl Drop for Recorder {
    fn drop(&mut self) {
        if let Some(tx) = self.finish.take() {
            let _ = tx.try_send((false, "recorder_dropped".into()));
        }
    }
}

fn record(
    root: PathBuf,
    id: Uuid,
    rate: u32,
    rx: mpsc::Receiver<Message>,
    end: mpsc::Receiver<(bool, String)>,
    drops: Arc<AtomicU64>,
    open_input: impl FnOnce() -> Result<NativeInput>,
) -> Result<()> {
    let started = Instant::now();
    let mut phone = Rolling::new(rate);
    let mut events = VecDeque::new();
    let mut levels = VecDeque::new();
    let mut input = open_input();
    let input_error = input.as_ref().err().map(ToString::to_string);
    let mut cable = Rolling::new(input.as_ref().map(|i| i.rate).unwrap_or(48000));
    let capture_started_ms = started.elapsed().as_millis() as u64;
    let mut meter = Instant::now();
    let (mut p_energy, mut p_count, mut c_energy, mut c_count) = (0.0, 0u64, 0.0, 0u64);
    let (complete, reason) = loop {
        let ending = match end.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Disconnected) => Some((false, "recorder_disconnected".into())),
            Err(_) => None,
        };
        for message in rx.try_iter() {
            match message {
                Message::Phone(samples) => {
                    for s in samples {
                        p_energy += (s as f64).powi(2);
                        p_count += 1;
                        phone.push(s);
                    }
                }
                Message::Event(event) => {
                    if events.len() == 128 {
                        events.pop_front();
                    }
                    events.push_back(serde_json::json!({"at_ms":started.elapsed().as_millis() as u64,"event":event}));
                }
            }
        }
        if let Ok(input) = &mut input {
            input.drain(|s| {
                c_energy += (s as f64).powi(2);
                c_count += 1;
                cable.push(s);
            });
        }
        if meter.elapsed() >= Duration::from_millis(100) || ending.is_some() {
            if levels.len() == SECONDS * 10 {
                levels.pop_front();
            }
            levels.push_back(Level {
                at_ms: started.elapsed().as_millis() as u64,
                phone_rms: (p_energy / p_count.max(1) as f64).sqrt(),
                virtual_rms: (c_energy / c_count.max(1) as f64).sqrt(),
            });
            (p_energy, p_count, c_energy, c_count) = (0.0, 0, 0.0, 0);
            meter = Instant::now();
        }
        if let Some(ending) = ending {
            break ending;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let input_dropped = input.as_ref().map(|i| i.dropped()).unwrap_or(0);
    let input_failed = input.as_ref().is_ok_and(|i| i.failed());
    drop(input); // Stop recording before any disk work.
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let category = if complete { "recent" } else { "fault" };
    std::fs::create_dir_all(&root)?;
    let dir = root.join(format!("{category}-{stamp:020}-{id}"));
    std::fs::create_dir(&dir)?;
    phone.save(&dir.join("phone.wav"))?;
    if input_error.is_none() {
        cable.save(&dir.join("virtual-microphone.wav"))?;
    }
    crate::config::atomic_write(
        &dir.join("report.json"),
        &serde_json::to_vec_pretty(&serde_json::json!({
            "version":env!("CARGO_PKG_VERSION"),"session_id":id,"complete":complete,"reason":reason,
            "duration_ms":started.elapsed().as_millis() as u64,"capture_started_ms":capture_started_ms,
            "phone_rate":phone.rate,"phone_total_samples":phone.total,
            "virtual_rate":cable.rate,"virtual_total_samples":cable.total,
            "phone_messages_dropped":drops.load(Ordering::Relaxed),
            "virtual_samples_dropped":input_dropped,"virtual_capture_failed":input_failed,
            "virtual_capture_error":input_error,"retained_seconds":SECONDS,
            "events":events,"levels":levels
        }))?,
    )?;
    prune(&root, category)?;
    tracing::info!(session=%id,reason,path=%dir.display(),"local diagnostic saved");
    Ok(())
}

fn prune(root: &Path, category: &str) -> Result<()> {
    let prefix = format!("{category}-");
    let mut dirs = std::fs::read_dir(root)?
        .filter_map(|d| d.ok())
        .filter(|d| {
            d.file_type().is_ok_and(|t| t.is_dir())
                && d.file_name().to_string_lossy().starts_with(&prefix)
        })
        .map(|d| d.path())
        .collect::<Vec<_>>();
    dirs.sort();
    for dir in dirs.iter().take(dirs.len().saturating_sub(KEEP)) {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct NativeInput {
    _stream: cpal::Stream,
    consumer: ringbuf::HeapCons<f32>,
    rate: u32,
    drops: Arc<AtomicU64>,
    error: Arc<AtomicBool>,
}
#[cfg(any(target_os = "windows", target_os = "macos"))]
impl NativeInput {
    fn open() -> Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use ringbuf::traits::{Producer, Split};
        let mut devices = cpal::default_host()
            .input_devices()?
            .filter(|d| {
                d.description().is_ok_and(|n| {
                    n.name().contains(if cfg!(target_os = "windows") {
                        "CABLE Output"
                    } else {
                        "BlackHole 2ch"
                    })
                })
            })
            .collect::<Vec<_>>();
        anyhow::ensure!(
            devices.len() == 1,
            "无法唯一确定虚拟麦克风，未录制其他麦克风"
        );
        let device = devices.remove(0);
        let config = device.default_input_config()?;
        let rate = config.sample_rate();
        let channels = config.channels() as usize;
        let (mut producer, consumer) = ringbuf::HeapRb::<f32>::new(rate as usize * 2).split();
        let drops = Arc::new(AtomicU64::new(0));
        let callback_drops = drops.clone();
        let error = Arc::new(AtomicBool::new(false));
        let callback_error = error.clone();
        let mut receive = move |s: f32| {
            if producer.try_push(s).is_err() {
                callback_drops.fetch_add(1, Ordering::Relaxed);
            }
        };
        let on_error = move |_: cpal::Error| {
            callback_error.store(true, Ordering::Relaxed);
        };
        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device.build_input_stream(
                config.into(),
                move |data: &[f32], _| {
                    for frame in data.chunks_exact(channels) {
                        receive(frame.iter().sum::<f32>() / channels as f32);
                    }
                },
                on_error,
                None,
            )?,
            cpal::SampleFormat::I16 => device.build_input_stream(
                config.into(),
                move |data: &[i16], _| {
                    for frame in data.chunks_exact(channels) {
                        receive(
                            frame.iter().map(|s| *s as f32 / 32768.0).sum::<f32>()
                                / channels as f32,
                        );
                    }
                },
                on_error,
                None,
            )?,
            _ => anyhow::bail!("虚拟麦克风诊断仅支持 float32/int16"),
        };
        stream.play()?;
        Ok(Self {
            _stream: stream,
            consumer,
            rate,
            drops,
            error,
        })
    }
    fn drain(&mut self, mut f: impl FnMut(f32)) {
        use ringbuf::traits::Consumer;
        while let Some(s) = self.consumer.try_pop() {
            f(s);
        }
    }
    fn dropped(&self) -> u64 {
        self.drops.load(Ordering::Relaxed)
    }
    fn failed(&self) -> bool {
        self.error.load(Ordering::Relaxed)
    }
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
struct NativeInput {
    rate: u32,
}
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl NativeInput {
    fn open() -> Result<Self> {
        anyhow::bail!("当前平台无虚拟麦克风诊断")
    }
    fn drain(&mut self, _: impl FnMut(f32)) {}
    fn dropped(&self) -> u64 {
        0
    }
    fn failed(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rolling_audio_keeps_only_latest_samples() {
        let mut audio = Rolling::new(2);
        for n in 0..100 {
            audio.push(n as f32);
        }
        assert_eq!(audio.total, 100);
        assert_eq!(audio.samples.len(), 60);
        assert_eq!(audio.samples.front(), Some(&40.0));
    }
    #[test]
    fn rotation_preserves_failures_while_normal_sessions_rotate() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "fault-0001",
            "recent-0001",
            "recent-0002",
            "recent-0003",
            "recent-0004",
        ] {
            std::fs::create_dir(root.path().join(name)).unwrap();
        }
        prune(root.path(), "recent").unwrap();
        assert!(root.path().join("fault-0001").exists());
        assert!(!root.path().join("recent-0001").exists());
        assert!(root.path().join("recent-0004").exists());
    }
    #[test]
    fn saves_phone_audio_and_failure_reason_even_when_virtual_capture_fails() {
        let root = tempfile::tempdir().unwrap();
        let (tx, rx) = mpsc::sync_channel(8);
        let (finish, end) = mpsc::sync_channel(1);
        tx.send(Message::Phone(vec![0.0, 0.25, -0.25])).unwrap();
        tx.send(Message::Event(
            "doubao recording=false panel_visible=false".into(),
        ))
        .unwrap();
        finish.send((false, "doubao_stopped".into())).unwrap();
        record(
            root.path().to_owned(),
            Uuid::new_v4(),
            48000,
            rx,
            end,
            Arc::new(AtomicU64::new(0)),
            || anyhow::bail!("fake capture unavailable"),
        )
        .unwrap();
        let dir = std::fs::read_dir(root.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("report.json")).unwrap()).unwrap();
        assert_eq!(report["reason"], "doubao_stopped");
        assert_eq!(report["phone_total_samples"], 3);
        assert_eq!(
            report["events"][0]["event"],
            "doubao recording=false panel_visible=false"
        );
        assert_eq!(report["virtual_capture_error"], "fake capture unavailable");
        let wav = hound::WavReader::open(dir.join("phone.wav")).unwrap();
        assert_eq!(wav.duration(), 3);
        assert!(!dir.join("virtual-microphone.wav").exists());
    }
}
