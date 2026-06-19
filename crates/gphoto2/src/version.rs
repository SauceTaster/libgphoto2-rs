//! Version information about the linked libgphoto2.

use std::ffi::CStr;

use gphoto2_sys as sys;

/// Short version/feature strings for the linked libgphoto2. The first entry is
/// the library version (e.g. `"2.5.34"`); the rest are build/feature flags such
/// as `"standard camlibs"`, `"no ltdl"`, `"EXIF"`.
pub fn library_version() -> Vec<String> {
    collect(sys::GPVersionVerbosity::GP_VERSION_SHORT)
}

/// Like [`library_version`] but each entry carries a human-readable
/// description (e.g. the full camlib set, what each build flag means).
pub fn library_version_verbose() -> Vec<String> {
    collect(sys::GPVersionVerbosity::GP_VERSION_VERBOSE)
}

fn collect(verbosity: sys::GPVersionVerbosity) -> Vec<String> {
    let mut out = Vec::new();
    // SAFETY: `gp_library_version` returns a NULL-terminated array of valid,
    // static C strings (or null). We only read it.
    unsafe {
        let mut cursor = sys::gp_library_version(verbosity);
        if cursor.is_null() {
            return out;
        }
        while !(*cursor).is_null() {
            out.push(CStr::from_ptr(*cursor).to_string_lossy().into_owned());
            cursor = cursor.add(1);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_version() {
        let v = library_version();
        assert!(!v.is_empty(), "libgphoto2 should report version strings");
        // The first entry is the library version, e.g. "2.5.34".
        let version = &v[0];
        assert!(
            version.starts_with(|c: char| c.is_ascii_digit()) && version.contains('.'),
            "first entry {version:?} should look like a version number"
        );
    }
}
