//! Bridge libgphoto2's internal logging to the [`log`] crate.
//!
//! Enabled by the `log` cargo feature. Call [`forward_to_log`] once and keep the
//! returned [`LogHandler`] alive; libgphoto2's messages are then emitted through
//! the `log` facade on target `"gphoto2"`. (For `tracing`, use the
//! [`tracing-log`](https://crates.io/crates/tracing-log) adapter.)
//!
//! ```no_run
//! # #[cfg(feature = "log")]
//! # fn main() -> gphoto2::Result<()> {
//! let _logging = gphoto2::log_bridge::forward_to_log(gphoto2::log_bridge::LogLevel::Debug)?;
//! // ... libgphoto2 calls now log on target "gphoto2" ...
//! # Ok(()) }
//! # #[cfg(not(feature = "log"))] fn main() {}
//! ```
//!
//! libgphoto2's logging is process-global, so this is too.

use core::ffi::{c_char, c_int, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};

use gphoto2_sys as sys;

use crate::error::{check, Result};
use crate::util::cstr_to_string;

/// The most verbose libgphoto2 level to forward.
///
/// Levels are cumulative: [`LogLevel::Debug`] also delivers `Verbose` and
/// `Error`. They map to `log` levels as: `Error → Error`, `Verbose → Info`,
/// `Debug → Debug`, `Data → Trace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Errors only.
    Error,
    /// Errors and verbose messages.
    Verbose,
    /// The above plus debug messages.
    Debug,
    /// Everything, including hex data dumps.
    Data,
}

impl LogLevel {
    fn to_raw(self) -> sys::GPLogLevel {
        match self {
            LogLevel::Error => sys::GPLogLevel::GP_LOG_ERROR,
            LogLevel::Verbose => sys::GPLogLevel::GP_LOG_VERBOSE,
            LogLevel::Debug => sys::GPLogLevel::GP_LOG_DEBUG,
            LogLevel::Data => sys::GPLogLevel::GP_LOG_DATA,
        }
    }
}

/// A registered log forwarder. Logging continues until this is dropped.
#[must_use = "dropping the LogHandler unregisters the log forwarder"]
pub struct LogHandler {
    id: c_int,
}

impl Drop for LogHandler {
    fn drop(&mut self) {
        // SAFETY: `id` came from gp_log_add_func and is removed exactly once.
        unsafe { sys::gp_log_remove_func(self.id) };
    }
}

/// Forward libgphoto2 log messages (up to `max_level`) to the `log` crate.
///
/// Keep the returned [`LogHandler`] alive for as long as you want logging.
pub fn forward_to_log(max_level: LogLevel) -> Result<LogHandler> {
    // SAFETY: `forward` is a valid `extern "C"` fn; it ignores the null `data`.
    let id = check(unsafe {
        sys::gp_log_add_func(max_level.to_raw(), Some(forward), core::ptr::null_mut())
    })?;
    Ok(LogHandler { id })
}

fn to_log_level(raw: sys::GPLogLevel) -> log::Level {
    match raw.0 {
        0 => log::Level::Error, // GP_LOG_ERROR
        1 => log::Level::Info,  // GP_LOG_VERBOSE
        2 => log::Level::Debug, // GP_LOG_DEBUG
        _ => log::Level::Trace, // GP_LOG_DATA (hex dumps)
    }
}

unsafe extern "C" fn forward(
    level: sys::GPLogLevel,
    domain: *const c_char,
    message: *const c_char,
    _data: *mut c_void,
) {
    // Never unwind across the C frame.
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let level = to_log_level(level);
        if !log::log_enabled!(target: "gphoto2", level) {
            return;
        }
        let domain = cstr_to_string(domain);
        let message = cstr_to_string(message);
        log::log!(target: "gphoto2", level, "[{domain}] {message}");
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;
    use std::sync::Mutex;

    #[test]
    fn levels_map_to_log_crate() {
        assert_eq!(
            to_log_level(sys::GPLogLevel::GP_LOG_ERROR),
            log::Level::Error
        );
        assert_eq!(
            to_log_level(sys::GPLogLevel::GP_LOG_VERBOSE),
            log::Level::Info
        );
        assert_eq!(
            to_log_level(sys::GPLogLevel::GP_LOG_DEBUG),
            log::Level::Debug
        );
        assert_eq!(
            to_log_level(sys::GPLogLevel::GP_LOG_DATA),
            log::Level::Trace
        );
    }

    static RECORDS: Mutex<Vec<String>> = Mutex::new(Vec::new());

    struct Recorder;
    impl log::Log for Recorder {
        fn enabled(&self, _: &log::Metadata) -> bool {
            true
        }
        fn log(&self, record: &log::Record) {
            if record.target() == "gphoto2" {
                RECORDS.lock().unwrap().push(record.args().to_string());
            }
        }
        fn flush(&self) {}
    }
    static LOGGER: Recorder = Recorder;

    // Invoke the trampoline directly (rather than relying on libgphoto2 to emit a
    // message — distro builds often compile debug logging out) and confirm it
    // routes through the `log` crate, on the right target, with domain + message.
    #[test]
    fn forward_routes_a_message_through_the_log_crate() {
        let _ = log::set_logger(&LOGGER);
        log::set_max_level(log::LevelFilter::Trace);
        RECORDS.lock().unwrap().clear();

        let domain = CString::new("ptp2").unwrap();
        let message = CString::new("could not claim the USB device").unwrap();
        // SAFETY: valid C strings; the forwarder ignores the null user data.
        unsafe {
            forward(
                sys::GPLogLevel::GP_LOG_ERROR,
                domain.as_ptr(),
                message.as_ptr(),
                core::ptr::null_mut(),
            );
        }

        let records = RECORDS.lock().unwrap();
        assert!(
            records
                .iter()
                .any(|r| r.contains("ptp2") && r.contains("could not claim the USB device")),
            "records: {records:?}"
        );
    }
}
