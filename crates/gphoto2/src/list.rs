//! Thin RAII wrapper over libgphoto2's `CameraList` (a list of name/value
//! pairs used for autodetect results, folder listings, etc.).

use core::ffi::{c_char, c_int};

use gphoto2_sys as sys;

use crate::error::{check, Error, Result};
use crate::util::cstr_to_string;

pub(crate) struct List {
    inner: *mut sys::CameraList,
}

impl List {
    pub(crate) fn new() -> Result<Self> {
        let mut inner: *mut sys::CameraList = core::ptr::null_mut();
        check(unsafe { sys::gp_list_new(&mut inner) })?;
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        Ok(Self { inner })
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::CameraList {
        self.inner
    }

    pub(crate) fn count(&self) -> Result<usize> {
        Ok(check(unsafe { sys::gp_list_count(self.inner) })? as usize)
    }

    /// The (name, value) pair at `index`.
    pub(crate) fn get(&self, index: usize) -> Result<(String, String)> {
        let mut name: *const c_char = core::ptr::null();
        let mut value: *const c_char = core::ptr::null();
        check(unsafe { sys::gp_list_get_name(self.inner, index as c_int, &mut name) })?;
        check(unsafe { sys::gp_list_get_value(self.inner, index as c_int, &mut value) })?;
        Ok((cstr_to_string(name), cstr_to_string(value)))
    }

    /// All entry names, ignoring values (folder/file listings only set names).
    pub(crate) fn names(&self) -> Result<Vec<String>> {
        let count = self.count()?;
        let mut names = Vec::with_capacity(count);
        for i in 0..count {
            let mut name: *const c_char = core::ptr::null();
            check(unsafe { sys::gp_list_get_name(self.inner, i as c_int, &mut name) })?;
            names.push(cstr_to_string(name));
        }
        Ok(names)
    }
}

impl Drop for List {
    fn drop(&mut self) {
        // SAFETY: `inner` came from `gp_list_new` and is freed exactly once.
        unsafe { sys::gp_list_free(self.inner) };
    }
}
