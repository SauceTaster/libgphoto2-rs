//! Error handling.
//!
//! libgphoto2 reports failure as a negative `int` (`GP_OK` is `0`, everything
//! lower is an error). [`Error`] wraps that code, [`ErrorKind`] gives it a
//! structured, matchable identity, and the internal `check` helper turns a raw
//! return value into a [`Result`].

use core::ffi::c_int;
use core::fmt;
use std::ffi::CStr;

use gphoto2_sys as sys;

/// Convenience alias for results from this crate.
pub type Result<T> = core::result::Result<T, Error>;

/// An error returned by libgphoto2.
///
/// Wraps the raw integer result code; use [`Error::kind`] for a structured
/// match or [`Error::message`] for libgphoto2's human-readable string.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Error {
    code: i32,
}

impl Error {
    /// Wrap a raw libgphoto2 result code.
    pub const fn new(code: i32) -> Self {
        Self { code }
    }

    /// The raw libgphoto2 result code (always negative for real errors).
    pub const fn code(self) -> i32 {
        self.code
    }

    /// A structured classification of this error.
    pub fn kind(self) -> ErrorKind {
        ErrorKind::from_code(self.code)
    }

    /// libgphoto2's human-readable description of the code.
    pub fn message(self) -> String {
        // `gp_result_as_string` understands both core and port result codes.
        let ptr = unsafe { sys::gp_result_as_string(self.code) };
        if ptr.is_null() {
            return format!("unknown libgphoto2 error ({})", self.code);
        }
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (libgphoto2 code {})", self.message(), self.code)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Error")
            .field("code", &self.code)
            .field("kind", &self.kind())
            .field("message", &self.message())
            .finish()
    }
}

impl std::error::Error for Error {}

/// A structured classification of a libgphoto2 error code.
///
/// `#[non_exhaustive]`: libgphoto2 may add codes, and unrecognised ones map to
/// [`ErrorKind::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// `GP_ERROR`: unspecified error.
    Generic,
    /// `GP_ERROR_BAD_PARAMETERS`.
    BadParameters,
    /// `GP_ERROR_NO_MEMORY`: out of memory, or camera storage full.
    NoMemory,
    /// `GP_ERROR_LIBRARY`: error inside the camera driver.
    Library,
    /// `GP_ERROR_UNKNOWN_PORT`.
    UnknownPort,
    /// `GP_ERROR_NOT_SUPPORTED`: the operation is not supported.
    NotSupported,
    /// `GP_ERROR_IO`: generic I/O error.
    Io,
    /// `GP_ERROR_FIXED_LIMIT_EXCEEDED`.
    FixedLimitExceeded,
    /// `GP_ERROR_TIMEOUT`.
    Timeout,
    /// `GP_ERROR_IO_USB_CLAIM`: could not claim the USB device (in use?).
    UsbClaim,
    /// `GP_ERROR_IO_USB_FIND`: USB device not found.
    UsbFind,
    /// `GP_ERROR_IO_LOCK`: could not lock the device.
    Lock,
    /// `GP_ERROR_CORRUPTED_DATA`.
    CorruptedData,
    /// `GP_ERROR_FILE_EXISTS`.
    FileExists,
    /// `GP_ERROR_MODEL_NOT_FOUND`.
    ModelNotFound,
    /// `GP_ERROR_DIRECTORY_NOT_FOUND`.
    DirectoryNotFound,
    /// `GP_ERROR_FILE_NOT_FOUND`.
    FileNotFound,
    /// `GP_ERROR_DIRECTORY_EXISTS`.
    DirectoryExists,
    /// `GP_ERROR_CAMERA_BUSY`.
    CameraBusy,
    /// `GP_ERROR_PATH_NOT_ABSOLUTE`.
    PathNotAbsolute,
    /// `GP_ERROR_CANCEL`: operation cancelled via a progress callback.
    Cancel,
    /// `GP_ERROR_CAMERA_ERROR`: the camera reported an error.
    CameraError,
    /// `GP_ERROR_OS_FAILURE`.
    OsFailure,
    /// `GP_ERROR_NO_SPACE`: not enough free space.
    NoSpace,
    /// Any code this crate does not specifically recognise.
    Other(i32),
}

impl ErrorKind {
    /// Classify a raw libgphoto2 result code.
    pub fn from_code(code: i32) -> Self {
        match code {
            sys::GP_ERROR => Self::Generic,
            sys::GP_ERROR_BAD_PARAMETERS => Self::BadParameters,
            sys::GP_ERROR_NO_MEMORY => Self::NoMemory,
            sys::GP_ERROR_LIBRARY => Self::Library,
            sys::GP_ERROR_UNKNOWN_PORT => Self::UnknownPort,
            sys::GP_ERROR_NOT_SUPPORTED => Self::NotSupported,
            sys::GP_ERROR_IO => Self::Io,
            sys::GP_ERROR_FIXED_LIMIT_EXCEEDED => Self::FixedLimitExceeded,
            sys::GP_ERROR_TIMEOUT => Self::Timeout,
            sys::GP_ERROR_IO_USB_CLAIM => Self::UsbClaim,
            sys::GP_ERROR_IO_USB_FIND => Self::UsbFind,
            sys::GP_ERROR_IO_LOCK => Self::Lock,
            sys::GP_ERROR_CORRUPTED_DATA => Self::CorruptedData,
            sys::GP_ERROR_FILE_EXISTS => Self::FileExists,
            sys::GP_ERROR_MODEL_NOT_FOUND => Self::ModelNotFound,
            sys::GP_ERROR_DIRECTORY_NOT_FOUND => Self::DirectoryNotFound,
            sys::GP_ERROR_FILE_NOT_FOUND => Self::FileNotFound,
            sys::GP_ERROR_DIRECTORY_EXISTS => Self::DirectoryExists,
            sys::GP_ERROR_CAMERA_BUSY => Self::CameraBusy,
            sys::GP_ERROR_PATH_NOT_ABSOLUTE => Self::PathNotAbsolute,
            sys::GP_ERROR_CANCEL => Self::Cancel,
            sys::GP_ERROR_CAMERA_ERROR => Self::CameraError,
            sys::GP_ERROR_OS_FAILURE => Self::OsFailure,
            sys::GP_ERROR_NO_SPACE => Self::NoSpace,
            other => Self::Other(other),
        }
    }
}

/// Turn a raw libgphoto2 return value into a [`Result`].
///
/// Non-negative values are success and are returned verbatim (several APIs use
/// the value as a count); negatives become an [`Error`].
#[inline]
pub(crate) fn check(code: c_int) -> Result<i32> {
    // `c_int` is `i32` on every Rust target, so no conversion is needed.
    if code < 0 {
        Err(Error::new(code))
    } else {
        Ok(code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_passes_through_non_negative() {
        assert_eq!(check(0).unwrap(), 0);
        assert_eq!(check(7).unwrap(), 7); // counts are returned verbatim
        assert!(check(-1).is_err());
    }

    #[test]
    fn classifies_known_codes() {
        assert_eq!(
            Error::new(sys::GP_ERROR_MODEL_NOT_FOUND).kind(),
            ErrorKind::ModelNotFound
        );
        assert_eq!(
            Error::new(sys::GP_ERROR_FILE_NOT_FOUND).kind(),
            ErrorKind::FileNotFound
        );
        assert_eq!(
            Error::new(sys::GP_ERROR_NOT_SUPPORTED).kind(),
            ErrorKind::NotSupported
        );
        assert_eq!(Error::new(sys::GP_ERROR).kind(), ErrorKind::Generic);
    }

    #[test]
    fn unknown_code_maps_to_other() {
        assert_eq!(Error::new(-9999).kind(), ErrorKind::Other(-9999));
    }

    #[test]
    fn message_and_display_are_populated() {
        // `message` calls into libgphoto2's gp_result_as_string.
        let err = Error::new(sys::GP_ERROR_MODEL_NOT_FOUND);
        assert!(!err.message().is_empty());
        assert!(err.to_string().contains("-105"));
    }
}
