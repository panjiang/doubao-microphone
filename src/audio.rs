use anyhow::{Result, ensure};
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tokio::sync::oneshot;

#[derive(Clone, Debug, Serialize)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub virtual_candidate: bool,
}

pub fn devices(mock: bool) -> Result<Vec<Device>> {
    if mock {
        return Ok(vec![Device {
            id: "mock".into(),
            name: "Mock output (does not feed a microphone)".into(),
            virtual_candidate: true,
        }]);
    }
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    {
        use cpal::traits::{DeviceTrait, HostTrait};
        let host = cpal::default_host();
        let mut out = Vec::new();
        for d in host.output_devices()? {
            let name = d.description()?.name().to_string();
            out.push(Device {
                id: d.id()?.to_string(),
                virtual_candidate: name.contains("CABLE Input") || name.contains("BlackHole"),
                name,
            });
        }
        return Ok(out);
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    anyhow::bail!("音频原生端仅支持 Windows/macOS；开发测试请使用 --mock")
}

enum Command {
    Open {
        device: String,
        rate: u32,
        reply: oneshot::Sender<Result<()>>,
    },
    Samples(Vec<f32>),
    Flush,
    Clear,
    Close,
}

#[derive(Default)]
pub struct Metrics {
    pub queued_samples: AtomicU64,
    pub output_rate: AtomicU64,
    pub gate: AtomicBool,
    pub failed: AtomicBool,
}
#[derive(Clone)]
pub struct Audio {
    tx: mpsc::SyncSender<Command>,
    pub metrics: Arc<Metrics>,
}
impl Audio {
    pub fn new(mock: bool) -> Self {
        let (tx, rx) = mpsc::sync_channel(8);
        let metrics = Arc::new(Metrics::default());
        let m = metrics.clone();
        std::thread::Builder::new()
            .name("audio-output".into())
            .spawn(move || worker(rx, m, mock))
            .expect("audio worker");
        Self { tx, metrics }
    }
    pub async fn open(&self, device: String, rate: u32) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.tx.try_send(Command::Open {
            device,
            rate,
            reply,
        })?;
        tokio::time::timeout(Duration::from_secs(3), rx).await???;
        Ok(())
    }
    pub fn push(&self, samples: Vec<f32>) -> Result<()> {
        ensure!(
            !self.metrics.failed.load(Ordering::Acquire),
            "音频设备已失效"
        );
        self.tx.try_send(Command::Samples(samples))?;
        Ok(())
    }
    pub fn activate(&self) {
        self.metrics.gate.store(true, Ordering::Release);
    }
    pub fn flush(&self) -> Result<()> {
        self.tx.try_send(Command::Flush)?;
        Ok(())
    }
    pub fn clear(&self) {
        self.metrics.gate.store(false, Ordering::Release);
        if self.tx.try_send(Command::Clear).is_err() {
            self.metrics.failed.store(true, Ordering::Release);
        }
    }
    pub fn close(&self) {
        self.metrics.gate.store(false, Ordering::Release);
        let _ = self.tx.try_send(Command::Close);
    }
    pub fn queued_ms(&self) -> u64 {
        let rate = self.metrics.output_rate.load(Ordering::Acquire);
        if rate == 0 {
            0
        } else {
            self.metrics.queued_samples.load(Ordering::Acquire) * 1000 / rate
        }
    }
    pub fn failed(&self) -> bool {
        self.metrics.failed.load(Ordering::Acquire)
    }
}

fn worker(rx: mpsc::Receiver<Command>, m: Arc<Metrics>, mock: bool) {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let mut native: Option<Native> = None;
    let mut mock_rate = 48000u32;
    while let Ok(command) = rx
        .recv_timeout(Duration::from_millis(10))
        .or_else(|e| match e {
            mpsc::RecvTimeoutError::Timeout => Ok(Command::Samples(Vec::new())),
            mpsc::RecvTimeoutError::Disconnected => Err(e),
        })
    {
        match command {
            Command::Open {
                device,
                rate,
                reply,
            } => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                {
                    native = None;
                }
                m.gate.store(false, Ordering::Release);
                m.failed.store(false, Ordering::Release);
                m.queued_samples.store(0, Ordering::Release);
                let result = if mock {
                    mock_rate = rate;
                    m.output_rate.store(rate as u64, Ordering::Release);
                    Ok(())
                } else {
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    {
                        Native::open(&device, rate, m.clone()).map(|n| native = Some(n))
                    }
                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    {
                        let _ = device;
                        Err(anyhow::anyhow!("native audio unavailable"))
                    }
                };
                if result.is_err() {
                    m.failed.store(true, Ordering::Release);
                }
                let _ = reply.send(result);
            }
            Command::Samples(samples) => {
                if mock {
                    m.queued_samples
                        .fetch_add(samples.len() as u64, Ordering::AcqRel);
                } else {
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    if !samples.is_empty() {
                        if let Some(n) = native.as_mut() {
                            if n.push(samples).is_err() {
                                m.failed.store(true, Ordering::Release);
                            }
                        } else {
                            m.failed.store(true, Ordering::Release);
                        }
                    }
                }
            }
            Command::Clear => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                if let Some(n) = native.as_mut() {
                    n.clear();
                }
                if mock {
                    m.queued_samples.store(0, Ordering::Release);
                }
            }
            Command::Flush => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                if !mock
                    && let Some(n) = native.as_mut()
                    && n.flush().is_err()
                {
                    m.failed.store(true, Ordering::Release);
                }
            }
            Command::Close => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                {
                    native = None;
                }
                m.queued_samples.store(0, Ordering::Release);
            }
        }
        if mock && m.gate.load(Ordering::Acquire) {
            // Mock clock only; no physical audio is produced.
            let _ = m
                .queued_samples
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                    Some(n.saturating_sub((mock_rate / 100) as u64))
                });
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
struct Native {
    _stream: cpal::Stream,
    producer: ringbuf::HeapProd<f32>,
    resampler: Option<rubato::SincFixedIn<f32>>,
    metrics: Arc<Metrics>,
    reset: Arc<AtomicBool>,
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Native {
    fn open(wanted: &str, input_rate: u32, m: Arc<Metrics>) -> Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use ringbuf::traits::{Consumer, Split};
        let host = cpal::default_host();
        let mut candidates = Vec::new();
        for d in host.output_devices()? {
            let name = d.description()?.name().to_string();
            if d.id()?.to_string() == wanted
                || name == wanted
                || name.starts_with(&format!("{wanted} ("))
            {
                candidates.push(d);
            }
        }
        ensure!(
            candidates.len() == 1,
            "找不到唯一音频输出设备，请在配置页选择：{wanted}"
        );
        let device = candidates.remove(0);
        let cfg = device.default_output_config()?;
        let rate = cfg.sample_rate();
        let channels = cfg.channels() as usize;
        let rb = ringbuf::HeapRb::<f32>::new(rate as usize);
        let (producer, mut consumer) = rb.split();
        m.output_rate.store(rate as u64, Ordering::Release);
        let reset = Arc::new(AtomicBool::new(false));
        let callback_metrics = m.clone();
        let callback_reset = reset.clone();
        let mut fill = move |data: &mut [f32]| {
            if callback_reset.load(Ordering::Acquire) {
                while consumer.try_pop().is_some() {
                    callback_metrics
                        .queued_samples
                        .fetch_sub(1, Ordering::AcqRel);
                }
                callback_reset.store(false, Ordering::Release);
            }
            for chunk in data.chunks_mut(channels) {
                let mono = if callback_metrics.gate.load(Ordering::Acquire) {
                    if let Some(s) = consumer.try_pop() {
                        callback_metrics
                            .queued_samples
                            .fetch_sub(1, Ordering::AcqRel);
                        s
                    } else {
                        0.0
                    }
                } else {
                    0.0
                };
                chunk.fill(mono);
            }
        };
        let err_m = m.clone();
        let error = move |_: cpal::Error| {
            err_m.failed.store(true, Ordering::Release);
        };
        // Fixed-size callback scratch storage; never allocate in the device callback.
        let config: cpal::StreamConfig = cfg.clone().into();
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::F32 => {
                device.build_output_stream(config, move |d: &mut [f32], _| fill(d), error, None)?
            }
            cpal::SampleFormat::I16 => {
                let mut scratch = vec![0f32; 16384];
                device.build_output_stream(
                    config,
                    move |d: &mut [i16], _| {
                        for chunk in d.chunks_mut(16384 / channels * channels) {
                            fill(&mut scratch[..chunk.len()]);
                            for (o, s) in chunk.iter_mut().zip(&scratch) {
                                *o = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
                            }
                        }
                    },
                    error,
                    None,
                )?
            }
            _ => anyhow::bail!("输出设备不支持 float32/int16，请调整虚拟音频设备格式"),
        };
        stream.play()?;
        let resampler = if input_rate != rate {
            Some(rubato::SincFixedIn::<f32>::new(
                rate as f64 / input_rate as f64,
                1.0,
                rubato::SincInterpolationParameters {
                    sinc_len: 128,
                    f_cutoff: 0.95,
                    interpolation: rubato::SincInterpolationType::Linear,
                    oversampling_factor: 128,
                    window: rubato::WindowFunction::BlackmanHarris2,
                },
                (input_rate / 50) as usize,
                1,
            )?)
        } else {
            None
        };
        Ok(Self {
            _stream: stream,
            producer,
            resampler,
            metrics: m,
            reset,
        })
    }
    fn push(&mut self, samples: Vec<f32>) -> Result<()> {
        use ringbuf::traits::Producer;
        use rubato::Resampler;
        let out = if let Some(r) = &mut self.resampler {
            r.process_partial(Some(&[samples]), None)?.remove(0)
        } else {
            samples
        };
        for sample in out {
            self.metrics.queued_samples.fetch_add(1, Ordering::AcqRel);
            if self.producer.try_push(sample).is_err() {
                self.metrics.queued_samples.fetch_sub(1, Ordering::AcqRel);
                anyhow::bail!("audio ring buffer full");
            }
        }
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        use ringbuf::traits::Producer;
        use rubato::Resampler;
        if let Some(r) = &mut self.resampler {
            for sample in r.process_partial::<Vec<f32>>(None, None)?.remove(0) {
                self.metrics.queued_samples.fetch_add(1, Ordering::AcqRel);
                if self.producer.try_push(sample).is_err() {
                    self.metrics.queued_samples.fetch_sub(1, Ordering::AcqRel);
                    anyhow::bail!("audio tail buffer full");
                }
            }
        }
        Ok(())
    }
    fn clear(&mut self) {
        // Only the non-realtime worker waits. Samples from a later session may
        // not be enqueued until the callback has discarded the old session.
        self.reset.store(true, Ordering::Release);
        let deadline = std::time::Instant::now() + Duration::from_millis(100);
        while self.reset.load(Ordering::Acquire) {
            if std::time::Instant::now() >= deadline {
                self.metrics.failed.store(true, Ordering::Release);
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(r) = self.resampler.as_mut() {
            use rubato::Resampler;
            r.reset();
        }
    }
}
