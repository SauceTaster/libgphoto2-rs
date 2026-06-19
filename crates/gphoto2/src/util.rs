//! Small internal helpers for converting C strings and releasing C allocations.

use core::ffi::{c_char, c_void};
use std::ffi::CStr;

// libgphoto2 hands back malloc'd buffers (event payloads, storage-info arrays)
// that the caller must release. We bind libc `free` directly rather than pull in
// the `libc` crate. Sound only when this binary and libgphoto2 share one C
// allocator, which holds on every supported configuration: a single system libc
// on Linux/macOS, and on Windows the MSYS2/MINGW64 toolchain where the camlib
// DLLs and this crate link the same MinGW CRT.
extern "C" {
    fn free(ptr: *mut c_void);
}

/// Free a pointer that libgphoto2 allocated for us. No-op on null.
///
/// # Safety
/// `ptr` must be null or a live allocation from libgphoto2's C allocator that
/// has not already been freed.
pub(crate) unsafe fn libc_free(ptr: *mut c_void) {
    if !ptr.is_null() {
        // SAFETY: caller guarantees `ptr` is a live C allocation.
        unsafe { free(ptr) };
    }
}

/// Convert a borrowed, NUL-terminated C string pointer to an owned `String`.
/// Returns an empty string for null pointers. Lossy on invalid UTF-8.
pub(crate) fn cstr_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: caller guarantees `ptr` is a valid, NUL-terminated C string.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// Convert a fixed-size, NUL-terminated `char` buffer (as embedded in many
/// libgphoto2 structs, e.g. `CameraAbilities::model`) to an owned `String`.
pub(crate) fn fixed_buf_to_string(buf: &[c_char]) -> String {
    // SAFETY: `c_char` and `u8` share size/alignment; we only read.
    let bytes = unsafe { core::slice::from_raw_parts(buf.as_ptr().cast::<u8>(), buf.len()) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_buffer_stops_at_nul_and_ignores_trailing_bytes() {
        // "hi\0XY\0" must read as just "hi".
        let buf: [c_char; 6] = [
            b'h' as c_char,
            b'i' as c_char,
            0,
            b'X' as c_char,
            b'Y' as c_char,
            0,
        ];
        assert_eq!(fixed_buf_to_string(&buf), "hi");
    }

    #[test]
    fn fixed_buffer_without_nul_uses_whole_buffer() {
        let buf: [c_char; 3] = [b'a' as c_char, b'b' as c_char, b'c' as c_char];
        assert_eq!(fixed_buf_to_string(&buf), "abc");
    }

    #[test]
    fn fixed_buffer_all_zero_is_empty() {
        let buf: [c_char; 4] = [0; 4];
        assert_eq!(fixed_buf_to_string(&buf), "");
    }

    #[test]
    fn null_cstr_pointer_is_empty() {
        assert_eq!(cstr_to_string(core::ptr::null()), "");
    }
}
