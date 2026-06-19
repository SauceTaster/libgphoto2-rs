//! libgphoto2's driver database — the list of supported camera models loaded
//! from the installed camlibs.

use core::ffi::c_int;
use std::ffi::CString;

use gphoto2_sys as sys;

use crate::context::Context;
use crate::error::{check, Error, Result};
use crate::util::fixed_buf_to_string;

/// RAII wrapper over `CameraAbilitiesList`. Internal: used to resolve a model
/// name to the abilities a [`Camera`](crate::Camera) needs before init.
pub(crate) struct AbilitiesList {
    inner: *mut sys::CameraAbilitiesList,
}

impl AbilitiesList {
    /// Allocate and load the full driver database for this context.
    pub(crate) fn load(context: &Context) -> Result<Self> {
        let mut inner: *mut sys::CameraAbilitiesList = core::ptr::null_mut();
        check(unsafe { sys::gp_abilities_list_new(&mut inner) })?;
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        let list = Self { inner };
        check(unsafe { sys::gp_abilities_list_load(list.inner, context.as_ptr()) })?;
        Ok(list)
    }

    pub(crate) fn count(&self) -> Result<usize> {
        Ok(check(unsafe { sys::gp_abilities_list_count(self.inner) })? as usize)
    }

    /// Index of `model` in the list, or an [`crate::ErrorKind::ModelNotFound`]
    /// error if no driver claims it.
    pub(crate) fn lookup_model(&self, model: &str) -> Result<i32> {
        let model = CString::new(model).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))?;
        check(unsafe { sys::gp_abilities_list_lookup_model(self.inner, model.as_ptr()) })
    }

    pub(crate) fn abilities_at(&self, index: i32) -> Result<sys::CameraAbilities> {
        // SAFETY: zero is a valid bit pattern for this POD struct.
        let mut raw: sys::CameraAbilities = unsafe { core::mem::zeroed() };
        check(unsafe {
            sys::gp_abilities_list_get_abilities(self.inner, index as c_int, &mut raw)
        })?;
        Ok(raw)
    }
}

impl Drop for AbilitiesList {
    fn drop(&mut self) {
        unsafe { sys::gp_abilities_list_free(self.inner) };
    }
}

/// Every camera model name the installed libgphoto2 drivers support.
///
/// This reads the driver database only — it touches no hardware. Handy to
/// confirm a model is available (e.g. the `"Directory Browse"` virtual camera).
pub fn supported_models(context: &Context) -> Result<Vec<String>> {
    let list = AbilitiesList::load(context)?;
    let count = list.count()?;
    let mut models = Vec::with_capacity(count);
    for i in 0..count {
        let abilities = list.abilities_at(i as i32)?;
        models.push(fixed_buf_to_string(&abilities.model));
    }
    Ok(models)
}
