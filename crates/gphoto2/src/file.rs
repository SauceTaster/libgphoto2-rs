//! An in-memory file retrieved from a camera.

use core::ffi::{c_char, c_ulong};
use std::ffi::CString;
use std::path::Path;

use gphoto2_sys as sys;

use crate::error::{check, Error, Result};

/// A file obtained from a camera (e.g. a captured image downloaded via
/// [`Camera::download`](crate::Camera::download)).
///
/// The bytes are owned by libgphoto2 and freed when this value drops.
pub struct CameraFile {
    inner: *mut sys::CameraFile,
}

impl CameraFile {
    /// Allocate a new, empty file object.
    pub fn new() -> Result<Self> {
        let mut inner: *mut sys::CameraFile = core::ptr::null_mut();
        check(unsafe { sys::gp_file_new(&mut inner) })?;
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        Ok(Self { inner })
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::CameraFile {
        self.inner
    }

    /// Borrow the file's raw bytes. The slice is valid for as long as this
    /// `CameraFile` is alive.
    pub fn data(&self) -> Result<&[u8]> {
        let mut ptr: *const c_char = core::ptr::null();
        let mut size: c_ulong = 0;
        check(unsafe { sys::gp_file_get_data_and_size(self.inner, &mut ptr, &mut size) })?;
        if ptr.is_null() || size == 0 {
            return Ok(&[]);
        }
        // SAFETY: libgphoto2 guarantees `ptr` points to `size` valid bytes that
        // live as long as the underlying CameraFile (tied to `&self`).
        Ok(unsafe { core::slice::from_raw_parts(ptr.cast::<u8>(), size as usize) })
    }

    /// Save the file to a local path.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let cpath = path_to_cstring(path.as_ref())?;
        check(unsafe { sys::gp_file_save(self.inner, cpath.as_ptr()) })?;
        Ok(())
    }
}

impl Drop for CameraFile {
    fn drop(&mut self) {
        // SAFETY: `inner` came from `gp_file_new` and is unref'd exactly once.
        unsafe { sys::gp_file_unref(self.inner) };
    }
}

#[cfg(unix)]
fn path_to_cstring(path: &Path) -> Result<CString> {
    use std::os::unix::ffi::OsStrExt;
    CString::new(path.as_os_str().as_bytes()).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))
}

#[cfg(not(unix))]
fn path_to_cstring(path: &Path) -> Result<CString> {
    let s = path
        .to_str()
        .ok_or_else(|| Error::new(sys::GP_ERROR_BAD_PARAMETERS))?;
    CString::new(s).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))
}
