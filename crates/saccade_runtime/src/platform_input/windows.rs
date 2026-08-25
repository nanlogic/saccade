#![allow(unsafe_code)]

use anyhow::{bail, Result};
use saccade_protocol::{ActionPayload, DispatchStatus, PreparedAction};
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetSystemMetrics,
    GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, SetForegroundWindow,
    ShowWindow, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_RESTORE,
};

use super::{event_plan, NativeStep};

pub(super) fn dispatch(
    prepared: &PreparedAction,
    payload: &ActionPayload,
    selection_name: Option<&str>,
) -> Result<DispatchStatus> {
    for step in event_plan(prepared, payload, selection_name)? {
        match step {
            NativeStep::PrimaryClick => click(prepared)?,
            NativeStep::TextFocusHandoff => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            NativeStep::UnicodeText => {
                let ActionPayload::Text { text } = payload else {
                    bail!("type requires a text payload");
                };
                for unit in text.encode_utf16() {
                    send_key(unit, false)?;
                    send_key(unit, true)?;
                }
            }
            NativeStep::ChoicePopupDelay => {
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
            NativeStep::ChoiceHome => send_virtual_key(0x24)?,
            NativeStep::ChoiceNext => send_virtual_key(0x28)?,
            NativeStep::ChoiceReturn | NativeStep::FileDialogReturn => send_virtual_key(0x0d)?,
            NativeStep::FileDialogDelay => {
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
            // The Windows Common Item Dialog does not reliably focus the file
            // name field when it opens. Alt+N is the native accelerator for
            // that field; without it, Unicode path input can land in the file
            // list and leave the chooser open without selecting anything.
            NativeStep::FileDialogGoTo => send_key_chord(0x12, 0x4e)?,
            NativeStep::FileDialogFieldDelay => {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            NativeStep::FilePathText => {
                let ActionPayload::File { path } = payload else {
                    bail!("file chooser requires a path payload");
                };
                for unit in path.encode_utf16() {
                    send_key(unit, false)?;
                    send_key(unit, true)?;
                }
            }
            NativeStep::FileDialogSelectionDelay => {
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
            NativeStep::FileDialogUploadDelay => {
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        }
    }
    Ok(DispatchStatus::AcceptedByOs)
}

fn click(prepared: &PreparedAction) -> Result<()> {
    focus_browser_window(prepared)?;
    let (x, y) = absolute_point(prepared);
    send_mouse(
        x,
        y,
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )?;
    send_mouse(
        x,
        y,
        MOUSEEVENTF_LEFTDOWN | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )?;
    send_mouse(
        x,
        y,
        MOUSEEVENTF_LEFTUP | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
    )
}

fn absolute_point(prepared: &PreparedAction) -> (i32, i32) {
    let x = prepared.screen_bounds.x + prepared.screen_bounds.width / 2.0;
    let y = prepared.screen_bounds.y + prepared.screen_bounds.height / 2.0;
    // SAFETY: reads immutable desktop metrics and has no pointer arguments.
    let (left, top, width, height) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN).max(1),
            GetSystemMetrics(SM_CYVIRTUALSCREEN).max(1),
        )
    };
    (
        (((x - left as f64) / (width - 1).max(1) as f64).clamp(0.0, 1.0) * 65535.0) as i32,
        (((y - top as f64) / (height - 1).max(1) as f64).clamp(0.0, 1.0) * 65535.0) as i32,
    )
}

fn send_mouse(dx: i32, dy: i32, flags: u32) -> Result<()> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send(&input)
}

fn send_key(unit: u16, key_up: bool) -> Result<()> {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: unit,
                dwFlags: KEYEVENTF_UNICODE | if key_up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send(&input)
}

fn send_virtual_key(key: u16) -> Result<()> {
    send_virtual_key_state(key, false)?;
    send_virtual_key_state(key, true)?;
    Ok(())
}

fn focus_browser_window(prepared: &PreparedAction) -> Result<()> {
    let mut windows = Vec::<HWND>::new();
    // SAFETY: the callback stores HWND values in the live Vec passed as LPARAM.
    unsafe {
        EnumWindows(
            Some(collect_browser_windows),
            &mut windows as *mut _ as LPARAM,
        )
    };
    let css_x = prepared.screen_bounds.x + prepared.screen_bounds.width / 2.0;
    let css_y = prepared.screen_bounds.y + prepared.screen_bounds.height / 2.0;
    for hwnd in windows {
        // SAFETY: hwnd came from EnumWindows and RECT is initialized by GetWindowRect.
        let mut rect = RECT::default();
        if unsafe { GetWindowRect(hwnd, &mut rect) } == 0
            || css_x < rect.left as f64
            || css_x >= rect.right as f64
            || css_y < rect.top as f64
            || css_y >= rect.bottom as f64
        {
            continue;
        }
        // A synthesized Alt key releases the Windows foreground lock for the
        // same bounded native action; no page content or locator is involved.
        send_virtual_key(0x12)?;
        // SetForegroundWindow may still be denied when the runtime is invoked
        // from a native-messaging process while another Chromium window owns
        // the foreground queue. Temporarily attach the caller to both input
        // queues so the bounded action can activate the exact browser window.
        let current_thread = unsafe { GetCurrentThreadId() };
        let foreground = unsafe { GetForegroundWindow() };
        let foreground_thread =
            unsafe { GetWindowThreadProcessId(foreground, std::ptr::null_mut()) };
        let target_thread = unsafe { GetWindowThreadProcessId(hwnd, std::ptr::null_mut()) };
        unsafe {
            if foreground_thread != 0 && foreground_thread != current_thread {
                AttachThreadInput(current_thread, foreground_thread, 1);
            }
            if target_thread != 0 && target_thread != current_thread {
                AttachThreadInput(current_thread, target_thread, 1);
            }
            ShowWindow(hwnd, SW_RESTORE);
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            if target_thread != 0 && target_thread != current_thread {
                AttachThreadInput(current_thread, target_thread, 0);
            }
            if foreground_thread != 0 && foreground_thread != current_thread {
                AttachThreadInput(current_thread, foreground_thread, 0);
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(150));
        if unsafe { GetForegroundWindow() } != hwnd {
            bail!("browser window could not be focused for native input");
        }
        return Ok(());
    }
    bail!("no focused Chrome or Edge window contains the prepared target")
}

unsafe extern "system" fn collect_browser_windows(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if unsafe { IsWindowVisible(hwnd) } == 0 {
        return 1;
    }
    let mut class_name = [0u16; 128];
    let class_len =
        unsafe { GetClassNameW(hwnd, class_name.as_mut_ptr(), class_name.len() as i32) };
    if class_len <= 0
        || String::from_utf16_lossy(&class_name[..class_len as usize]) != "Chrome_WidgetWin_1"
    {
        return 1;
    }
    let mut title = [0u16; 512];
    let title_len = unsafe { GetWindowTextW(hwnd, title.as_mut_ptr(), title.len() as i32) };
    if title_len <= 0 {
        return 1;
    }
    let title = String::from_utf16_lossy(&title[..title_len as usize]);
    if !title.contains("Google Chrome") && !title.contains("Microsoft Edge") {
        return 1;
    }
    let windows = unsafe { &mut *(lparam as *mut Vec<HWND>) };
    windows.push(hwnd);
    1
}

fn send_key_chord(modifier: u16, key: u16) -> Result<()> {
    send_virtual_key_state(modifier, false)?;
    send_virtual_key_state(key, false)?;
    send_virtual_key_state(key, true)?;
    send_virtual_key_state(modifier, true)
}

fn send_virtual_key_state(key: u16, key_up: bool) -> Result<()> {
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    send(&input)
}

fn send(input: &INPUT) -> Result<()> {
    // SAFETY: input points to one initialized INPUT for the synchronous call.
    if unsafe { SendInput(1, input, std::mem::size_of::<INPUT>() as i32) } != 1 {
        bail!("SendInput rejected the native event");
    }
    Ok(())
}
