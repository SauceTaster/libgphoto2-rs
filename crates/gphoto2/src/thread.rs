//! Drive a camera from any thread by pinning the session to its own thread.

use std::sync::mpsc::{self, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;

use gphoto2_sys as sys;

use crate::camera::Camera;
use crate::context::Context;
use crate::error::{Error, Result};

type Job = Box<dyn FnOnce(&Camera, &Context) + Send>;

/// A camera session pinned to its own thread.
///
/// libgphoto2 is not thread-safe across a camera session, so [`Camera`] and
/// [`Context`] are `!Send`. `CameraThread` owns them on a dedicated thread and
/// runs operations there on your behalf; the handle itself is **`Send + Sync`**,
/// so you can move it between threads or share it via `Arc`.
///
/// Submit work as closures with [`run`](Self::run). Only `Send` values can come
/// back, which keeps the non-`Send` session on its thread: consume things like
/// [`CameraFile`](crate::CameraFile) *inside* the closure and return owned data
/// (e.g. `file.data()?.to_vec()`), not the file itself.
///
/// ```no_run
/// use gphoto2::{Camera, CameraThread, Context};
///
/// # fn main() -> gphoto2::Result<()> {
/// let cam = CameraThread::spawn(|| {
///     let ctx = Context::new()?;
///     let camera = Camera::new(&ctx)?;
///     Ok((ctx, camera))
/// })?;
///
/// let models = cam.run(|camera, _ctx| camera.abilities().map(|a| a.model()))?;
/// # let _ = models; Ok(())
/// # }
/// ```
pub struct CameraThread {
    // `Mutex<Option<…>>`: the Mutex makes the handle `Sync`; `Option` lets `Drop`
    // close the channel so the worker's recv loop ends.
    sender: Mutex<Option<Sender<Job>>>,
    handle: Option<JoinHandle<()>>,
}

impl CameraThread {
    /// Spawn the worker thread. `init` runs **on that thread** and produces the
    /// session (typically `Context::new()` + a `Camera::new`/`open`). Blocks
    /// until initialization finishes, propagating any error from `init`.
    pub fn spawn<I>(init: I) -> Result<Self>
    where
        I: FnOnce() -> Result<(Context, Camera)> + Send + 'static,
    {
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let (init_tx, init_rx) = mpsc::channel::<Result<()>>();

        let handle = std::thread::spawn(move || {
            let (context, camera) = match init() {
                Ok(session) => {
                    let _ = init_tx.send(Ok(()));
                    session
                }
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };
            // Run jobs until all senders drop. A panicking job is contained so
            // the worker survives for later jobs (its result channel just closes,
            // surfacing as an error to that `run` caller).
            while let Ok(job) = job_rx.recv() {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job(&camera, &context);
                }));
            }
        });

        match init_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                sender: Mutex::new(Some(job_tx)),
                handle: Some(handle),
            }),
            Ok(Err(e)) => {
                let _ = handle.join();
                Err(e)
            }
            // Worker vanished before reporting — treat as a generic failure.
            Err(_) => {
                let _ = handle.join();
                Err(Error::new(sys::GP_ERROR))
            }
        }
    }

    /// Run `f` on the camera thread and wait for its result.
    ///
    /// Returns a generic [`Error`] if the worker thread has stopped (e.g. a
    /// previous job panicked).
    pub fn run<R, F>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&Camera, &Context) -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = mpsc::channel::<R>();
        let job: Job = Box::new(move |camera, context| {
            let _ = tx.send(f(camera, context));
        });

        {
            let guard = self.sender.lock().unwrap_or_else(|e| e.into_inner());
            let sender = guard.as_ref().ok_or_else(|| Error::new(sys::GP_ERROR))?;
            sender.send(job).map_err(|_| Error::new(sys::GP_ERROR))?;
        }
        rx.recv().map_err(|_| Error::new(sys::GP_ERROR))
    }
}

impl Drop for CameraThread {
    fn drop(&mut self) {
        // Close the channel so the worker's recv loop ends, then join it.
        if let Ok(mut guard) = self.sender.lock() {
            guard.take();
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<CameraThread>();
    }
}
