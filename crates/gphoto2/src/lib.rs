//! Safe, idiomatic Rust bindings to [libgphoto2](http://www.gphoto.org/) —
//! detect, capture from, and download files off thousands of digital cameras.
//!
//! This is the safe layer of `vibe-libgphoto2-rs`; it sits on top of the raw
//! FFI in [`gphoto2_sys`]. Resources (contexts, cameras, lists, files) are
//! owned RAII types that free themselves on drop, and every fallible call
//! returns a [`Result`] carrying libgphoto2's structured [`ErrorKind`].
//!
//! # Example
//!
//! ```no_run
//! use gphoto2::{Camera, Context};
//!
//! # fn main() -> gphoto2::Result<()> {
//! let context = Context::new()?;
//!
//! // What's attached?
//! for cam in Camera::autodetect(&context)? {
//!     println!("{} @ {}", cam.model, cam.port);
//! }
//!
//! // Open the camera, capture, and download.
//! let camera = Camera::new(&context)?;
//! let path = camera.capture_image(&context)?;
//! let file = camera.download(&context, &path)?;
//! file.save(&path.name)?;
//! # Ok(())
//! # }
//! ```
//!
//! # Threading
//!
//! libgphoto2 is not thread-safe across a camera session, so [`Context`] and
//! [`Camera`] are deliberately neither `Send` nor `Sync`.

#![forbid(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

pub mod abilities;
pub mod camera;
pub mod context;
pub mod error;
pub mod events;
pub mod file;
pub mod info;
#[cfg(feature = "log")]
pub mod log_bridge;
pub mod storage;
pub mod thread;
pub mod version;
pub mod widget;

mod abilities_list;
mod list;
mod port;
mod util;

pub use abilities::{Abilities, CameraOperations, DriverStatus, FileOperations, FolderOperations};
pub use abilities_list::supported_models;
pub use camera::{Camera, CameraDescriptor, CameraFilePath};
pub use context::{Context, ProgressHandler};
pub use error::{Error, ErrorKind, Result};
pub use events::CameraEvent;
pub use file::{CameraFile, FileType};
pub use info::{FileInfo, FilePermissions};
#[cfg(feature = "log")]
pub use log_bridge::{forward_to_log, LogHandler, LogLevel};
pub use storage::{AccessType, FilesystemType, StorageInfo, StorageType};
pub use thread::CameraThread;
pub use version::{library_version, library_version_verbose};
pub use widget::{Widget, WidgetType, WidgetValue};

/// Common imports for everyday use: `use gphoto2::prelude::*;`.
pub mod prelude {
    pub use crate::{
        Abilities, Camera, CameraDescriptor, CameraEvent, CameraFile, CameraFilePath, CameraThread,
        Context, Error, ErrorKind, FileInfo, FileType, Result, StorageInfo, Widget, WidgetType,
        WidgetValue,
    };
}
