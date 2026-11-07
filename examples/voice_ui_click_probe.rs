//! P0 experiment: use the existing microphone button through ordinary window
//! mouse messages, with a dedicated empty input window and a fixed virtual mic.
#[cfg(target_os = "windows")]
mod native {
    use anyhow::{Context, Result, ensure};
    use std::time::{Duration, Instant};
    use windows::{
        Win32::{
            Foundation::{HWND, LPARAM, RECT, WPARAM},
            System::Threading::{AttachThreadInput, GetCurrentThreadId},
            UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
        },
        core::{BOOL, w},
    };

    #[derive(Clone, Debug)]
    struct Window {
        hwnd: HWND,
        class: String,
        rect: RECT,
        visible: bool,
    }
    struct Enumeration {
        pid: u32,
        windows: Vec<Window>,
    }
    struct DpiScope(DPI_AWARENESS_CONTEXT);
    impl Drop for DpiScope {
        fn drop(&mut self) {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
    unsafe extern "system" fn enumerate(hwnd: HWND, data: LPARAM) -> BOOL {
        let data = unsafe { &mut *(data.0 as *mut Enumeration) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        if pid == data.pid {
            let mut name = [0u16; 128];
            let len = unsafe { GetClassNameW(hwnd, &mut name) };
            let mut rect = RECT::default();
            let _ = unsafe { GetWindowRect(hwnd, &mut rect) };
            data.windows.push(Window {
                hwnd,
                class: String::from_utf16_lossy(&name[..len.max(0) as usize]),
                rect,
                visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
            });
        }
        true.into()
    }
    fn snapshot(pid: u32) -> Result<Vec<Window>> {
        let mut data = Enumeration {
            pid,
            windows: Vec::new(),
        };
        unsafe { EnumWindows(Some(enumerate), LPARAM(&mut data as *mut _ as isize))? };
        Ok(data.windows)
    }
    fn pump(ms: u64) {
        let end = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < end {
            unsafe {
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn capture(window: &Window, path: &str) -> Result<()> {
        use windows::Win32::Graphics::Gdi::*;
        #[link(name = "user32")]
        unsafe extern "system" {
            fn PrintWindow(window: HWND, dc: HDC, flags: u32) -> BOOL;
        }
        let _dpi = DpiScope(unsafe {
            SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)
        });
        let mut rect = RECT::default();
        unsafe { GetWindowRect(window.hwnd, &mut rect)? };
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;
        ensure!(width > 0 && height > 0 && width <= 800 && height <= 200);
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let old = SelectObject(memory, HGDIOBJ(bitmap.0));
            let result = (|| -> Result<()> {
                if !PrintWindow(window.hwnd, memory, 2).as_bool() {
                    BitBlt(
                        memory,
                        0,
                        0,
                        width,
                        height,
                        Some(screen),
                        rect.left,
                        rect.top,
                        SRCCOPY | CAPTUREBLT,
                    )?;
                }
                let _ = SelectObject(memory, old);
                let mut info = BITMAPINFO::default();
                info.bmiHeader.biSize = 40;
                info.bmiHeader.biWidth = width;
                info.bmiHeader.biHeight = -height;
                info.bmiHeader.biPlanes = 1;
                info.bmiHeader.biBitCount = 32;
                let mut pixels = vec![0u8; (width * height * 4) as usize];
                ensure!(
                    GetDIBits(
                        memory,
                        bitmap,
                        0,
                        height as u32,
                        Some(pixels.as_mut_ptr().cast()),
                        &mut info,
                        DIB_RGB_COLORS,
                    ) == height,
                    "could not capture test voice panel"
                );
                let mut bytes = Vec::new();
                bytes.extend_from_slice(b"BM");
                bytes.extend_from_slice(&(54 + pixels.len() as u32).to_le_bytes());
                bytes.extend_from_slice(&[0u8; 4]);
                bytes.extend_from_slice(&54u32.to_le_bytes());
                bytes.extend_from_slice(&40u32.to_le_bytes());
                bytes.extend_from_slice(&width.to_le_bytes());
                bytes.extend_from_slice(&(-height).to_le_bytes());
                bytes.extend_from_slice(&1u16.to_le_bytes());
                bytes.extend_from_slice(&32u16.to_le_bytes());
                bytes.extend_from_slice(&[0u8; 24]);
                bytes.extend_from_slice(&pixels);
                std::fs::write(path, bytes)?;
                Ok(())
            })();
            let _ = SelectObject(memory, old);
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            result
        }
    }
    struct InputWindow {
        host: HWND,
        previous: HWND,
    }
    impl Drop for InputWindow {
        fn drop(&mut self) {
            unsafe {
                let restore = GetForegroundWindow() == self.host;
                let _ = DestroyWindow(self.host);
                if restore && IsWindow(Some(self.previous)).as_bool() {
                    let _ = SetForegroundWindow(self.previous);
                }
            }
        }
    }
    fn virtual_microphone_selected() -> Result<()> {
        use cpal::traits::{DeviceTrait, HostTrait};
        let path =
            std::path::PathBuf::from(std::env::var_os("APPDATA").context("APPDATA missing")?)
                .join("DoubaoIME/conf/config.json");
        let config: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
        let selected = config["voice"]["selectedMicrophoneId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("select CABLE Output explicitly in Doubao before this probe")?;
        for input in cpal::default_host().input_devices()? {
            if input.description()?.name().contains("CABLE Output")
                && input.id()?.to_string().ends_with(selected)
            {
                println!("virtual_microphone_confirmed=CABLE Output");
                return Ok(());
            }
        }
        anyhow::bail!("Doubao is not configured to use CABLE Output")
    }
    fn play_fixture(path: String) -> std::thread::JoinHandle<Result<()>> {
        std::thread::spawn(move || {
            let mut wav = hound::WavReader::open(path)?;
            let spec = wav.spec();
            ensure!(spec.channels == 1 && spec.bits_per_sample == 16);
            let samples: Vec<f32> = wav
                .samples::<i16>()
                .map(|s| s.map(|s| s as f32 / 32768.0))
                .collect::<std::result::Result<_, _>>()?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async {
                let audio = doubao_microphone::audio::Audio::new(false);
                audio.open("CABLE Input".into(), spec.sample_rate).await?;
                audio.activate();
                for frame in samples.chunks((spec.sample_rate / 50) as usize) {
                    audio.push(frame.to_vec()).await?;
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                audio.flush().await?;
                while audio.queued_ms() > 0 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
                audio.close().await;
                Ok(())
            })
        })
    }
    fn capture_active(pid: u32) -> Result<bool> {
        std::thread::spawn(move || -> Result<bool> {
            use windows::{
                Win32::{Media::Audio::*, System::Com::*},
                core::{Interface, PCWSTR},
            };
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
                let result = (|| -> Result<bool> {
                    let path = std::path::PathBuf::from(std::env::var_os("APPDATA").unwrap())
                        .join("DoubaoIME/conf/config.json");
                    let config: serde_json::Value = serde_json::from_slice(&std::fs::read(path)?)?;
                    let id = config["voice"]["selectedMicrophoneId"].as_str().unwrap();
                    let id: Vec<u16> = id.encode_utf16().chain(Some(0)).collect();
                    let enumerator: IMMDeviceEnumerator =
                        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
                    let device = enumerator.GetDevice(PCWSTR(id.as_ptr()))?;
                    let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
                    let sessions = manager.GetSessionEnumerator()?;
                    for i in 0..sessions.GetCount()? {
                        let session = sessions.GetSession(i)?;
                        let details: IAudioSessionControl2 = session.cast()?;
                        if details.GetProcessId()? == pid
                            && session.GetState()? == AudioSessionStateActive
                        {
                            return Ok(true);
                        }
                    }
                    Ok(false)
                })();
                CoUninitialize();
                result
            }
        })
        .join()
        .map_err(|_| anyhow::anyhow!("capture session query failed"))?
    }
    pub fn run() -> Result<()> {
        unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE) };
        let pid: u32 = std::env::args()
            .nth(1)
            .context("usage: voice_ui_click_probe <ImeService process ID>")?
            .parse()?;
        // Read-only diagnostic: no clicks, focus changes, or audio capture.
        if std::env::args().nth(2).as_deref() == Some("status") {
            println!("capture_active={}", capture_active(pid)?);
            for window in snapshot(pid)? {
                if window.class == "OimeVoiceWaveWindow" {
                    println!("voice_panel_visible={}", window.visible);
                }
            }
            return Ok(());
        }
        if std::env::args().nth(2).as_deref() == Some("capture") {
            let path = std::env::args()
                .nth(3)
                .context("capture output path required")?;
            for window in snapshot(pid)? {
                if window.visible && window.class == "OimeVoiceWaveWindow" {
                    println!("capture_window={window:?} dpi={}", unsafe {
                        GetDpiForWindow(window.hwnd)
                    });
                    capture(&window, &path)?;
                }
            }
            return Ok(());
        }
        virtual_microphone_selected()?;
        println!("capture_active_before={}", capture_active(pid)?);
        let before = snapshot(pid)?;
        for w in &before {
            println!("before={w:?}");
        }
        ensure!(
            !before
                .iter()
                .any(|w| w.visible && w.class == "OimeVoiceWaveWindow"),
            "voice session already open"
        );
        let bars: Vec<_> = before
            .iter()
            .filter(|w| {
                w.class == "OimeDirectUIWindow"
                    && w.rect.right - w.rect.left == 156
                    && w.rect.bottom - w.rect.top == 45
            })
            .collect();
        ensure!(bars.len() == 1, "expected one validated 156x45 status bar");
        let bar = bars[0];
        unsafe {
            let previous = GetForegroundWindow();
            let host = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("DoubaoMicrophone P0 - empty input test"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                bar.rect.left - 650,
                bar.rect.top - 360,
                640,
                300,
                None,
                None,
                None,
                None,
            )?;
            let _guard = InputWindow { host, previous };
            let edit = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("EDIT"),
                w!(""),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_MULTILINE as u32),
                12,
                12,
                580,
                230,
                Some(host),
                None,
                None,
                None,
            )?;
            let own_thread = GetCurrentThreadId();
            let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let attached = own_thread != foreground_thread
                && AttachThreadInput(own_thread, foreground_thread, true).as_bool();
            let _ = BringWindowToTop(host);
            let _ = SetForegroundWindow(host);
            let _ = SetFocus(Some(edit));
            if attached {
                let _ = AttachThreadInput(own_thread, foreground_thread, false);
            }
            pump(700);
            ensure!(
                GetForegroundWindow() == host,
                "test input window did not gain focus"
            );
            // Coordinates come from the installed status_bar_window.xml. This
            // experiment deliberately rejects other sizes/layouts.
            if !bar.visible {
                let _ = ShowWindow(bar.hwnd, SW_SHOWNOACTIVATE);
            }
            let began = Instant::now();
            let point = LPARAM(54 | (20 << 16));
            for (message, key) in [(WM_MOUSEMOVE, 0), (WM_LBUTTONDOWN, 1), (WM_LBUTTONUP, 0)] {
                PostMessageW(Some(bar.hwnd), message, WPARAM(key), point)?;
            }
            while began.elapsed() < Duration::from_millis(1500) {
                if snapshot(pid)?
                    .iter()
                    .any(|w| w.visible && w.class == "OimeVoiceWaveWindow")
                {
                    break;
                }
                pump(10);
            }
            println!("activation_ms={}", began.elapsed().as_millis());
            pump(100);
            let after = snapshot(pid)?;
            for w in &after {
                println!("after={w:?}");
            }
            let wave: Vec<_> = after
                .iter()
                .filter(|w| w.visible && w.class == "OimeVoiceWaveWindow")
                .collect();
            println!("visible_voice_panels={}", wave.len());
            println!("capture_active_started={}", capture_active(pid)?);
            println!(
                "focus_preserved_after_start={}",
                GetForegroundWindow() == host
            );
            for w in &wave {
                if let Some(path) = std::env::args().nth(2).filter(|p| p != "-") {
                    capture(w, &path)?;
                    println!("captured_test_voice_panel");
                }
            }
            if !wave.is_empty()
                && let Some(wav) = std::env::args().nth(3)
            {
                pump(250);
                let player = play_fixture(wav);
                let deadline = Instant::now() + Duration::from_secs(12);
                while !player.is_finished() && Instant::now() < deadline {
                    pump(10);
                }
                if !player.is_finished() {
                    println!("fixture_playback_timed_out");
                } else {
                    match player.join() {
                        Ok(Ok(())) => println!("fixture_playback_complete"),
                        Ok(Err(e)) => println!("fixture_playback_error={e}"),
                        Err(_) => println!("fixture_playback_thread_failed"),
                    }
                }
                pump(200);
            }
            if !wave.is_empty() {
                for (message, key) in [(WM_MOUSEMOVE, 0), (WM_LBUTTONDOWN, 1), (WM_LBUTTONUP, 0)] {
                    PostMessageW(Some(bar.hwnd), message, WPARAM(key), point)?;
                }
            }
            println!(
                "focus_preserved_after_stop={}",
                GetForegroundWindow() == host
            );
            let commit_deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < commit_deadline {
                pump(20);
                if GetWindowTextLengthW(edit) > 0 {
                    break;
                }
            }
            let length = GetWindowTextLengthW(edit).max(0) as usize;
            let mut text = vec![0u16; length + 1];
            let n = GetWindowTextW(edit, &mut text).max(0) as usize;
            let text = String::from_utf16_lossy(&text[..n]);
            println!("recognized_character_count={n}");
            println!("capture_active_after_stop={}", capture_active(pid)?);
            println!(
                "fixture_matched={}",
                text.contains("手机") && text.contains("语音输入")
            );
            println!(
                "cleanup_voice_visible={}",
                snapshot(pid)?
                    .iter()
                    .any(|w| w.visible && w.class == "OimeVoiceWaveWindow")
            );
            if !bar.visible {
                let _ = ShowWindow(bar.hwnd, SW_HIDE);
            }
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    native::run()
}
#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This probe requires Windows.");
}
