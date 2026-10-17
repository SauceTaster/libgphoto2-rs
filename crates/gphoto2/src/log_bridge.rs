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
//! libgphoto2's logging is process-global, so this is too. Multiple handles
//! forward each message once, using the most verbose active level.
//!
//! A single C callback is installed before the first context or file is
//! created and remains installed for the lifetime of the process. Dropping a
//! handle disables forwarding in Rust; it never changes the C callback list.
//! With this feature enabled, native code may still format debug messages and
//! USB data hex dumps even when no forwarding handles are active.
//! Code using raw FFI or another libgphoto2 wrapper must coordinate its own
//! changes to that process-global list before starting concurrent operations.

use core::ffi::{c_char, c_void};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use gphoto2_sys as sys;

use crate::error::{check, Error, Result};
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
    fn index(self) -> usize {
        match self {
            LogLevel::Error => 0,
            LogLevel::Verbose => 1,
            LogLevel::Debug => 2,
            LogLevel::Data => 3,
        }
    }
}

struct Forwarders {
    counts: Mutex<[usize; 4]>,
    // Zero disables forwarding; otherwise this is the highest active index + 1.
    level: AtomicU8,
}

impl Forwarders {
    const fn new() -> Self {
        Self {
            counts: Mutex::new([0; 4]),
            level: AtomicU8::new(0),
        }
    }

    fn add(&self, level: LogLevel) -> Result<()> {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        counts[level.index()] = counts[level.index()]
            .checked_add(1)
            .ok_or_else(|| Error::new(sys::GP_ERROR_FIXED_LIMIT_EXCEEDED))?;
        self.update(&counts);
        Ok(())
    }

    fn remove(&self, level: LogLevel) {
        let mut counts = self.counts.lock().unwrap_or_else(|e| e.into_inner());
        counts[level.index()] -= 1;
        self.update(&counts);
    }

    fn update(&self, counts: &[usize; 4]) {
        let level = counts.iter().rposition(|&count| count != 0);
        self.level
            .store(level.map_or(0, |index| index as u8 + 1), Ordering::Release);
    }

    fn enabled(&self, level: sys::GPLogLevel) -> bool {
        level.0 < u32::from(self.level.load(Ordering::Acquire))
    }
}

static FORWARDERS: Forwarders = Forwarders::new();
static INITIALIZED: OnceLock<Result<()>> = OnceLock::new();

/// Bootstrap the C callback before wrapper-owned native objects can emit logs.
/// Every public constructor that can reach logging must call this first.
pub(crate) fn initialize() -> Result<()> {
    *INITIALIZED.get_or_init(|| {
        // SAFETY: all wrapper-owned contexts/files pass this same OnceLock
        // before entering C. No wrapper camera operation can therefore run
        // during registration. The function and null data remain valid for
        // the whole process, and we never remove or replace this callback.
        check(unsafe {
            sys::gp_log_add_func(
                sys::GPLogLevel::GP_LOG_DATA,
                Some(forward),
                core::ptr::null_mut(),
            )
        })
        .map(|_| ())
    })
}

/// An active log forwarder. Dropping it releases its requested level.
///
/// Messages already being forwarded may finish after this handle is dropped.
#[must_use = "keep the LogHandler alive to continue forwarding logs"]
pub struct LogHandler {
    level: LogLevel,
}

impl Drop for LogHandler {
    fn drop(&mut self) {
        FORWARDERS.remove(self.level);
    }
}

/// Forward libgphoto2 log messages (up to `max_level`) to the `log` crate.
///
/// Keep the returned [`LogHandler`] alive for as long as you want logging.
/// Multiple handles combine their levels without duplicating messages. This
/// function and the returned handles may be used from different threads.
pub fn forward_to_log(max_level: LogLevel) -> Result<LogHandler> {
    initialize()?;
    FORWARDERS.add(max_level)?;
    Ok(LogHandler { level: max_level })
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
        if !FORWARDERS.enabled(level) {
            return;
        }
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

    #[test]
    fn handles_combine_levels_and_disable_when_last_drops() {
        let state = Forwarders::new();
        assert!(!state.enabled(sys::GPLogLevel::GP_LOG_ERROR));
        state.add(LogLevel::Error).unwrap();
        state.add(LogLevel::Debug).unwrap();
        state.add(LogLevel::Debug).unwrap();
        assert!(state.enabled(sys::GPLogLevel::GP_LOG_DEBUG));
        assert!(!state.enabled(sys::GPLogLevel::GP_LOG_DATA));
        state.remove(LogLevel::Debug);
        assert!(state.enabled(sys::GPLogLevel::GP_LOG_DEBUG));
        state.remove(LogLevel::Debug);
        assert!(state.enabled(sys::GPLogLevel::GP_LOG_ERROR));
        assert!(!state.enabled(sys::GPLogLevel::GP_LOG_VERBOSE));
        state.remove(LogLevel::Error);
        assert!(!state.enabled(sys::GPLogLevel::GP_LOG_ERROR));
    }

    #[test]
    fn concurrent_handles_leave_forwarding_disabled() {
        let state = Forwarders::new();
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..100 {
                        state.add(LogLevel::Data).unwrap();
                        assert!(state.enabled(sys::GPLogLevel::GP_LOG_DATA));
                        state.remove(LogLevel::Data);
                    }
                });
            }
        });
        assert!(!state.enabled(sys::GPLogLevel::GP_LOG_ERROR));
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
        // Exercise the callback without entering C so this also runs in Miri.
        FORWARDERS.add(LogLevel::Debug).unwrap();
        let _handler = LogHandler {
            level: LogLevel::Debug,
        };
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
