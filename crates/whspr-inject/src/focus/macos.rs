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

/// `kCFStringEncodingUTF8`.
const UTF8: u32 = 0x0800_0100;

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
