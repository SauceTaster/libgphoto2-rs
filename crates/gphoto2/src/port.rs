//! libgphoto2's I/O port database — the ports (usb, serial, disk, ptpip, …)
//! loaded from the installed iolibs.

use core::ffi::c_int;
use std::ffi::CString;

use gphoto2_sys as sys;

use crate::error::{check, Error, Result};

/// RAII wrapper over `GPPortInfoList`. Internal: used to resolve a port path
/// (e.g. `"disk:/tmp/photos"`) to the port info a [`Camera`](crate::Camera)
/// needs before init.
pub(crate) struct PortInfoList {
    inner: *mut sys::GPPortInfoList,
}

impl PortInfoList {
    /// Allocate and load the list of available ports.
    pub(crate) fn load() -> Result<Self> {
        let mut inner: *mut sys::GPPortInfoList = core::ptr::null_mut();
        check(unsafe { sys::gp_port_info_list_new(&mut inner) })?;
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        let list = Self { inner };
        check(unsafe { sys::gp_port_info_list_load(list.inner) })?;
        Ok(list)
    }

    /// Index of the port matching `path`, or an error if none matches.
    pub(crate) fn lookup_path(&self, path: &str) -> Result<i32> {
        let path = CString::new(path).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))?;
        check(unsafe { sys::gp_port_info_list_lookup_path(self.inner, path.as_ptr()) })
    }

    /// The opaque `GPPortInfo` at `index`.
    ///
    /// The returned handle borrows from this list, so the `PortInfoList` must
    /// outlive any use of it (e.g. the `gp_camera_set_port_info` call).
    pub(crate) fn info_at(&self, index: i32) -> Result<sys::GPPortInfo> {
        let mut info: sys::GPPortInfo = core::ptr::null_mut();
        check(unsafe { sys::gp_port_info_list_get_info(self.inner, index as c_int, &mut info) })?;
        Ok(info)
    }
}

impl Drop for PortInfoList {
    fn drop(&mut self) {
        unsafe { sys::gp_port_info_list_free(self.inner) };
    }
}
