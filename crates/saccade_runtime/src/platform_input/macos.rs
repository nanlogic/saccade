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
    fn CGEventKeyboardSetUnicodeString(event: CGEventRef, length: usize, string: *const u16);
    fn CGEventPost(tap: u32, event: CGEventRef);
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
const HID_SYSTEM_STATE: i32 = 1;

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
            NativeStep::UnicodeText => {
                let ActionPayload::Text { text } = payload else {
                    bail!("type requires a text payload");
                };
                post_unicode(text)?;
            }
            NativeStep::ChoicePopupDelay => {
                std::thread::sleep(std::time::Duration::from_millis(750));
            }
            NativeStep::ChoiceTypeahead => post_unicode(selection_name.unwrap_or_default())?,
            NativeStep::Return => post_virtual_key(KEY_RETURN)?,
            NativeStep::PostActionDelay => {
                std::thread::sleep(std::time::Duration::from_millis(300));
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

fn post_unicode(text: &str) -> Result<()> {
    let utf16: Vec<u16> = text.encode_utf16().collect();
    for key_down in [true, false] {
        // SAFETY: null source is allowed; utf16 lives through the synchronous call.
        let event = unsafe { CGEventCreateKeyboardEvent(ptr::null_mut(), 0, key_down) };
        if event.is_null() {
            bail!("CoreGraphics could not create a keyboard event");
        }
        unsafe {
            CGEventKeyboardSetUnicodeString(event, utf16.len(), utf16.as_ptr());
            CGEventPost(HID_EVENT_TAP, event);
            CFRelease(event.cast_const());
        }
    }
    Ok(())
}
