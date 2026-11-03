//! Public accessibility metadata and an emptiness bit; never prints input text.
#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use windows::Win32::{
        System::Com::*,
        UI::{Accessibility::*, WindowsAndMessaging::*},
    };
    use windows::core::Interface;
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let result = (|| -> anyhow::Result<()> {
            if std::env::args().nth(1).as_deref() == Some("interfaces") {
                for (name, class) in [("legacy", CUIAutomation), ("modern", CUIAutomation8)] {
                    let a: IUIAutomation = CoCreateInstance(&class, None, CLSCTX_INPROC_SERVER)?;
                    match a.cast::<IUIAutomation2>() {
                        Ok(limits) => println!(
                            "{name}: connection={:?} transaction={:?}",
                            limits.SetConnectionTimeout(150),
                            limits.SetTransactionTimeout(150)
                        ),
                        Err(e) => println!("{name}: IUIAutomation2 error={e}"),
                    }
                }
                return Ok(());
            }
            let a: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let e = a.GetFocusedElement()?;
            let mut pid = 0;
            let window = GetForegroundWindow();
            GetWindowThreadProcessId(window, Some(&mut pid));
            let text = e
                .GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId)
                .and_then(|p| p.DocumentRange())
                .and_then(|r| r.GetText(1))
                .ok();
            let value = e
                .GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId)
                .ok();
            println!(
                "{}",
                serde_json::json!({"foreground_pid":pid,"focused_pid":e.CurrentProcessId()?,
                "kind":e.CurrentControlType()?.0,"class":e.CurrentClassName()?.to_string(),
                "automation_id":e.CurrentAutomationId()?.to_string(),"aria_role":e.CurrentAriaRole()?.to_string(),
                "text_pattern":text.is_some(),"empty":text.map(|t|t.to_string().chars().all(|c| c=='\r'||c=='\n')),
                "value_pattern":value.is_some(),"has_keyboard_focus":e.CurrentHasKeyboardFocus()?.as_bool()})
            );
            Ok(())
        })();
        CoUninitialize();
        result
    }
}
#[cfg(not(target_os = "windows"))]
fn main() {}
