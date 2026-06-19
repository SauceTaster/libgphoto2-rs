//! Camera storage information, from
//! [`Camera::storage_info`](crate::Camera::storage_info).

use gphoto2_sys as sys;

use crate::util::fixed_buf_to_string;

/// Hardware type of a storage (mirrors PTP storage types).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageType {
    /// Unknown / unspecified.
    Unknown,
    /// Fixed ROM.
    FixedRom,
    /// Removable ROM.
    RemovableRom,
    /// Fixed RAM.
    FixedRam,
    /// Removable RAM (memory card, etc.).
    RemovableRam,
    /// A value this wrapper does not recognise.
    Other,
}

impl StorageType {
    fn from_raw(t: sys::CameraStorageType) -> Self {
        match t.0 {
            0 => Self::Unknown,
            1 => Self::FixedRom,
            2 => Self::RemovableRom,
            3 => Self::FixedRam,
            4 => Self::RemovableRam,
            _ => Self::Other,
        }
    }
}

/// Access mode of a storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessType {
    /// Read and write.
    ReadWrite,
    /// Read only.
    ReadOnly,
    /// Read only, but deletion is allowed.
    ReadOnlyWithDelete,
    /// A value this wrapper does not recognise.
    Other,
}

impl AccessType {
    fn from_raw(t: sys::CameraStorageAccessType) -> Self {
        match t.0 {
            0 => Self::ReadWrite,
            1 => Self::ReadOnly,
            2 => Self::ReadOnlyWithDelete,
            _ => Self::Other,
        }
    }
}

/// Filesystem hierarchy type of a storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesystemType {
    /// Undefined / unknown.
    Undefined,
    /// Generic flat (everything in one directory).
    GenericFlat,
    /// Generic tree hierarchy.
    GenericHierarchical,
    /// DCIM-style storage.
    Dcf,
    /// A value this wrapper does not recognise.
    Other,
}

impl FilesystemType {
    fn from_raw(t: sys::CameraStorageFilesystemType) -> Self {
        match t.0 {
            0 => Self::Undefined,
            1 => Self::GenericFlat,
            2 => Self::GenericHierarchical,
            3 => Self::Dcf,
            _ => Self::Other,
        }
    }
}

/// Information about one camera storage (card / partition).
///
/// Each field is `Some` only if the driver reported it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageInfo {
    /// Base directory of the storage (e.g. `"/"`).
    pub base_dir: Option<String>,
    /// Volume label.
    pub label: Option<String>,
    /// Human-readable description.
    pub description: Option<String>,
    /// Hardware type.
    pub storage_type: Option<StorageType>,
    /// Filesystem hierarchy type.
    pub filesystem_type: Option<FilesystemType>,
    /// Access mode.
    pub access: Option<AccessType>,
    /// Total capacity in kilobytes.
    pub capacity_kb: Option<u64>,
    /// Free space in kilobytes.
    pub free_kb: Option<u64>,
    /// Free space expressed as a number of images (camera's own estimate).
    pub free_images: Option<u64>,
}

impl StorageInfo {
    pub(crate) fn from_raw(s: &sys::CameraStorageInformation) -> Self {
        let has = |bit: sys::CameraStorageInfoFields| s.fields.0 & bit.0 != 0;

        StorageInfo {
            base_dir: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_BASE)
                .then(|| fixed_buf_to_string(&s.basedir)),
            label: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_LABEL)
                .then(|| fixed_buf_to_string(&s.label)),
            description: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_DESCRIPTION)
                .then(|| fixed_buf_to_string(&s.description)),
            storage_type: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_STORAGETYPE)
                .then(|| StorageType::from_raw(s.type_)),
            filesystem_type: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_FILESYSTEMTYPE)
                .then(|| FilesystemType::from_raw(s.fstype)),
            access: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_ACCESS)
                .then(|| AccessType::from_raw(s.access)),
            capacity_kb: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_MAXCAPACITY)
                .then_some(s.capacitykbytes),
            free_kb: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_FREESPACEKBYTES)
                .then_some(s.freekbytes),
            free_images: has(sys::CameraStorageInfoFields::GP_STORAGEINFO_FREESPACEIMAGES)
                .then_some(s.freeimages),
        }
    }
}
