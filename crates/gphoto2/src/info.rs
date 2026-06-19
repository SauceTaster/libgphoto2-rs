//! File metadata returned by [`Camera::file_info`](crate::Camera::file_info).

use gphoto2_sys as sys;

use crate::util::fixed_buf_to_string;

/// Access permissions reported for a file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilePermissions {
    /// The file can be read/downloaded.
    pub read: bool,
    /// The file can be deleted.
    pub delete: bool,
}

/// Metadata about a file on the camera.
///
/// Each field is `Some` only if the driver actually reported it (libgphoto2
/// flags which members are valid via a bitmask).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInfo {
    /// Size in bytes.
    pub size: Option<u64>,
    /// MIME type, e.g. `"image/jpeg"`.
    pub mime_type: Option<String>,
    /// Pixel width.
    pub width: Option<u32>,
    /// Pixel height.
    pub height: Option<u32>,
    /// Modification time, seconds since the Unix epoch.
    pub mtime: Option<i64>,
    /// Whether the camera considers the file already downloaded.
    pub downloaded: Option<bool>,
    /// Access permissions.
    pub permissions: Option<FilePermissions>,
}

impl FileInfo {
    // `mtime as i64`: time_t is i64 on Linux/macOS but i32 on Windows, so the
    // cast is needed there and a no-op here.
    #[allow(clippy::unnecessary_cast)]
    pub(crate) fn from_raw(info: &sys::CameraFileInfo) -> Self {
        let f = &info.file;
        let has = |bit: sys::CameraFileInfoFields| f.fields.0 & bit.0 != 0;

        FileInfo {
            size: has(sys::CameraFileInfoFields::GP_FILE_INFO_SIZE).then_some(f.size),
            mime_type: has(sys::CameraFileInfoFields::GP_FILE_INFO_TYPE)
                .then(|| fixed_buf_to_string(&f.type_)),
            width: has(sys::CameraFileInfoFields::GP_FILE_INFO_WIDTH).then_some(f.width),
            height: has(sys::CameraFileInfoFields::GP_FILE_INFO_HEIGHT).then_some(f.height),
            mtime: has(sys::CameraFileInfoFields::GP_FILE_INFO_MTIME).then_some(f.mtime as i64),
            downloaded: has(sys::CameraFileInfoFields::GP_FILE_INFO_STATUS)
                .then_some(f.status.0 == sys::CameraFileStatus::GP_FILE_STATUS_DOWNLOADED.0),
            permissions: has(sys::CameraFileInfoFields::GP_FILE_INFO_PERMISSIONS).then_some(
                FilePermissions {
                    read: f.permissions.0 & sys::CameraFilePermissions::GP_FILE_PERM_READ.0 != 0,
                    delete: f.permissions.0 & sys::CameraFilePermissions::GP_FILE_PERM_DELETE.0
                        != 0,
                },
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_set_fields_and_ignores_unset() {
        // SAFETY: CameraFileInfo is plain data; zero is a valid (all-unset) value.
        let mut raw: sys::CameraFileInfo = unsafe { core::mem::zeroed() };
        raw.file.fields = sys::CameraFileInfoFields(
            sys::CameraFileInfoFields::GP_FILE_INFO_SIZE.0
                | sys::CameraFileInfoFields::GP_FILE_INFO_WIDTH.0,
        );
        raw.file.size = 4096;
        raw.file.width = 1920;
        raw.file.height = 1080; // set but its field bit is not -> must be ignored

        let info = FileInfo::from_raw(&raw);
        assert_eq!(info.size, Some(4096));
        assert_eq!(info.width, Some(1920));
        assert_eq!(
            info.height, None,
            "height bit unset -> None even though value is non-zero"
        );
        assert_eq!(info.mime_type, None);
        assert_eq!(info.permissions, None);
    }
}
