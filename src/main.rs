#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]
use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use doubao_microphone::{
    agent, audio, config, controller::Controller, identity, platform, server, tls, tray,
};
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(
    version,
    about = "豆包麦克风 · Doubao Microphone"
)]
struct Args {
    #[arg(long, global = true, help = "开发模拟模式；不产生原生按键和声音")]
    mock: bool,
    #[arg(long, global = true)]
    data_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Option<Command>,
}
#[derive(Subcommand)]
enum Command {
    Serve {
        #[arg(long)]
        headless: bool,
        #[arg(long)]
        open: bool,
    },
    Probe {
        #[command(subcommand)]
        probe: Probe,
    },
    #[command(hide = true)]
    InputAgent,
    #[cfg(target_os = "windows")]
    #[command(hide = true)]
    InstallDefaults,
    #[cfg(target_os = "windows")]
    #[command(hide = true)]
    CheckTrialKeyboard,
}
#[derive(Subcommand)]
enum Probe {
    Devices,
    Key {
        #[arg(long)]
        key: Option<String>,
        #[arg(long, default_value_t = 3)]
        seconds: u64,
    },
    Audio {
        #[arg(long)]
        wav: PathBuf,
        #[arg(long)]
        device: String,
        #[arg(long)]
        hold_key: Option<String>,
    },
}

fn main() -> Result<()> {
    let args = Args::parse();
    if matches!(args.command, Some(Command::InputAgent)) {
        return agent::run(args.mock);
    }
    #[cfg(target_os = "windows")]
    if matches!(args.command, Some(Command::CheckTrialKeyboard)) {
        ensure!(platform::trial_keyboard_available(), "无法访问试用版 UMDF 虚拟键盘");
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    if matches!(args.command, Some(Command::InstallDefaults)) {
        ensure!(!args.mock, "安装配置不支持模拟模式");
        let dir = config::data_dir(args.data_dir)?;
        let mut settings = config::Settings::load(&dir)?;
        settings.resolve_shortcut();
        let previous = settings.autostart;
        if !dir.join("settings.json").exists() { settings.autostart = true; }
        platform::set_autostart(settings.autostart)?;
        if let Err(error) = settings.save(&dir) {
            let _ = platform::set_autostart(previous);
            return Err(error);
        }
        return Ok(());
    }
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "doubao_microphone=info".into()),
        )
        .init();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    match args.command {
        Some(Command::Probe { probe }) => runtime.block_on(probe_run(probe, args.mock)),
        command => {
            let (headless, open) = match command {
                Some(Command::Serve { headless, open }) => (headless, open),
                _ => (false, false),
            };
            ensure!(
                args.mock || cfg!(any(target_os = "windows", target_os = "macos")),
                "native mode supports Windows/macOS only"
            );
            let dir = config::data_dir(args.data_dir)?;
            // Per-user single instance: keep this TCP listener for the process lifetime.
            let mut settings = config::Settings::load(&dir)?;
            let previous_ip = settings.lan_ip;
            let network_changed = !args.mock && settings.resolve_lan_ip(&config::interfaces());
            if cfg!(any(target_os = "windows", target_os = "macos")) && !args.mock {
                settings.resolve_shortcut();
            }
            #[cfg(target_os = "macos")]
            if !args.mock && !dir.join("settings.json").exists() {
                // First launch after installation. Preserve explicit saved preferences on upgrades.
                settings.autostart = std::env::current_exe()?.starts_with("/Applications")
                    || std::env::var_os("HOME").is_some_and(|home| std::env::current_exe().is_ok_and(|exe|exe.starts_with(PathBuf::from(home).join("Applications"))));
                settings.save(&dir)?;
            }
            settings.validate()?;
            let admin_lock =
                std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, settings.admin_port))
                    .context("配置端口已占用；可能已有一个实例运行")?;
            admin_lock.set_nonblocking(true)?;
            let bootstrap = std::net::TcpListener::bind((settings.lan_ip, settings.bootstrap_port))
                .context("证书向导端口不可用")?;
            bootstrap.set_nonblocking(true)?;
            let mobile = std::net::TcpListener::bind((settings.lan_ip, settings.mobile_port))
                .context("手机 HTTPS 端口不可用")?;
            mobile.set_nonblocking(true)?;
            if network_changed {
                settings.save(&dir)?;
                tracing::info!(old_ip=%previous_ip, ip=%settings.lan_ip, interface=?settings.network_interface, "resolved current network address from interface");
            }
            // Retarget startup only after the single-instance lock and ports are held.
            #[cfg(any(target_os = "windows", target_os = "macos"))]
            if !args.mock { platform::set_autostart(settings.autostart)?; }
            let _ = rustls::crypto::ring::default_provider().install_default();
            let material = runtime.block_on(tls::prepare(&dir, settings.lan_ip, args.mock))?;
            let controller =
                runtime.block_on(Controller::spawn(settings.clone(), args.mock, dir.clone()))?;
            let token = identity::secret();
            let url = format!("http://127.0.0.1:{}/#token={token}", settings.admin_port);
            let app = Arc::new(server::App {
                controller: controller.clone(),
                identities: Mutex::new(identity::Identities::load(&dir)?),
                dir: dir.clone(),
                admin_token: token,
                initial: settings,
                mock: args.mock,
                profile: material.profile,
                ca_name: material.ca_name,
            });
            let tls_handle = axum_server::Handle::new();
            let stop_tls = tls_handle.clone();
            let cert = (*material.server).clone();
            let a = app.clone();
            let _admin = runtime.spawn(async move {
                let listener = tokio::net::TcpListener::from_std(admin_lock)?;
                axum::serve(listener, server::admin_router(a))
                    .await
                    .map_err(anyhow::Error::from)
            });
            let a = app.clone();
            let _bootstrap = runtime.spawn(async move {
                let listener = tokio::net::TcpListener::from_std(bootstrap)?;
                axum::serve(listener, server::bootstrap_router(a))
                    .await
                    .map_err(anyhow::Error::from)
            });
            let a = app.clone();
            let mobile_task = runtime.spawn(async move {
                axum_server::from_tcp_rustls(mobile, cert)?
                    .handle(tls_handle)
                    .serve(server::mobile_router(a).into_make_service())
                    .await
            });
            if headless {
                // Deliberately printed only for explicit headless use; never put the token in logs.
                println!("Configuration URL (local access token): {url}");
            }
            config::atomic_write(&dir.join("admin-url.txt"), url.as_bytes())?;
            tracing::info!(mock=args.mock,path=%dir.display(),"bridge started");
            if open {
                let _ = webbrowser::open(&url);
            }
            #[cfg(target_os = "macos")]
            if !args.mock {
                // Open Settings after the configuration page so it remains in front.
                platform::request_accessibility();
            }
            if headless {
                runtime.block_on(async{tokio::select!{_=tokio::signal::ctrl_c()=>{},r=mobile_task=>{tracing::error!(result=?r,"HTTPS listener stopped");}}});
            } else {
                tray::run(&runtime, controller.clone(), url)?;
            }
            runtime.block_on(controller.shutdown());
            stop_tls.graceful_shutdown(Some(Duration::from_secs(1)));
            Ok(())
        }
    }
}

fn parse_key(s: &str) -> Result<platform::Shortcut> {
    serde_json::from_value(serde_json::Value::String(s.into())).context("unknown shortcut")
}
async fn probe_run(probe: Probe, mock: bool) -> Result<()> {
    match probe {
        Probe::Devices => println!(
            "{}",
            serde_json::to_string_pretty(
                &serde_json::json!({"mock":mock,"platform":platform::setup_status(),"outputs":audio::devices(mock)?})
            )?
        ),
        Probe::Key { key, seconds } => {
            ensure!(
                (1..=10).contains(&seconds),
                "probe duration must be 1..10 seconds"
            );
            let key = key
                .as_deref()
                .map(parse_key)
                .transpose()?
                .unwrap_or(platform::locked_shortcut().unwrap_or(platform::Shortcut::LeftAlt));
            ensure!(mock || platform::supported(key), "此快捷键不受当前平台支持");
            let mut a = agent::Agent::spawn(mock).await?;
            eprintln!("3 秒后模拟按住 {key:?}；请将光标放到测试输入框。此操作可能触发豆包听写。");
            tokio::time::sleep(Duration::from_secs(3)).await;
            a.press(key).await?;
            let end = tokio::time::Instant::now() + Duration::from_secs(seconds);
            while tokio::time::Instant::now() < end {
                a.renew().await?;
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            a.release().await?;
            a.shutdown().await;
            eprintln!("快捷键已释放。请人工确认豆包是否被启动和结束；本命令不能判断识别是否成功。");
        }
        Probe::Audio {
            wav,
            device,
            hold_key,
        } => {
            let mut reader = hound::WavReader::open(wav)?;
            let spec = reader.spec();
            ensure!(
                spec.sample_format == hound::SampleFormat::Int && spec.bits_per_sample == 16,
                "probe supports 16-bit PCM WAV only"
            );
            ensure!(
                spec.channels == 1 || spec.channels == 2,
                "mono/stereo WAV required"
            );
            let samples = reader
                .samples::<i16>()
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let mono: Vec<f32> = samples
                .chunks(spec.channels as usize)
                .map(|f| f.iter().map(|s| *s as f32 / 32768.0).sum::<f32>() / spec.channels as f32)
                .collect();
            let out = audio::Audio::new(mock);
            out.open(device, spec.sample_rate).await?;
            let mut a = agent::Agent::spawn(mock).await?;
            eprintln!("3 秒后播放测试 WAV；请先将豆包麦克风设为虚拟设备，并将光标放入输入框。");
            tokio::time::sleep(Duration::from_secs(3)).await;
            if let Some(k) = hold_key {
                a.press(parse_key(&k)?).await?;
                tokio::time::sleep(Duration::from_millis(350)).await;
            }
            out.activate();
            for chunk in mono.chunks((spec.sample_rate / 50) as usize) {
                out.push(chunk.to_vec()).await?;
                a.renew().await?;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            while out.queued_ms() > 0 {
                a.renew().await?;
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
            a.release().await?;
            out.close().await;
            a.shutdown().await;
            eprintln!("播放完成。是否产生正确文字需人工确认。");
        }
    }
    Ok(())
}
