#![allow(unsafe_code)]

use std::ffi::c_void;
use std::ptr;

use anyhow::{bail, Result};
use saccade_protocol::{ActionPayload, DispatchStatus, PreparedAction};

use super::{event_plan, NativeStep};

type CFTypeRef = *const c_void;
type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;

#[repr(C)]
#[derive(Clone, Copy)]
struct CGPoint {
    x: f64,
    y: f64,
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn CGPreflightPostEventAccess() -> bool;
    fn CGRequestPostEventAccess() -> bool;
    fn CGEventSourceCreate(state_id: i32) -> CGEventSourceRef;
    fn CGEventCreateMouseEvent(
        source: CGEventSourceRef,
        mouse_type: u32,
        mouse_cursor_position: CGPoint,
        mouse_button: u32,
    ) -> CGEventRef;
    fn CGEventCreateKeyboardEvent(
        source: CGEventSourceRef,
        virtual_key: u16,
        key_down: bool,
    ) -> CGEventRef;
    fn CGEventSetFlags(event: CGEventRef, flags: u64);
    fn CGEventKeyboardSetUnicodeString(event: CGEventRef, length: usize, string: *const u16);
    fn CGEventPost(tap: u32, event: CGEventRef);
    fn CGEventPostToPid(pid: i32, event: CGEventRef);
    fn getppid() -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(value: CFTypeRef);
}

const HID_EVENT_TAP: u32 = 0;
const LEFT_MOUSE_DOWN: u32 = 1;
const LEFT_MOUSE_UP: u32 = 2;
const MOUSE_MOVED: u32 = 5;
const LEFT_BUTTON: u32 = 0;
const KEY_RETURN: u16 = 0x24;
const KEY_COMMAND: u16 = 0x37;
const KEY_SHIFT: u16 = 0x38;
const KEY_G: u16 = 0x05;
const FLAG_SHIFT: u64 = 1 << 17;
const FLAG_COMMAND: u64 = 1 << 20;
const HID_SYSTEM_STATE: i32 = 1;
const KEY_PRESS_DURATION: std::time::Duration = std::time::Duration::from_millis(10);
const TEXT_FOCUS_HANDOFF_DELAY: std::time::Duration = std::time::Duration::from_millis(100);
const CHOICE_POPUP_READY_DELAY: std::time::Duration = std::time::Duration::from_millis(300);

struct EventSource(CGEventSourceRef);

impl Drop for EventSource {
    fn drop(&mut self) {
        // SAFETY: the source was created by CGEventSourceCreate and is released once.
        unsafe { CFRelease(self.0.cast_const()) };
    }
}

pub(super) fn accessibility_trusted() -> bool {
    // SAFETY: no arguments; preflights the permission used by CGEventPost.
    unsafe { CGPreflightPostEventAccess() }
}

pub(super) fn request_accessibility() -> bool {
    // SAFETY: no arguments; asks for the permission used by CGEventPost.
    unsafe { CGRequestPostEventAccess() }
}

pub(super) fn dispatch(
    prepared: &PreparedAction,
    payload: &ActionPayload,
    selection_name: Option<&str>,
) -> Result<DispatchStatus> {
    if !accessibility_trusted() {
        return Ok(DispatchStatus::PermissionRequired);
    }
    let point = CGPoint {
        x: prepared.screen_bounds.x + prepared.screen_bounds.width / 2.0,
        y: prepared.screen_bounds.y + prepared.screen_bounds.height / 2.0,
    };
    // Native Messaging launches the Host as a direct child of the browser.
    // Keep keyboard delivery bound to that exact browser process so another
    // application becoming frontmost between prepare and dispatch cannot
    // receive the text or selection keystrokes.
    let browser_pid = unsafe { getppid() };
    if browser_pid <= 1 {
        bail!("native Host has no valid browser parent process");
    }
    for step in event_plan(prepared, payload, selection_name)? {
        match step {
            NativeStep::PrimaryClick => {
                let source = event_source()?;
                post_mouse(source.0, MOUSE_MOVED, point)?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                post_mouse(source.0, LEFT_MOUSE_DOWN, point)?;
                std::thread::sleep(std::time::Duration::from_millis(50));
                post_mouse(source.0, LEFT_MOUSE_UP, point)?;
            }
            NativeStep::TextFocusHandoff => {
                // Let Chromium finish focus/autofill routing before Unicode is
                // posted. There is no retry if the subsequent verifier fails.
                std::thread::sleep(TEXT_FOCUS_HANDOFF_DELAY);
            }
            NativeStep::UnicodeText => {
                let ActionPayload::Text { text } = payload else {
                    bail!("type requires a text payload");
                };
                post_unicode(browser_pid, text)?;
            }
            NativeStep::ChoicePopupDelay => {
                // Chromium's native menu needs a short handoff before it accepts
                // keyboard input. Success is still decided by the fresh selected
                // option observation, never by this timer.
                std::thread::sleep(CHOICE_POPUP_READY_DELAY);
            }
            NativeStep::ChoiceHome => post_virtual_key(115)?,
            NativeStep::ChoiceNext => post_virtual_key(125)?,
            NativeStep::Return => post_virtual_key(KEY_RETURN)?,
            NativeStep::FileDialogDelay => {
                std::thread::sleep(std::time::Duration::from_millis(1500));
            }
            NativeStep::FileDialogGoTo => post_key_chord(&[KEY_COMMAND, KEY_SHIFT], KEY_G)?,
            NativeStep::FileDialogFieldDelay => {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            NativeStep::FilePathText => {
                let ActionPayload::File { path } = payload else {
                    bail!("file chooser requires a path payload");
                };
                post_unicode(browser_pid, path)?;
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

fn post_virtual_key(key: u16) -> Result<()> {
    for key_down in [true, false] {
        // SAFETY: null source is allowed; checked event is posted and released once.
        let event = unsafe { CGEventCreateKeyboardEvent(ptr::null_mut(), key, key_down) };
        if event.is_null() {
            bail!("CoreGraphics could not create a keyboard event");
        }
        unsafe {
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event.cast_const());
        }
    }
    Ok(())
}

fn post_key_chord(modifiers: &[u16], key: u16) -> Result<()> {
    let mut flags = 0;
    for modifier in modifiers {
        flags |= match *modifier {
            KEY_COMMAND => FLAG_COMMAND,
            KEY_SHIFT => FLAG_SHIFT,
            _ => bail!("unsupported native key modifier"),
        };
        post_key_event(*modifier, true)?;
    }
    for key_down in [true, false] {
        let event = unsafe { CGEventCreateKeyboardEvent(ptr::null_mut(), key, key_down) };
        if event.is_null() {
            bail!("CoreGraphics could not create a keyboard chord event");
        }
        unsafe {
            CGEventSetFlags(event, flags);
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event.cast_const());
        }
    }
    for modifier in modifiers.iter().rev() {
        post_key_event(*modifier, false)?;
    }
    Ok(())
}

fn post_key_event(key: u16, key_down: bool) -> Result<()> {
    let event = unsafe { CGEventCreateKeyboardEvent(ptr::null_mut(), key, key_down) };
    if event.is_null() {
        bail!("CoreGraphics could not create a keyboard event");
    }
    unsafe {
        CGEventPost(HID_EVENT_TAP, event);
        CFRelease(event.cast_const());
    }
    Ok(())
}

fn event_source() -> Result<EventSource> {
    // SAFETY: HID_SYSTEM_STATE is a documented CGEventSourceStateID.
    let source = unsafe { CGEventSourceCreate(HID_SYSTEM_STATE) };
    if source.is_null() {
        bail!("CoreGraphics could not create a HID event source");
    }
    Ok(EventSource(source))
}

fn post_mouse(source: CGEventSourceRef, mouse_type: u32, point: CGPoint) -> Result<()> {
    // SAFETY: source is live; checked event is posted and released once.
    let event = unsafe { CGEventCreateMouseEvent(source, mouse_type, point, LEFT_BUTTON) };
    if event.is_null() {
        bail!("CoreGraphics could not create a mouse event");
    }
    unsafe {
        CGEventPost(HID_EVENT_TAP, event);
        CFRelease(event.cast_const());
    }
    Ok(())
}

fn post_unicode(browser_pid: i32, text: &str) -> Result<()> {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let source = event_source()?;
    for key_down in [true, false] {
        // SAFETY: the source is live and utf16 lives through the synchronous call.
        let event = unsafe { CGEventCreateKeyboardEvent(source.0, 0, key_down) };
        if event.is_null() {
            bail!("CoreGraphics could not create a keyboard event");
        }
        unsafe {
            if key_down {
                CGEventKeyboardSetUnicodeString(event, utf16.len(), utf16.as_ptr());
            }
            CGEventPostToPid(browser_pid, event);
            CFRelease(event.cast_const());
        }
        if key_down {
            std::thread::sleep(KEY_PRESS_DURATION);
        }
    }
    Ok(())
}
