//! Smoke tests that do not require a physical camera. These exercise the FFI
//! boundary (allocation, string marshalling, error translation) end to end.

use gphoto2::{library_version, Camera, Context};

#[test]
fn library_version_is_reported() {
    let version = library_version();
    assert!(!version.is_empty());
    // First entry is the library version, e.g. "2.5.34".
    assert!(version[0].contains('.'));
}

#[test]
fn context_creation_and_autodetect() {
    let context = Context::new().expect("context creation should succeed");
    // With no camera attached this returns an empty list, not an error.
    let cameras = Camera::autodetect(&context).expect("autodetect should not error");
    // Just prove we can read the results without panicking.
    for cam in cameras {
        assert!(!cam.port.is_empty());
    }
}
