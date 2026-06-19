//! An in-memory file retrieved from a camera.

use core::ffi::{c_char, c_ulong};
use std::ffi::CString;
use std::path::Path;

use gphoto2_sys as sys;

use crate::error::{check, Error, Result};
use crate::util::cstr_to_string;

/// Which view of a file to fetch from (or store to) the camera.
///
/// One physical image can offer several views — a full `Normal` image, a small
/// `Preview`, the `Exif` block, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    /// The regular, full-resolution data.
    Normal,
    /// A reduced-size preview / thumbnail.
    Preview,
    /// Raw sensor data (where the driver post-processes the normal view).
    Raw,
    /// Audio data or annotation.
    Audio,
    /// The embedded EXIF metadata block.
    Exif,
    /// File metadata (e.g. on MTP devices).
    Metadata,
}

impl FileType {
    pub(crate) fn to_raw(self) -> sys::CameraFileType {
        match self {
            FileType::Normal => sys::CameraFileType::GP_FILE_TYPE_NORMAL,
            FileType::Preview => sys::CameraFileType::GP_FILE_TYPE_PREVIEW,
            FileType::Raw => sys::CameraFileType::GP_FILE_TYPE_RAW,
            FileType::Audio => sys::CameraFileType::GP_FILE_TYPE_AUDIO,
            FileType::Exif => sys::CameraFileType::GP_FILE_TYPE_EXIF,
            FileType::Metadata => sys::CameraFileType::GP_FILE_TYPE_METADATA,
        }
    }
}

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

    /// Build an in-memory file from bytes — for uploading with
    /// [`Camera::upload`](crate::Camera::upload).
    ///
    /// The `name` is the filename the camera will see; `mime_type` is e.g.
    /// `"image/jpeg"`. The bytes are copied into libgphoto2-owned memory.
    pub fn from_bytes(name: &str, mime_type: &str, data: &[u8]) -> Result<Self> {
        let file = Self::new()?;
        let cname = to_cstring(name)?;
        let cmime = to_cstring(mime_type)?;
        check(unsafe { sys::gp_file_set_name(file.inner, cname.as_ptr()) })?;
        check(unsafe { sys::gp_file_set_mime_type(file.inner, cmime.as_ptr()) })?;
        if !data.is_empty() {
            // `gp_file_append` copies into libgphoto2-managed memory (unlike
            // `set_data_and_size`, which would take ownership of our pointer and
            // later free it with a possibly-different allocator).
            check(unsafe {
                sys::gp_file_append(
                    file.inner,
                    data.as_ptr().cast::<c_char>(),
                    data.len() as c_ulong,
                )
            })?;
        }
        Ok(file)
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::CameraFile {
        self.inner
    }

    /// The file's name, or an empty string if unset.
    pub fn name(&self) -> Result<String> {
        let mut ptr: *const c_char = core::ptr::null();
        check(unsafe { sys::gp_file_get_name(self.inner, &mut ptr) })?;
        Ok(cstr_to_string(ptr))
    }

    /// The file's MIME type (e.g. `"image/jpeg"`), or empty if unknown.
    pub fn mime_type(&self) -> Result<String> {
        let mut ptr: *const c_char = core::ptr::null();
        check(unsafe { sys::gp_file_get_mime_type(self.inner, &mut ptr) })?;
        Ok(cstr_to_string(ptr))
    }

    /// The file's modification time (seconds since the Unix epoch); `0` if unset.
    // `as i64`: normalize libgphoto2's `time_t` (a 64-bit `c_long` on Unix, `i64`
    // on Windows) to a fixed width — a no-op on every 64-bit target.
    #[allow(clippy::unnecessary_cast)]
    pub fn mtime(&self) -> Result<i64> {
        let mut mtime: sys::time_t = 0;
        check(unsafe { sys::gp_file_get_mtime(self.inner, &mut mtime) })?;
        Ok(mtime as i64)
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

fn to_cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))
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
