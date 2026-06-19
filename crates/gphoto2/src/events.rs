//! Camera events, as delivered by `gp_camera_wait_for_event`.
//!
//! Events drive tethered workflows: after [`Camera::trigger_capture`] you poll
//! [`Camera::wait_for_event`] until a [`CameraEvent::FileAdded`] /
//! [`CameraEvent::CaptureComplete`] arrives, then download the result.
//!
//! [`Camera::trigger_capture`]: crate::Camera::trigger_capture
//! [`Camera::wait_for_event`]: crate::Camera::wait_for_event

use core::ffi::{c_char, c_void};

use gphoto2_sys as sys;

use crate::camera::CameraFilePath;
use crate::util::{cstr_to_string, fixed_buf_to_string};

// libgphoto2 mallocs the event payload and documents that the caller frees it.
// We bind libc `free` directly rather than pull in the `libc` crate. This is
// sound only when this binary and libgphoto2 share one C allocator, which holds
// on every supported configuration: a single system libc on Linux/macOS, and on
// Windows the MSYS2/MINGW64 toolchain where the camlib DLLs and this crate link
// the same MinGW CRT. (Mixing a foreign-CRT libgphoto2 would be a heap mismatch.)
extern "C" {
    fn free(ptr: *mut c_void);
}

/// An event reported by a camera.
///
/// `#[non_exhaustive]`: libgphoto2 may add event types; unrecognised ones arrive
/// as [`CameraEvent::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum CameraEvent {
    /// Nothing happened within the wait timeout.
    Timeout,
    /// A new file appeared on the camera (e.g. a freshly captured image).
    FileAdded(CameraFilePath),
    /// A new folder appeared on the camera.
    FolderAdded(CameraFilePath),
    /// An existing file changed.
    FileChanged(CameraFilePath),
    /// The most recent capture completed.
    CaptureComplete,
    /// A driver-specific event the wrapper doesn't model; carries any text the
    /// driver provided (often empty).
    Unknown(String),
}

/// Interpret a raw `(eventtype, eventdata)` pair from
/// `gp_camera_wait_for_event`, taking ownership of `data` and freeing it.
///
/// # Safety
/// `data` must be exactly what libgphoto2 returned for `ty`: either null, a
/// malloc'd [`sys::CameraFilePath`] (file/folder events), or a malloc'd C string
/// (unknown events). It is freed here and must not be used afterwards.
pub(crate) unsafe fn interpret_event(ty: sys::CameraEventType, data: *mut c_void) -> CameraEvent {
    // SAFETY: per this fn's contract, `data` matches `ty` and is ours to read.
    let event = unsafe {
        match ty.0 {
            1 => CameraEvent::Timeout,                      // GP_EVENT_TIMEOUT
            2 => CameraEvent::FileAdded(read_path(data)),   // GP_EVENT_FILE_ADDED
            3 => CameraEvent::FolderAdded(read_path(data)), // GP_EVENT_FOLDER_ADDED
            4 => CameraEvent::CaptureComplete,              // GP_EVENT_CAPTURE_COMPLETE
            5 => CameraEvent::FileChanged(read_path(data)), // GP_EVENT_FILE_CHANGED
            _ => CameraEvent::Unknown(read_unknown(data)),  // GP_EVENT_UNKNOWN (0) / future
        }
    };

    if !data.is_null() {
        // SAFETY: libgphoto2 allocated `data` with the C allocator; we own it.
        unsafe { free(data) };
    }
    event
}

/// Read a `CameraFilePath` payload (does not free).
///
/// # Safety
/// `data` must be null or point to a valid `CameraFilePath`.
unsafe fn read_path(data: *mut c_void) -> CameraFilePath {
    let p = data.cast::<sys::CameraFilePath>();
    if p.is_null() {
        return CameraFilePath {
            folder: String::new(),
            name: String::new(),
        };
    }
    // SAFETY: `p` is non-null and points to a valid `CameraFilePath`.
    unsafe {
        CameraFilePath {
            folder: fixed_buf_to_string(&(*p).folder),
            name: fixed_buf_to_string(&(*p).name),
        }
    }
}

/// Read an unknown-event C string payload (does not free).
///
/// # Safety
/// `data` must be null or a valid NUL-terminated C string.
unsafe fn read_unknown(data: *mut c_void) -> String {
    if data.is_null() {
        String::new()
    } else {
        cstr_to_string(data.cast::<c_char>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    extern "C" {
        fn malloc(size: usize) -> *mut c_void;
    }

    /// Allocate a `CameraFilePath` the way libgphoto2 would, so `interpret_event`
    /// can read and `free` it through the real C allocator.
    unsafe fn malloc_path(folder: &str, name: &str) -> *mut c_void {
        // SAFETY: we allocate and fully initialise a CameraFilePath.
        unsafe {
            let p =
                malloc(core::mem::size_of::<sys::CameraFilePath>()).cast::<sys::CameraFilePath>();
            assert!(!p.is_null());
            core::ptr::write_bytes(p, 0, 1);
            copy_into(&mut (*p).folder, folder);
            copy_into(&mut (*p).name, name);
            p.cast()
        }
    }

    /// Copy `s` (+ NUL) into a fixed C char buffer. Safe: operates on a slice.
    fn copy_into(buf: &mut [c_char], s: &str) {
        let bytes = s.as_bytes();
        assert!(bytes.len() < buf.len());
        for (slot, &b) in buf.iter_mut().zip(bytes) {
            *slot = b as c_char;
        }
        buf[bytes.len()] = 0;
    }

    unsafe fn malloc_cstr(s: &str) -> *mut c_void {
        let bytes = s.as_bytes();
        // SAFETY: we allocate len+1 bytes and write exactly that many.
        unsafe {
            let p = malloc(bytes.len() + 1).cast::<u8>();
            assert!(!p.is_null());
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
            *p.add(bytes.len()) = 0;
            p.cast()
        }
    }

    #[test]
    fn file_added_is_parsed_and_freed() {
        let ev = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_FILE_ADDED,
                malloc_path("/store_00010001/DCIM/100GPHOT", "GPH_0099.JPG"),
            )
        };
        assert_eq!(
            ev,
            CameraEvent::FileAdded(CameraFilePath {
                folder: "/store_00010001/DCIM/100GPHOT".into(),
                name: "GPH_0099.JPG".into(),
            })
        );
    }

    #[test]
    fn folder_added_and_file_changed_are_parsed() {
        let folder = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_FOLDER_ADDED,
                malloc_path("/store", "newdir"),
            )
        };
        assert!(matches!(folder, CameraEvent::FolderAdded(p) if p.name == "newdir"));

        let changed = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_FILE_CHANGED,
                malloc_path("/store", "IMG_0001.JPG"),
            )
        };
        assert!(matches!(changed, CameraEvent::FileChanged(p) if p.name == "IMG_0001.JPG"));
    }

    #[test]
    fn timeout_and_capture_complete_carry_no_data() {
        let timeout = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_TIMEOUT,
                core::ptr::null_mut(),
            )
        };
        assert_eq!(timeout, CameraEvent::Timeout);

        let complete = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_CAPTURE_COMPLETE,
                core::ptr::null_mut(),
            )
        };
        assert_eq!(complete, CameraEvent::CaptureComplete);
    }

    #[test]
    fn unknown_event_carries_text_or_empty() {
        let with_text = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_UNKNOWN,
                malloc_cstr("PTP Property d10e changed"),
            )
        };
        assert_eq!(
            with_text,
            CameraEvent::Unknown("PTP Property d10e changed".into())
        );

        let empty = unsafe {
            interpret_event(
                sys::CameraEventType::GP_EVENT_UNKNOWN,
                core::ptr::null_mut(),
            )
        };
        assert_eq!(empty, CameraEvent::Unknown(String::new()));
    }
}
