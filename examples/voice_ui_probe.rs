//! Read-only inspection of the voice application's public accessibility surface.
//! No keyboard injection, process patching, or voice/text capture.
#[cfg(target_os = "windows")]
fn main() -> anyhow::Result<()> {
    use anyhow::Context;
    use windows::Win32::{
        System::{Com::*, Variant::VARIANT},
        UI::Accessibility::*,
    };
    unsafe fn walk(
        element: &IUIAutomationElement,
        condition: &IUIAutomationCondition,
        depth: usize,
        remaining: &mut usize,
    ) {
        if depth > 12 || *remaining == 0 {
            return;
        }
        *remaining -= 1;
        // Names are inspected only on action controls, never document/text nodes.
        let kind = unsafe { element.CurrentControlType() }.unwrap_or_default();
        let name = if kind == UIA_ButtonControlTypeId
            || kind == UIA_MenuItemControlTypeId
            || kind == UIA_ComboBoxControlTypeId
            || kind == UIA_ListItemControlTypeId
        {
            unsafe { element.CurrentName() }
                .unwrap_or_default()
                .to_string()
        } else {
            String::new()
        };
        let class = unsafe { element.CurrentClassName() }
            .unwrap_or_default()
            .to_string();
        let id = unsafe { element.CurrentAutomationId() }
            .unwrap_or_default()
            .to_string();
        let invoke = unsafe {
            element.GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)
        }
        .is_ok();
        println!(
            "{}class={class:?} id={id:?} kind={} invoke={invoke} action={:?}",
            " ".repeat(depth * 2),
            kind.0,
            name.chars().take(30).collect::<String>()
        );
        if let Ok(children) = unsafe { element.FindAll(TreeScope_Children, condition) } {
            for i in 0..unsafe { children.Length() }.unwrap_or(0) {
                if let Ok(child) = unsafe { children.GetElement(i) } {
                    unsafe { walk(&child, condition, depth + 1, remaining) };
                }
            }
        }
    }
    let pid: i32 = std::env::args()
        .nth(1)
        .context("usage: voice_ui_probe <ImeService process ID>")?
        .parse()?;
    std::thread::spawn(|| {
        use cpal::traits::{DeviceTrait, HostTrait};
        if let Some(input) = cpal::default_host().default_input_device() {
            if let Ok(description) = input.description() {
                println!("default_input={:?}", description.name());
            }
        }
    })
    .join()
    .map_err(|_| anyhow::anyhow!("audio device enumeration failed"))?;
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
        let result = (|| -> anyhow::Result<()> {
            let automation: IUIAutomation =
                CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let root = automation.GetRootElement()?;
            let process =
                automation.CreatePropertyCondition(UIA_ProcessIdPropertyId, &VARIANT::from(pid))?;
            let all = automation.CreateTrueCondition()?;
            if matches!(
                std::env::args().nth(2).as_deref(),
                Some("microphones" | "select-cable")
            ) {
                let id = automation.CreatePropertyCondition(
                    UIA_AutomationIdPropertyId,
                    &VARIANT::from("MicrophoneButton"),
                )?;
                let condition = automation.CreateAndCondition(&process, &id)?;
                let buttons = root.FindAll(TreeScope_Descendants, &condition)?;
                anyhow::ensure!(buttons.Length()? == 1, "expected one microphone selector");
                let class = automation.CreatePropertyCondition(
                    UIA_ClassNamePropertyId,
                    &VARIANT::from("MicrophonePickerDialog"),
                )?;
                let condition = automation.CreateAndCondition(&process, &class)?;
                let dialogs = root.FindAll(TreeScope_Descendants, &condition)?;
                anyhow::ensure!(dialogs.Length()? == 1, "expected one microphone dialog");
                let dialog = dialogs.GetElement(0)?;
                if dialog.CurrentIsOffscreen()?.as_bool() {
                    buttons
                        .GetElement(0)?
                        .GetCurrentPatternAs::<IUIAutomationInvokePattern>(UIA_InvokePatternId)?
                        .Invoke()?;
                    std::thread::sleep(std::time::Duration::from_millis(200));
                }
                let label = automation.CreatePropertyCondition(
                    UIA_AutomationIdPropertyId,
                    &VARIANT::from("NameText"),
                )?;
                let labels = dialog.FindAll(TreeScope_Descendants, &label)?;
                let mut cable = None;
                for i in 0..labels.Length()? {
                    let element = labels.GetElement(i)?;
                    let name = element.CurrentName()?.to_string();
                    println!("microphone_choice={name:?}");
                    if name.contains("CABLE Output") {
                        anyhow::ensure!(cable.is_none(), "multiple cable inputs");
                        cable = Some(element);
                    }
                }
                if std::env::args().nth(2).as_deref() == Some("select-cable") {
                    let element = cable.context("CABLE Output is not listed")?;
                    let item = automation.ControlViewWalker()?.GetParentElement(&element)?;
                    item.GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(
                        UIA_SelectionItemPatternId,
                    )?
                    .Select()?;
                    println!("selected=CABLE Output");
                    let button_kind = automation.CreatePropertyCondition(
                        UIA_ControlTypePropertyId,
                        &VARIANT::from(UIA_ButtonControlTypeId.0),
                    )?;
                    let controls = dialog.FindAll(TreeScope_Descendants, &button_kind)?;
                    for i in 0..controls.Length()? {
                        let button = controls.GetElement(i)?;
                        println!(
                            "dialog_button id={:?} name={:?}",
                            button.CurrentAutomationId()?.to_string(),
                            button.CurrentName()?.to_string()
                        );
                        let children = button.FindAll(TreeScope_Descendants, &all)?;
                        for k in 0..children.Length()? {
                            let text = children.GetElement(k)?.CurrentName()?.to_string();
                            if !text.is_empty() {
                                println!("dialog_button_label={text:?}");
                            }
                        }
                    }
                }
                return Ok(());
            }
            let windows = root.FindAll(TreeScope_Children, &process)?;
            let n = windows.Length()?;
            println!("target_process={pid} accessible_windows={n}");
            let mut remaining = 300;
            for i in 0..n {
                let window = windows.GetElement(i)?;
                println!(
                    "window_handle={:?} bounds={:?}",
                    window.CurrentNativeWindowHandle()?,
                    window.CurrentBoundingRectangle()?
                );
                walk(&window, &all, 0, &mut remaining);
            }
            Ok(())
        })();
        CoUninitialize();
        result
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("This probe requires Windows.");
}
