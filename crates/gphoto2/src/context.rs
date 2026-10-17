//! The libgphoto2 [`Context`] and its progress / cancellation / message
//! callbacks.

use core::ffi::{c_char, c_uint, c_void};
use std::cell::RefCell;
use std::panic::{catch_unwind, AssertUnwindSafe};

use gphoto2_sys as sys;

use crate::error::{Error, Result};
use crate::util::cstr_to_string;

/// Receives progress reports for long-running operations (downloads, captures).
///
/// libgphoto2 supports nested progress operations: [`start`](Self::start)
/// returns an id that identifies the operation in later
/// [`update`](Self::update) / [`stop`](Self::stop) calls.
pub trait ProgressHandler {
    /// A new operation began, working toward `target` units. Return an id used
    /// to identify it in [`update`](Self::update) and [`stop`](Self::stop).
    fn start(&mut self, target: f32, message: &str) -> u32;
    /// The operation with `id` has progressed to `current` units.
    fn update(&mut self, id: u32, current: f32);
    /// The operation with `id` finished.
    fn stop(&mut self, id: u32);
}

type CancelFn = Box<dyn FnMut() -> bool>;
type TextFn = Box<dyn FnMut(&str)>;
type ProgressFn = Box<dyn ProgressHandler>;

/// User callbacks, stored on the heap so their address is a stable `data`
/// pointer for the C side. Owned by a [`Context`] via a raw pointer to a
/// `RefCell` — the `RefCell` makes re-entrant callbacks (a handler that calls
/// back into libgphoto2) well-defined instead of aliasing UB.
#[derive(Default)]
struct Handlers {
    cancel: Option<CancelFn>,
    progress: Option<ProgressFn>,
    message: Option<TextFn>,
    status: Option<TextFn>,
    error: Option<TextFn>,
}

/// A libgphoto2 context.
///
/// The context is threaded through almost every camera operation and carries
/// optional callbacks: progress reporting, cancellation, and message/status/
/// error notifications. Create one, optionally attach handlers, then pass it by
/// reference:
///
/// ```no_run
/// # fn main() -> gphoto2::Result<()> {
/// let mut context = gphoto2::Context::new()?;
/// context.set_cancel_handler(|| false); // never cancel
/// # Ok(()) }
/// ```
///
/// Reference-counted in C and released on drop. Intentionally **not**
/// `Send`/`Sync`: libgphoto2 is not thread-safe across a camera session.
///
/// Avoid calling camera operations from within a handler. If you do, the
/// re-entrant callback is skipped (it returns a safe default) so soundness is
/// preserved, but the nested notification is lost.
pub struct Context {
    inner: *mut sys::GPContext,
    // Owned heap allocation (from `Box::into_raw`); its address is the stable
    // `data` pointer handed to every C callback, and is freed in `Drop`. It MUST
    // stay a pointer to a separate allocation (not an inlined field) so the
    // pointer survives moves of `Context` and the trampolines never alias it.
    handlers: *mut RefCell<Handlers>,
}

impl Context {
    /// Create a new context with no callbacks attached.
    pub fn new() -> Result<Self> {
        #[cfg(feature = "log")]
        crate::log_bridge::initialize()?;
        // SAFETY: no preconditions; returns null only on allocation failure.
        let inner = unsafe { sys::gp_context_new() };
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        let handlers = Box::into_raw(Box::new(RefCell::new(Handlers::default())));
        Ok(Self { inner, handlers })
    }

    pub(crate) fn as_ptr(&self) -> *mut sys::GPContext {
        self.inner
    }

    fn data_ptr(&self) -> *mut c_void {
        self.handlers.cast()
    }

    /// Set the cancellation handler. It is polled during long operations; return
    /// `true` to abort (the operation then fails with
    /// [`ErrorKind::Cancel`](crate::ErrorKind::Cancel)).
    pub fn set_cancel_handler(&mut self, handler: impl FnMut() -> bool + 'static) -> &mut Self {
        // SAFETY: we own `handlers`; `&mut self` guarantees no op/callback runs.
        unsafe { (*self.handlers).borrow_mut().cancel = Some(Box::new(handler)) };
        unsafe {
            sys::gp_context_set_cancel_func(self.inner, Some(cancel_trampoline), self.data_ptr());
        }
        self
    }

    /// Set the progress handler (see [`ProgressHandler`]).
    pub fn set_progress_handler(&mut self, handler: impl ProgressHandler + 'static) -> &mut Self {
        unsafe { (*self.handlers).borrow_mut().progress = Some(Box::new(handler)) };
        unsafe {
            sys::gp_context_set_progress_funcs(
                self.inner,
                Some(progress_start_trampoline),
                Some(progress_update_trampoline),
                Some(progress_stop_trampoline),
                self.data_ptr(),
            );
        }
        self
    }

    /// Set the handler for informational messages from the driver.
    pub fn set_message_handler(&mut self, handler: impl FnMut(&str) + 'static) -> &mut Self {
        unsafe { (*self.handlers).borrow_mut().message = Some(Box::new(handler)) };
        unsafe {
            sys::gp_context_set_message_func(self.inner, Some(message_trampoline), self.data_ptr());
        }
        self
    }

    /// Set the handler for status updates from the driver.
    pub fn set_status_handler(&mut self, handler: impl FnMut(&str) + 'static) -> &mut Self {
        unsafe { (*self.handlers).borrow_mut().status = Some(Box::new(handler)) };
        unsafe {
            sys::gp_context_set_status_func(self.inner, Some(status_trampoline), self.data_ptr());
        }
        self
    }

    /// Set the handler for error notifications from the driver.
    pub fn set_error_handler(&mut self, handler: impl FnMut(&str) + 'static) -> &mut Self {
        unsafe { (*self.handlers).borrow_mut().error = Some(Box::new(handler)) };
        unsafe {
            sys::gp_context_set_error_func(self.inner, Some(error_trampoline), self.data_ptr());
        }
        self
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // Order matters: unref the context first so the C side stops calling our
        // trampolines, *then* free the closures. `gp_context_unref` frees the
        // GPContext only at refcount 0; that is always now, because `Context` is
        // non-`Clone` and never exposes `gp_context_ref`, so it holds the sole
        // reference.
        // SAFETY: `inner` came from gp_context_new; `handlers` from Box::into_raw.
        unsafe {
            sys::gp_context_unref(self.inner);
            drop(Box::from_raw(self.handlers));
        }
    }
}

// --- Trampolines: C calling convention, recover `Handlers`, call the closure. ---
//
// Each: guards a null `data`; forms only a *shared* `&RefCell` (shared refs may
// alias, so this is sound even under re-entry); `try_borrow_mut`s — on failure
// (a re-entrant callback) returns a safe default; and runs the closure under
// `catch_unwind` (unwinding across the C frame would be UB).

const FEEDBACK_OK: sys::GPContextFeedback = sys::GPContextFeedback::GP_CONTEXT_FEEDBACK_OK;
const FEEDBACK_CANCEL: sys::GPContextFeedback = sys::GPContextFeedback::GP_CONTEXT_FEEDBACK_CANCEL;

/// Recover the `&RefCell<Handlers>` from the C `data` pointer.
///
/// # Safety
/// `data` must be non-null and the `*mut RefCell<Handlers>` we registered.
unsafe fn handlers<'a>(data: *mut c_void) -> &'a RefCell<Handlers> {
    // SAFETY: a shared reference to the RefCell; aliasing shared refs is allowed.
    unsafe { &*data.cast::<RefCell<Handlers>>() }
}

unsafe extern "C" fn cancel_trampoline(
    _context: *mut sys::GPContext,
    data: *mut c_void,
) -> sys::GPContextFeedback {
    if data.is_null() {
        return FEEDBACK_OK;
    }
    let Ok(mut handlers) = (unsafe { handlers(data) }).try_borrow_mut() else {
        return FEEDBACK_OK;
    };
    let Some(cancel) = handlers.cancel.as_mut() else {
        return FEEDBACK_OK;
    };
    let cancelled = catch_unwind(AssertUnwindSafe(cancel)).unwrap_or(false);
    if cancelled {
        FEEDBACK_CANCEL
    } else {
        FEEDBACK_OK
    }
}

unsafe extern "C" fn progress_start_trampoline(
    _context: *mut sys::GPContext,
    target: f32,
    text: *const c_char,
    data: *mut c_void,
) -> c_uint {
    if data.is_null() {
        return 0;
    }
    let Ok(mut handlers) = (unsafe { handlers(data) }).try_borrow_mut() else {
        return 0;
    };
    let Some(progress) = handlers.progress.as_mut() else {
        return 0;
    };
    catch_unwind(AssertUnwindSafe(|| {
        progress.start(target, &cstr_to_string(text))
    }))
    .unwrap_or(0)
}

unsafe extern "C" fn progress_update_trampoline(
    _context: *mut sys::GPContext,
    id: c_uint,
    current: f32,
    data: *mut c_void,
) {
    if data.is_null() {
        return;
    }
    let Ok(mut handlers) = (unsafe { handlers(data) }).try_borrow_mut() else {
        return;
    };
    if let Some(progress) = handlers.progress.as_mut() {
        let _ = catch_unwind(AssertUnwindSafe(|| progress.update(id, current)));
    }
}

unsafe extern "C" fn progress_stop_trampoline(
    _context: *mut sys::GPContext,
    id: c_uint,
    data: *mut c_void,
) {
    if data.is_null() {
        return;
    }
    let Ok(mut handlers) = (unsafe { handlers(data) }).try_borrow_mut() else {
        return;
    };
    if let Some(progress) = handlers.progress.as_mut() {
        let _ = catch_unwind(AssertUnwindSafe(|| progress.stop(id)));
    }
}

// message / status / error share the same shape: a single text argument.
macro_rules! text_trampoline {
    ($name:ident, $field:ident) => {
        unsafe extern "C" fn $name(
            _context: *mut sys::GPContext,
            text: *const c_char,
            data: *mut c_void,
        ) {
            if data.is_null() {
                return;
            }
            let Ok(mut handlers) = (unsafe { handlers(data) }).try_borrow_mut() else {
                return;
            };
            if let Some(cb) = handlers.$field.as_mut() {
                let _ = catch_unwind(AssertUnwindSafe(|| cb(&cstr_to_string(text))));
            }
        }
    };
}

text_trampoline!(message_trampoline, message);
text_trampoline!(status_trampoline, status);
text_trampoline!(error_trampoline, error);

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::ffi::CString;
    use std::rc::Rc;

    fn data_of(cell: &RefCell<Handlers>) -> *mut c_void {
        (cell as *const RefCell<Handlers>)
            .cast::<c_void>()
            .cast_mut()
    }

    #[test]
    fn text_trampolines_route_to_their_own_field() {
        // Distinct sinks per field prove there's no message/status/error crosswiring.
        let msg = Rc::new(RefCell::new(String::new()));
        let sts = Rc::new(RefCell::new(String::new()));
        let err = Rc::new(RefCell::new(String::new()));
        let handlers = {
            let (m, s, e) = (msg.clone(), sts.clone(), err.clone());
            RefCell::new(Handlers {
                message: Some(Box::new(move |t: &str| *m.borrow_mut() = t.to_owned())),
                status: Some(Box::new(move |t: &str| *s.borrow_mut() = t.to_owned())),
                error: Some(Box::new(move |t: &str| *e.borrow_mut() = t.to_owned())),
                ..Default::default()
            })
        };
        let data = data_of(&handlers);
        let m = CString::new("a message").unwrap();
        let s = CString::new("a status").unwrap();
        let e = CString::new("an error").unwrap();
        unsafe {
            message_trampoline(core::ptr::null_mut(), m.as_ptr(), data);
            status_trampoline(core::ptr::null_mut(), s.as_ptr(), data);
            error_trampoline(core::ptr::null_mut(), e.as_ptr(), data);
        }
        assert_eq!(*msg.borrow(), "a message");
        assert_eq!(*sts.borrow(), "a status");
        assert_eq!(*err.borrow(), "an error");
    }

    #[test]
    fn cancel_trampoline_maps_bool_to_feedback() {
        let yes = RefCell::new(Handlers {
            cancel: Some(Box::new(|| true)),
            ..Default::default()
        });
        let fb = unsafe { cancel_trampoline(core::ptr::null_mut(), data_of(&yes)) };
        assert_eq!(fb.0, FEEDBACK_CANCEL.0);

        let no = RefCell::new(Handlers {
            cancel: Some(Box::new(|| false)),
            ..Default::default()
        });
        let fb = unsafe { cancel_trampoline(core::ptr::null_mut(), data_of(&no)) };
        assert_eq!(fb.0, FEEDBACK_OK.0);
    }

    #[test]
    fn progress_id_round_trips_through_trampolines() {
        struct Rec {
            started: Rc<Cell<bool>>,
            last_id: Rc<Cell<u32>>,
            updates: Rc<Cell<u32>>,
            stopped: Rc<Cell<bool>>,
        }
        impl ProgressHandler for Rec {
            fn start(&mut self, _target: f32, _message: &str) -> u32 {
                self.started.set(true);
                42
            }
            fn update(&mut self, id: u32, _current: f32) {
                self.last_id.set(id);
                self.updates.set(self.updates.get() + 1);
            }
            fn stop(&mut self, id: u32) {
                self.last_id.set(id);
                self.stopped.set(true);
            }
        }
        let started = Rc::new(Cell::new(false));
        let last_id = Rc::new(Cell::new(0));
        let updates = Rc::new(Cell::new(0));
        let stopped = Rc::new(Cell::new(false));
        let handlers = RefCell::new(Handlers {
            progress: Some(Box::new(Rec {
                started: started.clone(),
                last_id: last_id.clone(),
                updates: updates.clone(),
                stopped: stopped.clone(),
            })),
            ..Default::default()
        });
        let data = data_of(&handlers);
        let text = CString::new("copying").unwrap();
        let id =
            unsafe { progress_start_trampoline(core::ptr::null_mut(), 100.0, text.as_ptr(), data) };
        assert_eq!(id, 42);
        unsafe { progress_update_trampoline(core::ptr::null_mut(), id, 50.0, data) };
        unsafe { progress_stop_trampoline(core::ptr::null_mut(), id, data) };
        assert!(started.get() && stopped.get());
        assert_eq!(updates.get(), 1);
        assert_eq!(last_id.get(), 42);
    }

    #[test]
    fn panicking_handler_is_caught_and_defaults() {
        let handlers = RefCell::new(Handlers {
            cancel: Some(Box::new(|| panic!("handler blew up"))),
            ..Default::default()
        });
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let fb = unsafe { cancel_trampoline(core::ptr::null_mut(), data_of(&handlers)) };
        std::panic::set_hook(prev);
        assert_eq!(fb.0, FEEDBACK_OK.0);
    }

    #[test]
    fn reentrant_callback_gets_default_not_double_borrow() {
        // A handler that re-enters a trampoline on the same context must get a
        // safe default (no double-borrow panic, no aliasing UB).
        let cell = Box::into_raw(Box::new(RefCell::new(Handlers::default())));
        let data: *mut c_void = cell.cast();
        let nested = Rc::new(Cell::new(u32::MAX));
        let nested_in = nested.clone();
        // Built outside the `unsafe` block so its inner `unsafe` is not redundant.
        let handler: Box<dyn FnMut() -> bool> = Box::new(move || {
            // Re-enter while the outer trampoline holds the borrow.
            let fb = unsafe { cancel_trampoline(core::ptr::null_mut(), data) };
            nested_in.set(fb.0);
            true
        });
        // SAFETY: `cell` is valid and not otherwise borrowed here.
        unsafe { (*cell).borrow_mut().cancel = Some(handler) };
        let outer = unsafe { cancel_trampoline(core::ptr::null_mut(), data) };
        assert_eq!(outer.0, FEEDBACK_CANCEL.0, "outer should run the handler");
        assert_eq!(
            nested.get(),
            FEEDBACK_OK.0,
            "nested re-entry should get the default"
        );
        // SAFETY: reclaim and free the box exactly once.
        unsafe { drop(Box::from_raw(cell)) };
    }

    #[test]
    fn null_data_and_missing_handlers_are_safe() {
        let fb = unsafe { cancel_trampoline(core::ptr::null_mut(), core::ptr::null_mut()) };
        assert_eq!(fb.0, FEEDBACK_OK.0);
        let empty = RefCell::new(Handlers::default());
        unsafe { message_trampoline(core::ptr::null_mut(), core::ptr::null(), data_of(&empty)) };
    }
}
