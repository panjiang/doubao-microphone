use anyhow::{Context, Result, ensure};
use serde::Serialize;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

const MAX_QUEUED_MS: u64 = 750;
const COMMAND_CAPACITY: usize = 40;
const SEND_WAIT: Duration = Duration::from_millis(100);

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
    Flush(oneshot::Sender<Result<()>>),
    Clear(oneshot::Sender<()>),
    Close(oneshot::Sender<()>),
}

#[derive(Default)]
pub struct Metrics {
    pub queued_samples: AtomicU64,
    pub pending_input_samples: AtomicU64,
    pub input_rate: AtomicU64,
    pub output_rate: AtomicU64,
    pub gate: AtomicBool,
    pub failed: AtomicBool,
}
#[derive(Clone)]
pub struct Audio {
    tx: mpsc::Sender<Command>,
    pub metrics: Arc<Metrics>,
}
impl Audio {
    pub fn new(mock: bool) -> Self {
        let (tx, rx) = mpsc::channel(COMMAND_CAPACITY);
        let metrics = Arc::new(Metrics::default());
        let m = metrics.clone();
        std::thread::Builder::new()
            .name("audio-output".into())
            .spawn(move || worker(rx, m, mock))
            .expect("audio worker");
        Self { tx, metrics }
    }
    async fn send(&self, command: Command, operation: &str) -> Result<()> {
        tokio::time::timeout(SEND_WAIT, self.tx.send(command))
            .await
            .with_context(|| format!("音频队列处理超时（{operation}），本次已停止"))?
            .with_context(|| format!("音频工作线程已退出（{operation}）"))?;
        Ok(())
    }
    pub async fn open(&self, device: String, rate: u32) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.send(
            Command::Open {
                device,
                rate,
                reply,
            },
            "打开设备",
        )
        .await?;
        tokio::time::timeout(Duration::from_secs(3), rx)
            .await
            .context("打开音频设备超时")?
            .context("音频工作线程未返回打开结果")??;
        Ok(())
    }
    pub async fn push(&self, samples: Vec<f32>) -> Result<()> {
        ensure!(!self.failed(), "音频设备已失效");
        let rate = self.metrics.input_rate.load(Ordering::Acquire);
        ensure!(rate > 0, "音频设备尚未连接");
        let count = samples.len() as u64;
        let incoming_ms = (count * 1000).div_ceil(rate);
        ensure!(
            self.queued_ms().saturating_add(incoming_ms) <= MAX_QUEUED_MS,
            "音频积压超过 750ms，本次已停止；请检查网络或音频设备"
        );
        self.metrics
            .pending_input_samples
            .fetch_add(count, Ordering::AcqRel);
        if let Err(error) = self.send(Command::Samples(samples), "传送音频").await {
            self.metrics
                .pending_input_samples
                .fetch_sub(count, Ordering::AcqRel);
            return Err(error);
        }
        Ok(())
    }
    pub fn activate(&self) {
        self.metrics.gate.store(true, Ordering::Release);
    }
    pub async fn flush(&self) -> Result<()> {
        let (reply, rx) = oneshot::channel();
        self.send(Command::Flush(reply), "刷新尾音").await?;
        tokio::time::timeout(Duration::from_millis(300), rx)
            .await
            .context("刷新音频尾部超时")?
            .context("音频工作线程未确认尾部刷新")??;
        Ok(())
    }
    pub async fn clear(&self) -> Result<()> {
        self.metrics.gate.store(false, Ordering::Release);
        let (reply, rx) = oneshot::channel();
        let result = async {
            self.send(Command::Clear(reply), "清空音频").await?;
            tokio::time::timeout(Duration::from_millis(300), rx)
                .await
                .context("清空音频超时")?
                .context("音频工作线程未确认清空")?;
            ensure!(!self.failed(), "音频设备未能完成清空");
            Ok(())
        }
        .await;
        if result.is_err() {
            self.metrics.failed.store(true, Ordering::Release);
        }
        result
    }
    pub async fn close(&self) {
        self.metrics.gate.store(false, Ordering::Release);
        let (reply, rx) = oneshot::channel();
        let result: Result<()> = async {
            self.send(Command::Close(reply), "关闭设备").await?;
            tokio::time::timeout(Duration::from_secs(1), rx)
                .await
                .context("关闭音频设备超时")?
                .context("音频工作线程未确认关闭")?;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            self.metrics.failed.store(true, Ordering::Release);
            tracing::error!(error=%error,"audio close failed");
        }
    }
    pub fn queued_ms(&self) -> u64 {
        let rate = self.metrics.output_rate.load(Ordering::Acquire);
        let input_rate = self.metrics.input_rate.load(Ordering::Acquire);
        let output = if rate == 0 {
            0
        } else {
            self.metrics.queued_samples.load(Ordering::Acquire) * 1000 / rate
        };
        let input = if input_rate == 0 {
            0
        } else {
            self.metrics.pending_input_samples.load(Ordering::Acquire) * 1000 / input_rate
        };
        output.saturating_add(input)
    }
    pub fn failed(&self) -> bool {
        self.metrics.failed.load(Ordering::Acquire)
    }
}

fn worker(mut rx: mpsc::Receiver<Command>, m: Arc<Metrics>, mock: bool) {
    #[cfg(any(target_os = "windows", target_os = "macos"))]
    let mut native: Option<Native> = None;
    let mut mock_rate = 48000u32;
    let mut mock_tick = std::time::Instant::now();
    loop {
        let command = if mock {
            match rx.try_recv() {
                Ok(command) => Some(command),
                Err(mpsc::error::TryRecvError::Empty) => {
                    std::thread::sleep(Duration::from_millis(10));
                    Some(Command::Samples(Vec::new()))
                }
                Err(mpsc::error::TryRecvError::Disconnected) => None,
            }
        } else {
            rx.blocking_recv()
        };
        let Some(command) = command else {
            break;
        };
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
                m.input_rate.store(rate as u64, Ordering::Release);
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
                let input_count = samples.len() as u64;
                if mock {
                    m.queued_samples
                        .fetch_add(samples.len() as u64, Ordering::AcqRel);
                } else {
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    if !samples.is_empty() {
                        if let Some(n) = native.as_mut() {
                            if let Err(error) = n.push(samples) {
                                tracing::warn!(error=%error,"audio output rejected samples");
                                m.failed.store(true, Ordering::Release);
                            }
                        } else {
                            m.failed.store(true, Ordering::Release);
                        }
                    }
                }
                m.pending_input_samples
                    .fetch_sub(input_count, Ordering::AcqRel);
            }
            Command::Clear(reply) => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                if let Some(n) = native.as_mut() {
                    n.clear();
                }
                if mock {
                    m.queued_samples.store(0, Ordering::Release);
                }
                let _ = reply.send(());
            }
            Command::Flush(reply) => {
                let result: Result<()> = {
                    #[cfg(any(target_os = "windows", target_os = "macos"))]
                    {
                        if !mock {
                            native
                                .as_mut()
                                .context("音频设备未打开")
                                .and_then(|n| n.flush())
                        } else {
                            Ok(())
                        }
                    }
                    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
                    {
                        Ok(())
                    }
                };
                if let Err(error) = &result {
                    tracing::warn!(error=%error,"audio tail flush failed");
                    m.failed.store(true, Ordering::Release);
                }
                let _ = reply.send(result);
            }
            Command::Close(reply) => {
                #[cfg(any(target_os = "windows", target_os = "macos"))]
                {
                    native = None;
                }
                m.queued_samples.store(0, Ordering::Release);
                m.input_rate.store(0, Ordering::Release);
                let _ = reply.send(());
            }
        }
        if mock {
            let now = std::time::Instant::now();
            if m.gate.load(Ordering::Acquire) {
                let consumed = (now.duration_since(mock_tick).as_nanos() * mock_rate as u128
                    / 1_000_000_000) as u64;
                let _ = m
                    .queued_samples
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                        Some(n.saturating_sub(consumed))
                    });
            }
            mock_tick = now;
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
