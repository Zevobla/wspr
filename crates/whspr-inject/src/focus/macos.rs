//! macOS focused-element lookup through the Accessibility C API
//! (`ApplicationServices`), with the Core Foundation plumbing it needs.
//! Every Create/Copy result is held in an [`Owned`] that releases it on drop,
//! so no path -- early return or error -- leaks a reference.

use std::ffi::{c_char, c_void, CStr};
use std::ptr;

type CFTypeRef = *const c_void;
type CFStringRef = *const c_void;
type CFIndex = isize;
type CFTypeID = usize;
type Boolean = u8;
type AXUIElementRef = *const c_void;
type AXError = i32;

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;
/// `kAXErrorSuccess`.
const AX_SUCCESS: AXError = 0;
/// How long (seconds) one Accessibility query may wait on an unresponsive
/// app before giving up -- the system default is six seconds, far too long
/// to hold up delivering a dictation.
const AX_MESSAGING_TIMEOUT_SECS: f32 = 0.5;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> Boolean;
    fn AXUIElementCreateSystemWide() -> AXUIElementRef;
    fn AXUIElementGetTypeID() -> CFTypeID;
    fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout_seconds: f32) -> AXError;
    fn AXUIElementCopyAttributeValue(
        element: AXUIElementRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementIsAttributeSettable(
        element: AXUIElementRef,
        attribute: CFStringRef,
        settable: *mut Boolean,
    ) -> AXError;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFGetTypeID(cf: CFTypeRef) -> CFTypeID;
    fn CFStringGetTypeID() -> CFTypeID;
    fn CFStringCreateWithBytes(
        allocator: *const c_void,
        bytes: *const u8,
        num_bytes: CFIndex,
        encoding: u32,
        is_external_representation: Boolean,
    ) -> CFStringRef;
    fn CFStringGetLength(string: CFStringRef) -> CFIndex;
    fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
    fn CFStringGetCString(
        string: CFStringRef,
        buffer: *mut c_char,
        buffer_size: CFIndex,
        encoding: u32,
    ) -> Boolean;
}

/// An owned Core Foundation reference -- the +1 retain a Create or Copy call
/// hands back -- released exactly once, on drop.
struct Owned(CFTypeRef);

impl Owned {
    /// Takes ownership of `reference`, or `None` for a null result.
    fn new(reference: CFTypeRef) -> Option<Self> {
        (!reference.is_null()).then_some(Self(reference))
    }

    /// Whether this reference is of the Core Foundation type `type_id`.
    fn has_type(&self, type_id: CFTypeID) -> bool {
        // SAFETY: `self.0` is a live, non-null CF reference we own.
        unsafe { CFGetTypeID(self.0) == type_id }
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from a Create/Copy call (so we own one
        // retain) and is released only here.
        unsafe { CFRelease(self.0) }
    }
}

/// A new CFString holding a copy of `text`.
fn cf_string(text: &str) -> Option<Owned> {
    let length = CFIndex::try_from(text.len()).ok()?;
    // SAFETY: `text` is valid UTF-8 of exactly `length` bytes; CF copies it.
    Owned::new(unsafe { CFStringCreateWithBytes(ptr::null(), text.as_ptr(), length, UTF8, 0) })
}

/// The contents of a CFString as a Rust `String`; `None` if `string` is not a
/// CFString or does not convert.
fn rust_string(string: &Owned) -> Option<String> {
    // SAFETY: a plain type-ID query with no arguments.
    if !string.has_type(unsafe { CFStringGetTypeID() }) {
        return None;
    }
    // SAFETY: `string` is a live CFString (checked above).
    let length = unsafe { CFStringGetLength(string.0) };
    // SAFETY: a pure size computation; +1 leaves room for the NUL.
    let capacity = unsafe { CFStringGetMaximumSizeForEncoding(length, UTF8) }.checked_add(1)?;
    let mut buffer = vec![0 as c_char; usize::try_from(capacity).ok()?];
    // SAFETY: `buffer` holds exactly `capacity` bytes, as CF is told.
    let converted = unsafe { CFStringGetCString(string.0, buffer.as_mut_ptr(), capacity, UTF8) };
    if converted == 0 {
        return None;
    }
    // SAFETY: on success CF wrote a NUL-terminated string into `buffer`.
    let text = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    text.to_str().ok().map(str::to_owned)
}

/// Copies `element`'s `attribute`; `None` when it has no value or the query
/// fails.
fn copy_attribute(element: &Owned, attribute: &str) -> Option<Owned> {
    let name = cf_string(attribute)?;
    let mut value: CFTypeRef = ptr::null();
    // SAFETY: `element` and `name` are live CF references; `value` is a
    // valid out-pointer that AX fills with a +1 reference on success.
    let status = unsafe { AXUIElementCopyAttributeValue(element.0, name.0, &mut value) };
    if status != AX_SUCCESS {
        return None;
    }
    Owned::new(value)
}

/// Whether `element`'s `attribute` can be set; `None` if the query fails.
fn attribute_settable(element: &Owned, attribute: &str) -> Option<bool> {
    let name = cf_string(attribute)?;
    let mut settable: Boolean = 0;
    // SAFETY: live CF references and a valid out-pointer, as above.
    let status = unsafe { AXUIElementIsAttributeSettable(element.0, name.0, &mut settable) };
    (status == AX_SUCCESS).then_some(settable != 0)
}

/// The keyboard-focused UI element's Accessibility role and whether its
/// `AXValue` is settable. `None` when this process is not trusted for
/// Accessibility (checked without ever raising the permission prompt), or
/// when no focused element can be read.
pub(super) fn focused_element_traits() -> Option<(Option<String>, Option<bool>)> {
    // SAFETY: argument-free query; never prompts (unlike
    // `AXIsProcessTrustedWithOptions`).
    if unsafe { AXIsProcessTrusted() } == 0 {
        return None;
    }
    // SAFETY: returns a +1 reference to the system-wide element.
    let system = Owned::new(unsafe { AXUIElementCreateSystemWide() })?;
    // SAFETY: `system` is a live AXUIElement. Best-effort: on failure the
    // default timeout simply stays in force.
    unsafe { AXUIElementSetMessagingTimeout(system.0, AX_MESSAGING_TIMEOUT_SECS) };
    let focused = copy_attribute(&system, "AXFocusedUIElement")?;
    // SAFETY: argument-free type-ID query.
    if !focused.has_type(unsafe { AXUIElementGetTypeID() }) {
        return None;
    }
    let role = copy_attribute(&focused, "AXRole").and_then(|role| rust_string(&role));
    Some((role, attribute_settable(&focused, "AXValue")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cf_strings_round_trip_ascii_and_non_ascii_text() {
        for text in ["AXTextField", "", "Überfeld \u{2713} 日本語"] {
            let string = cf_string(text).expect("CFStringCreateWithBytes succeeds");
            assert_eq!(rust_string(&string).as_deref(), Some(text));
        }
    }
}
