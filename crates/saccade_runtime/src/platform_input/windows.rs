#![allow(unsafe_code)]

use anyhow::{bail, Result};
use saccade_protocol::{ActionPayload, DispatchStatus, PreparedAction};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    KEYEVENTF_UNICODE, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MOVE, MOUSEEVENTF_VIRTUALDESK, MOUSEINPUT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
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
            NativeStep::ChoiceTypeahead => {
                for unit in selection_name.unwrap_or_default().encode_utf16() {
                    send_key(unit, false)?;
                    send_key(unit, true)?;
                }
            }
            NativeStep::Return => send_virtual_key(0x0d)?,
            NativeStep::PostActionDelay => {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        }
    }
    Ok(DispatchStatus::AcceptedByOs)
}

fn click(prepared: &PreparedAction) -> Result<()> {
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
    for key_up in [false, true] {
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
        send(&input)?;
    }
    Ok(())
}

fn send(input: &INPUT) -> Result<()> {
    // SAFETY: input points to one initialized INPUT for the synchronous call.
    if unsafe { SendInput(1, input, std::mem::size_of::<INPUT>() as i32) } != 1 {
        bail!("SendInput rejected the native event");
    }
    Ok(())
}
