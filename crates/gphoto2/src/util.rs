//! Small internal helpers for converting C strings to owned `String`s.

use core::ffi::c_char;
use std::ffi::CStr;

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
