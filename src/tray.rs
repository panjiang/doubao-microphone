use crate::controller::Controller;
use anyhow::Result;

#[cfg(target_os = "windows")]
fn windows_tray_pixels() -> u32 {
    use windows::{
        Win32::UI::{
            HiDpi::{GetDpiForSystem, GetDpiForWindow, GetSystemMetricsForDpi},
            WindowsAndMessaging::{FindWindowW, SM_CXSMICON, SM_CYSMICON},
        },
        core::w,
    };
    unsafe {
        let dpi = FindWindowW(w!("Shell_TrayWnd"), None)
            .map(|window| GetDpiForWindow(window))
            .ok()
            .filter(|dpi| *dpi > 0)
            .unwrap_or_else(|| GetDpiForSystem().max(96));
        GetSystemMetricsForDpi(SM_CXSMICON, dpi)
            .max(GetSystemMetricsForDpi(SM_CYSMICON, dpi))
            .max(1) as u32
    }
}

#[cfg(target_os = "windows")]
fn windows_tray_icon(pixels: u32) -> tray_icon::Icon {
    let (data, size): (&[u8], u32) = match pixels {
        16 => (include_bytes!("../assets/icons/tray-16.rgba"), 16),
        20 => (include_bytes!("../assets/icons/tray-20.rgba"), 20),
        24 => (include_bytes!("../assets/icons/tray-24.rgba"), 24),
        28 => (include_bytes!("../assets/icons/tray-28.rgba"), 28),
        32 => (include_bytes!("../assets/icons/tray-32.rgba"), 32),
        40 => (include_bytes!("../assets/icons/tray-40.rgba"), 40),
        48 => (include_bytes!("../assets/icons/tray-48.rgba"), 48),
        64 => (include_bytes!("../assets/icons/tray-64.rgba"), 64),
        _ => (include_bytes!("../assets/icons/tray-128.rgba"), 128),
    };
    // Unusual DPI values use a larger source; the shell still controls display size.
    tray_icon::Icon::from_rgba(data.to_vec(), size, size).expect("embedded tray icon")
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn run(runtime: &tokio::runtime::Runtime, controller: Controller, url: String) -> Result<()> {
    use tao::{
        event::{Event, StartCause},
        event_loop::{ControlFlow, EventLoopBuilder},
    };
    use tray_icon::{
        TrayIcon, TrayIconBuilder,
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    };
    #[derive(Debug)]
    enum UserEvent {
        Menu(MenuEvent),
        Status(crate::controller::Status),
    }
    let mut builder = EventLoopBuilder::<UserEvent>::with_user_event();
    let mut event_loop = builder.build();
    #[cfg(target_os = "macos")]
    {
        use tao::platform::macos::EventLoopExtMacOS;
        event_loop.set_activation_policy(tao::platform::macos::ActivationPolicy::Accessory);
    }
    let proxy = event_loop.create_proxy();
    let menu = Menu::new();
    let phone = MenuItem::new("手机：未连接", false, None);
    let state = MenuItem::new("状态：待命", false, None);
    let open = MenuItem::new("打开配置页", true, None);
    let pause = MenuItem::new("暂停接收", true, None);
    let quit = MenuItem::new("退出", true, None);
    menu.append_items(&[
        &phone,
        &state,
        &PredefinedMenuItem::separator(),
        &open,
        &pause,
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));
    let mut status_rx = controller.status.clone();
    let proxy = event_loop.create_proxy();
    runtime.spawn(async move {
        while status_rx.changed().await.is_ok() {
            let _ = proxy.send_event(UserEvent::Status(status_rx.borrow().clone()));
        }
    });
    let mut tray: Option<TrayIcon> = None;
    #[cfg(target_os = "windows")]
    let mut tray_pixels = 0;
    let mut paused = false;
    let handle = runtime.handle().clone();
    use tao::platform::run_return::EventLoopExtRunReturn;
    event_loop.run_return(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                #[cfg(target_os = "windows")]
                let icon = {
                    tray_pixels = windows_tray_pixels();
                    windows_tray_icon(tray_pixels)
                };
                #[cfg(target_os = "macos")]
                let icon = tray_icon::Icon::from_rgba(
                    include_bytes!("../assets/icons/tray-64.rgba").to_vec(),64,64,
                ).expect("embedded microphone icon");
                match TrayIconBuilder::new()
                    .with_menu(Box::new(menu.clone()))
                    .with_tooltip("豆包远程")
                    .with_icon(icon)
                    .build()
                {
                    Ok(t) => tray = Some(t),
                    Err(e) => {
                        tracing::error!(error=%e,"tray startup failed");
                        *flow = ControlFlow::Exit;
                    }
                }
            }
            #[cfg(target_os = "windows")]
            Event::NewEvents(_) => {
                let pixels = windows_tray_pixels();
                if pixels != tray_pixels && let Some(t) = &tray {
                    match t.set_icon(Some(windows_tray_icon(pixels))) {
                        Ok(()) => tray_pixels = pixels,
                        Err(e) => tracing::warn!(error=%e, "tray DPI update failed"),
                    }
                }
            }
            Event::UserEvent(UserEvent::Status(s)) => {
                paused = s.paused;
                phone.set_text(if s.connected {
                    "手机：已连接"
                } else {
                    "手机：未连接"
                });
                state.set_text(if s.paused {
                    "状态：已手动暂停"
                } else if s.input_state==crate::controller::InputState::Recovering {
                    "状态：输入服务恢复中"
                } else if s.input_state==crate::controller::InputState::Failed {
                    "状态：输入服务需处理，请打开配置页"
                } else if s.error.is_some() {
                    "状态：异常，请打开配置页"
                } else if s.testing {
                    "状态：快捷键测试中"
                } else if s.session_id.is_some() {
                    "状态：输入中"
                } else {
                    "状态：待命"
                });
                pause.set_text(if s.paused {
                    "恢复接收"
                } else {
                    "暂停接收"
                });
                if let Some(t) = &tray {
                    let _ = t.set_tooltip(Some(if s.mock {
                        "豆包远程 · 模拟模式"
                    } else {
                        "豆包远程"
                    }));
                }
            }
            Event::UserEvent(UserEvent::Menu(e)) => {
                if e.id == open.id() {
                    let _ = webbrowser::open(&url);
                } else if e.id == pause.id() {
                    let c = controller.clone();
                    let p = !paused;
                    handle.spawn(async move {
                        c.pause(p).await;
                    });
                } else if e.id == quit.id() {
                    *flow = ControlFlow::Exit;
                }
            }
            Event::LoopDestroyed => {
                tray.take();
            }
            _ => {}
        }
    });
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn run(
    _runtime: &tokio::runtime::Runtime,
    _controller: Controller,
    _url: String,
) -> Result<()> {
    anyhow::bail!("本平台没有原生托盘，请使用 --headless --mock")
}
