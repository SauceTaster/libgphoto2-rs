//! The libgphoto2 [`Context`].

use gphoto2_sys as sys;

use crate::error::{Error, Result};

/// A libgphoto2 context.
///
/// The context carries progress/error/cancellation callbacks and is threaded
/// through almost every camera operation. Create one with [`Context::new`] and
/// pass it by reference; it is reference-counted in C and released on drop.
///
/// A `Context` is intentionally **not** `Send`/`Sync`: libgphoto2 is not
/// thread-safe across a single camera session.
pub struct Context {
    inner: *mut sys::GPContext,
}

impl Context {
    /// Create a new context.
    pub fn new() -> Result<Self> {
        // SAFETY: no preconditions; returns null only on allocation failure.
        let inner = unsafe { sys::gp_context_new() };
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        Ok(Self { inner })
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::GPContext {
        self.inner
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: `inner` was created by `gp_context_new` and not yet unref'd.
        unsafe { sys::gp_context_unref(self.inner) };
    }
}
