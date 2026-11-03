use crate::controller::Controller;
use anyhow::Result;

#[cfg(any(target_os = "windows", target_os = "macos"))]
pub fn run(runtime: &tokio::runtime::Runtime, controller: Controller, url: String) -> Result<()> {
    use tao::{
        event::{Event, StartCause},
        event_loop::{ControlFlow, EventLoopBuilder},
    };
    use tray_icon::{
        Icon, TrayIcon, TrayIconBuilder,
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
    let mut paused = false;
    let handle = runtime.handle().clone();
    use tao::platform::run_return::EventLoopExtRunReturn;
    event_loop.run_return(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        match event {
            Event::NewEvents(StartCause::Init) => {
                let mut rgba = vec![0u8; 32 * 32 * 4];
                for y in 0..32 {
                    for x in 0..32 {
                        if (x as i32 - 16).pow(2) + (y as i32 - 16).pow(2) < 180 {
                            let i = (y * 32 + x) * 4;
                            rgba[i..i + 4].copy_from_slice(&[35, 75, 198, 255]);
                        }
                    }
                }
                let icon = Icon::from_rgba(rgba, 32, 32).expect("embedded tray icon");
                match TrayIconBuilder::new()
                    .with_menu(Box::new(menu.clone()))
                    .with_tooltip("豆包语音遥控")
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
            Event::UserEvent(UserEvent::Status(s)) => {
                paused = s.paused;
                phone.set_text(if s.connected {
                    "手机：已连接"
                } else {
                    "手机：未连接"
                });
                state.set_text(if s.paused {
                    "状态：已暂停"
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
                        "语音遥控 · 模拟模式"
                    } else {
                        "豆包语音遥控"
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
