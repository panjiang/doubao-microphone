//! Bounded developer-only trace for the verified Doubao 1.0.0.23 build.
//! Hardware execution breakpoints; never patches code or changes control flow.
//! Records numeric stop paths only, not input text, keys, or audio.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
mod trace {
    use anyhow::{Context, Result, ensure};
    use std::{
        collections::HashMap,
        io::Write,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };
    use windows::Win32::{
        Foundation::*,
        Security::*,
        System::{Diagnostics::Debug::*, Threading::*},
    };

    const SITES: [(&str, u64); 4] = [
        ("voice_stop", 0x8785c0),
        ("tsf_stop_intent", 0x891d70),
        ("asr_failure", 0x861d53),
        ("no_speech_timeout", 0x871252),
    ];
    struct Thread(HANDLE);
    #[repr(align(16))]
    struct AlignedContext(CONTEXT);
    impl std::ops::Deref for AlignedContext {
        type Target = CONTEXT;
        fn deref(&self) -> &CONTEXT {
            &self.0
        }
    }
    impl std::ops::DerefMut for AlignedContext {
        fn deref_mut(&mut self) -> &mut CONTEXT {
            &mut self.0
        }
    }
    impl Drop for Thread {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
    fn thread(id: u32) -> Result<Thread> {
        Ok(Thread(unsafe {
            OpenThread(
                THREAD_GET_CONTEXT | THREAD_SET_CONTEXT | THREAD_SUSPEND_RESUME,
                false,
                id,
            )?
        }))
    }
    fn context(handle: HANDLE, flags: CONTEXT_FLAGS) -> Result<AlignedContext> {
        let mut c = AlignedContext(CONTEXT {
            ContextFlags: flags,
            ..Default::default()
        });
        unsafe {
            GetThreadContext(handle, &mut c.0).context("get suspended thread context")?;
        }
        Ok(c)
    }
    fn debug_registers(c: &CONTEXT) -> [u64; 6] {
        [c.Dr0, c.Dr1, c.Dr2, c.Dr3, c.Dr6, c.Dr7]
    }
    fn set_registers(c: &mut CONTEXT, r: [u64; 6]) {
        c.Dr0 = r[0];
        c.Dr1 = r[1];
        c.Dr2 = r[2];
        c.Dr3 = r[3];
        c.Dr6 = r[4];
        c.Dr7 = r[5];
    }
    struct Debugger {
        pid: u32,
        base: u64,
        original: HashMap<u32, [u64; 6]>,
        pending: Option<(u32, u32)>,
    }
    impl Debugger {
        fn install(&mut self, tid: u32) -> Result<()> {
            let t = thread(tid)?;
            let mut c = context(t.0, CONTEXT_DEBUG_REGISTERS_AMD64)?;
            ensure!(
                c.Dr7 & 0xff == 0,
                "existing hardware debugger on thread {tid}"
            );
            self.original.insert(tid, debug_registers(&c));
            let mut sites = [0u64; 6];
            for (i, (_, rva)) in SITES.iter().enumerate() {
                sites[i] = self.base + rva;
            }
            sites[5] = 0x55; // Local execute breakpoints, one byte, four slots.
            set_registers(&mut c, sites);
            unsafe {
                SetThreadContext(t.0, &c.0).context("set hardware breakpoints")?;
            }
            Ok(())
        }
    }
    impl Drop for Debugger {
        fn drop(&mut self) {
            for (tid, original) in &self.original {
                if let Ok(t) = thread(*tid) {
                    let already_suspended = self.pending.is_some();
                    let suspended = already_suspended || unsafe { SuspendThread(t.0) } != u32::MAX;
                    if suspended {
                        if let Ok(mut c) = context(t.0, CONTEXT_DEBUG_REGISTERS_AMD64) {
                            set_registers(&mut c, *original);
                            if let Err(e) = unsafe { SetThreadContext(t.0, &c.0) } {
                                eprintln!("restore thread={tid}: {e}");
                            }
                        }
                        if !already_suspended {
                            unsafe {
                                ResumeThread(t.0);
                            }
                        }
                    }
                }
            }
            if let Some((pid, tid)) = self.pending.take() {
                unsafe {
                    let _ = ContinueDebugEvent(pid, tid, DBG_CONTINUE);
                }
            }
            let detached = unsafe { DebugActiveProcessStop(self.pid) };
            println!(
                "detached={} restored_threads={}",
                detached.is_ok(),
                self.original.len()
            );
        }
    }
    fn emit(value: serde_json::Value) {
        println!("{value}");
        let _ = std::io::stdout().flush();
    }
    pub fn run() -> Result<()> {
        let pid: u32 = std::env::args()
            .nth(1)
            .context("usage: voice_stop_trace <ImeService PID> [seconds<=60]")?
            .parse()?;
        let seconds: u64 = std::env::args().nth(2).unwrap_or("45".into()).parse()?;
        let stop_file = std::env::args().nth(3).map(std::path::PathBuf::from);
        ensure!((1..=60).contains(&seconds), "bounded to 60 seconds");
        unsafe {
            let mut token = HANDLE::default();
            OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
                &mut token,
            )
            .context("open debugger token")?;
            let token = Thread(token);
            let mut luid = LUID::default();
            LookupPrivilegeValueW(None, windows::core::w!("SeDebugPrivilege"), &mut luid)?;
            let privileges = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            AdjustTokenPrivileges(token.0, false, Some(&privileges), 0, None, None)
                .context("enable debugger privilege")?;
            ensure!(
                GetLastError() != ERROR_NOT_ALL_ASSIGNED,
                "current process has no debug privilege"
            );
        }
        let process =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid) }
                .context("open process for trace")?;
        let process = Thread(process);
        let mut name = [0u16; 1024];
        let mut n = name.len() as u32;
        unsafe {
            QueryFullProcessImageNameW(
                process.0,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(name.as_mut_ptr()),
                &mut n,
            )?;
        }
        let path = String::from_utf16_lossy(&name[..n as usize]);
        ensure!(
            path.to_lowercase()
                == format!(
                    "{}\\doubaoime\\versions\\v1.0.0.23\\imeservice.exe",
                    std::env::var("ProgramFiles")?.to_lowercase()
                ),
            "unverified executable"
        );
        let mut present = windows::core::BOOL::default();
        unsafe {
            CheckRemoteDebuggerPresent(process.0, &mut present)
                .context("check existing debugger")?;
        }
        ensure!(!present.as_bool(), "process already being debugged");
        unsafe {
            DebugActiveProcess(pid).context("attach debugger")?;
        }
        let mut debugger = Debugger {
            pid,
            base: 0,
            original: HashMap::new(),
            pending: None,
        };
        unsafe {
            DebugSetProcessKillOnExit(false).context("protect debuggee on exit")?;
        }
        let started = Instant::now();
        let mut initial_break = true;
        while started.elapsed() < Duration::from_secs(seconds)
            && !stop_file.as_ref().is_some_and(|p| p.exists())
        {
            let mut event = DEBUG_EVENT::default();
            if unsafe { WaitForDebugEvent(&mut event, 100) }.is_err() {
                continue;
            }
            debugger.pending = Some((event.dwProcessId, event.dwThreadId));
            let mut continuation = DBG_CONTINUE;
            unsafe {
                match event.dwDebugEventCode {
                    CREATE_PROCESS_DEBUG_EVENT => {
                        let info = event.u.CreateProcessInfo;
                        debugger.base = info.lpBaseOfImage as u64;
                        debugger.install(event.dwThreadId)?;
                        if !info.hFile.is_invalid() {
                            let _ = CloseHandle(info.hFile);
                        }
                    }
                    CREATE_THREAD_DEBUG_EVENT => {
                        debugger.install(event.dwThreadId)?;
                    }
                    EXIT_THREAD_DEBUG_EVENT => {
                        debugger.original.remove(&event.dwThreadId);
                    }
                    LOAD_DLL_DEBUG_EVENT => {
                        let f = event.u.LoadDll.hFile;
                        if !f.is_invalid() {
                            let _ = CloseHandle(f);
                        }
                    }
                    EXIT_PROCESS_DEBUG_EVENT => {
                        debugger.original.clear();
                    }
                    EXCEPTION_DEBUG_EVENT => {
                        let exception = event.u.Exception.ExceptionRecord;
                        let code = exception.ExceptionCode.0 as u32;
                        if code == 0x80000003 && initial_break {
                            initial_break = false;
                            emit(
                                serde_json::json!({"ready":true,"pid":pid,"bounded_seconds":seconds,"threads":debugger.original.len()}),
                            );
                        } else if code == 0x80000004 {
                            let t = thread(event.dwThreadId)?;
                            let mut c = context(
                                t.0,
                                CONTEXT_DEBUG_REGISTERS_AMD64
                                    | CONTEXT_CONTROL_AMD64
                                    | CONTEXT_INTEGER_AMD64,
                            )?;
                            let site = SITES.iter().find(|(_, rva)| debugger.base + rva == c.Rip);
                            if let Some((name, _)) = site {
                                let read_memory =
                                    |address: u64, length: usize| -> Option<Vec<u8>> {
                                        let mut bytes = vec![0u8; length];
                                        ReadProcessMemory(
                                            process.0,
                                            address as *const _,
                                            bytes.as_mut_ptr().cast(),
                                            length,
                                            None,
                                        )
                                        .ok()
                                        .map(|_| bytes)
                                    };
                                let failure = if *name == "asr_failure" {
                                    let numeric = read_memory(c.Rsi, 8);
                                    let text = read_memory(c.Rsi + 8, 32).and_then(|s| {
                                        let length =
                                            u64::from_le_bytes(s[16..24].try_into().ok()?) as usize;
                                        let capacity =
                                            u64::from_le_bytes(s[24..32].try_into().ok()?);
                                        if length > 512
                                            || capacity < length as u64
                                            || capacity > 1048576
                                        {
                                            return None;
                                        }
                                        let bytes = if capacity < 16 {
                                            s[..length].to_vec()
                                        } else {
                                            read_memory(
                                                u64::from_le_bytes(s[..8].try_into().ok()?),
                                                length,
                                            )?
                                        };
                                        String::from_utf8(bytes).ok()
                                    });
                                    numeric.map(|v| serde_json::json!({"type":u32::from_le_bytes(v[..4].try_into().unwrap()),
                                        "raw_status":u32::from_le_bytes(v[4..].try_into().unwrap()),"message":text}))
                                } else {
                                    None
                                };
                                // Read only the return address and fixed numeric message fields.
                                let read_word = |offset: u64| -> Option<u64> {
                                    let mut value = 0u64;
                                    ReadProcessMemory(
                                        process.0,
                                        (c.Rsp + offset) as *const _,
                                        &mut value as *mut _ as *mut _,
                                        8,
                                        None,
                                    )
                                    .ok()
                                    .map(|_| value)
                                };
                                let ret = read_word(0).and_then(|v| v.checked_sub(debugger.base));
                                emit(
                                    serde_json::json!({"at_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64,
                                "elapsed_ms":started.elapsed().as_millis() as u64,"site":name,"tid":event.dwThreadId,
                                "rdx":c.Rdx,"r8":c.R8,"r9":c.R9,"r13_low":c.R13 & 255,
                                "return_rva":ret.map(|r|format!("{r:x}")),
                                "message":if *name=="tsf_stop_intent" {read_word(0x90)}else{None},
                                "wparam":if *name=="tsf_stop_intent" {read_word(0x88)}else{None},"asr_failure":failure}),
                                );
                                c.EFlags |= 1 << 16; // Resume once through the watched instruction.
                                c.Dr6 = 0;
                                SetThreadContext(t.0, &c.0)?;
                            } else {
                                continuation = DBG_EXCEPTION_NOT_HANDLED;
                            }
                        } else {
                            continuation = DBG_EXCEPTION_NOT_HANDLED;
                        }
                    }
                    _ => {}
                }
            }
            unsafe {
                ContinueDebugEvent(event.dwProcessId, event.dwThreadId, continuation)?;
            }
            debugger.pending = None;
        }
        Ok(())
    }
}
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
fn main() -> anyhow::Result<()> {
    trace::run()
}
#[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
fn main() {
    eprintln!("Windows x64 developer diagnostic only");
}
