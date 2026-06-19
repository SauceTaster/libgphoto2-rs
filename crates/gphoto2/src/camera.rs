//! The [`Camera`] — a connected camera you can detect, capture from, and
//! download files from.

use core::ffi::{c_int, c_void};
use std::ffi::CString;
use std::time::Duration;

use gphoto2_sys as sys;

use crate::abilities::Abilities;
use crate::abilities_list::AbilitiesList;
use crate::context::Context;
use crate::error::{check, Error, Result};
use crate::events::{interpret_event, CameraEvent};
use crate::file::{CameraFile, FileType};
use crate::info::FileInfo;
use crate::list::List;
use crate::port::PortInfoList;
use crate::storage::StorageInfo;
use crate::util::{fixed_buf_to_string, libc_free};
use crate::widget::Widget;

/// A camera discovered on a port: its model name and gphoto2 port path
/// (e.g. model `"Canon EOS R"`, port `"usb:001,007"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraDescriptor {
    /// The detected model name.
    pub model: String,
    /// The gphoto2 port path the camera was found on.
    pub port: String,
}

/// A folder + filename identifying an object on the camera's filesystem,
/// as produced by a capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraFilePath {
    /// Absolute folder on the camera (e.g. `"/store_00010001/DCIM/100CANON"`).
    pub folder: String,
    /// File name within `folder` (e.g. `"IMG_0001.JPG"`).
    pub name: String,
}

/// A connected camera.
///
/// Not `Send`/`Sync`: a camera session must stay on one thread.
pub struct Camera {
    inner: *mut sys::Camera,
}

impl Camera {
    /// List every camera currently attached, without claiming any of them.
    ///
    /// Returns an empty vector if none are connected.
    pub fn autodetect(context: &Context) -> Result<Vec<CameraDescriptor>> {
        let list = List::new()?;
        // Returns the number of cameras (>= 0) or a negative error.
        check(unsafe { sys::gp_camera_autodetect(list.as_ptr(), context.as_ptr()) })?;

        let count = list.count()?;
        let mut cameras = Vec::with_capacity(count);
        for i in 0..count {
            let (model, port) = list.get(i)?;
            cameras.push(CameraDescriptor { model, port });
        }
        Ok(cameras)
    }

    /// Open and initialize the auto-detected camera.
    ///
    /// If exactly one camera is attached, libgphoto2 selects it automatically.
    /// With multiple cameras attached the choice is unspecified; richer
    /// selection (by model/port) will arrive with the port-info API.
    pub fn new(context: &Context) -> Result<Self> {
        let mut inner: *mut sys::Camera = core::ptr::null_mut();
        check(unsafe { sys::gp_camera_new(&mut inner) })?;
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        let camera = Self { inner };
        // `init` connects to the device; on failure `camera`'s Drop unrefs it.
        check(unsafe { sys::gp_camera_init(camera.inner, context.as_ptr()) })?;
        Ok(camera)
    }

    /// Open a specific camera by model name and gphoto2 port path.
    ///
    /// Unlike [`Camera::new`], this targets an exact device — necessary with
    /// multiple cameras attached, and the way to reach virtual cameras such as
    /// libgphoto2's `"Directory Browse"` model on a `"disk:/path"` port.
    pub fn open(context: &Context, model: &str, port: &str) -> Result<Self> {
        let mut inner: *mut sys::Camera = core::ptr::null_mut();
        check(unsafe { sys::gp_camera_new(&mut inner) })?;
        if inner.is_null() {
            return Err(Error::new(sys::GP_ERROR_NO_MEMORY));
        }
        let camera = Self { inner };

        // Resolve model -> abilities and apply them.
        let abilities_list = AbilitiesList::load(context)?;
        let model_index = abilities_list.lookup_model(model)?;
        let abilities = abilities_list.abilities_at(model_index)?;
        check(unsafe { sys::gp_camera_set_abilities(camera.inner, abilities) })?;

        // Resolve port path -> port info and apply it. `port_list` must stay
        // alive until after set_port_info: the GPPortInfo borrows from it.
        let port_list = PortInfoList::load()?;
        let port_index = port_list.lookup_path(port)?;
        let port_info = port_list.info_at(port_index)?;
        check(unsafe { sys::gp_camera_set_port_info(camera.inner, port_info) })?;

        check(unsafe { sys::gp_camera_init(camera.inner, context.as_ptr()) })?;
        Ok(camera)
    }

    /// The abilities (identity + capabilities) of this camera.
    pub fn abilities(&self) -> Result<Abilities> {
        // SAFETY: zero is a valid bit pattern for this POD struct.
        let mut raw: sys::CameraAbilities = unsafe { core::mem::zeroed() };
        check(unsafe { sys::gp_camera_get_abilities(self.inner, &mut raw) })?;
        Ok(Abilities::from_raw(raw))
    }

    /// A driver-provided, human-readable summary of the camera's status.
    pub fn summary(&self, context: &Context) -> Result<String> {
        let mut text = sys::CameraText {
            text: [0; 32 * 1024],
        };
        check(unsafe { sys::gp_camera_get_summary(self.inner, &mut text, context.as_ptr()) })?;
        Ok(fixed_buf_to_string(&text.text))
    }

    /// Trigger a still capture and return where the resulting file lives on the
    /// camera. Use [`Camera::download`] to fetch the bytes.
    pub fn capture_image(&self, context: &Context) -> Result<CameraFilePath> {
        let mut path = sys::CameraFilePath {
            name: [0; 128],
            folder: [0; 1024],
        };
        check(unsafe {
            sys::gp_camera_capture(
                self.inner,
                sys::CameraCaptureType::GP_CAPTURE_IMAGE,
                &mut path,
                context.as_ptr(),
            )
        })?;
        Ok(CameraFilePath {
            folder: fixed_buf_to_string(&path.folder),
            name: fixed_buf_to_string(&path.name),
        })
    }

    /// Trigger a capture but return immediately, without waiting for the file.
    ///
    /// Pair with [`Camera::wait_for_event`] to learn when the resulting file is
    /// ready (the tethered-shooting flow). For a simple blocking capture, use
    /// [`Camera::capture_image`] instead.
    pub fn trigger_capture(&self, context: &Context) -> Result<()> {
        check(unsafe { sys::gp_camera_trigger_capture(self.inner, context.as_ptr()) })?;
        Ok(())
    }

    /// Capture a viewfinder preview frame (liveview) into an in-memory file.
    ///
    /// Returns a small, fast preview — not a full-resolution capture. Call
    /// repeatedly to stream liveview.
    pub fn capture_preview(&self, context: &Context) -> Result<CameraFile> {
        let file = CameraFile::new()?;
        check(unsafe {
            sys::gp_camera_capture_preview(self.inner, file.as_ptr(), context.as_ptr())
        })?;
        Ok(file)
    }

    /// Wait up to `timeout` for the next camera event.
    ///
    /// Returns [`CameraEvent::Timeout`] if nothing happens in time, or
    /// [`ErrorKind::NotSupported`](crate::ErrorKind::NotSupported) for drivers
    /// with no event source.
    pub fn wait_for_event(&self, context: &Context, timeout: Duration) -> Result<CameraEvent> {
        let ms = timeout.as_millis().min(c_int::MAX as u128) as c_int;
        let mut event_type = sys::CameraEventType(0);
        let mut data: *mut c_void = core::ptr::null_mut();
        check(unsafe {
            sys::gp_camera_wait_for_event(
                self.inner,
                ms,
                &mut event_type,
                &mut data,
                context.as_ptr(),
            )
        })?;
        // SAFETY: on success libgphoto2 set `event_type` and, for some events,
        // malloc'd `data` to match it; `interpret_event` owns and frees `data`.
        Ok(unsafe { interpret_event(event_type, data) })
    }

    /// Download the normal (full-resolution) view of a file from the camera.
    pub fn download(&self, context: &Context, path: &CameraFilePath) -> Result<CameraFile> {
        self.download_typed(context, path, FileType::Normal)
    }

    /// Download a specific [`FileType`] view of a file (e.g. a thumbnail
    /// [`FileType::Preview`], the [`FileType::Exif`] block, or [`FileType::Raw`]).
    pub fn download_typed(
        &self,
        context: &Context,
        path: &CameraFilePath,
        file_type: FileType,
    ) -> Result<CameraFile> {
        let folder = to_cstring(&path.folder)?;
        let name = to_cstring(&path.name)?;
        let file = CameraFile::new()?;
        check(unsafe {
            sys::gp_camera_file_get(
                self.inner,
                folder.as_ptr(),
                name.as_ptr(),
                file_type.to_raw(),
                file.as_ptr(),
                context.as_ptr(),
            )
        })?;
        Ok(file)
    }

    /// Metadata (size, MIME type, dimensions, mtime, permissions) for a file,
    /// without downloading it. Fields are present only if the camera reports them.
    pub fn file_info(&self, context: &Context, folder: &str, name: &str) -> Result<FileInfo> {
        let cfolder = to_cstring(folder)?;
        let cname = to_cstring(name)?;
        // SAFETY: CameraFileInfo is plain data; zero-init then libgphoto2 fills it.
        let mut info: sys::CameraFileInfo = unsafe { core::mem::zeroed() };
        check(unsafe {
            sys::gp_camera_file_get_info(
                self.inner,
                cfolder.as_ptr(),
                cname.as_ptr(),
                &mut info,
                context.as_ptr(),
            )
        })?;
        Ok(FileInfo::from_raw(&info))
    }

    /// Delete a single file from the camera.
    pub fn delete_file(&self, context: &Context, folder: &str, name: &str) -> Result<()> {
        let cfolder = to_cstring(folder)?;
        let cname = to_cstring(name)?;
        check(unsafe {
            sys::gp_camera_file_delete(
                self.inner,
                cfolder.as_ptr(),
                cname.as_ptr(),
                context.as_ptr(),
            )
        })?;
        Ok(())
    }

    /// Upload a file into `folder` on the camera. The target filename is the
    /// file's own [`name`](CameraFile::name) (set via
    /// [`CameraFile::from_bytes`]).
    pub fn upload(&self, context: &Context, folder: &str, file: &CameraFile) -> Result<()> {
        let cfolder = to_cstring(folder)?;
        let cname = to_cstring(&file.name()?)?;
        check(unsafe {
            sys::gp_camera_folder_put_file(
                self.inner,
                cfolder.as_ptr(),
                cname.as_ptr(),
                FileType::Normal.to_raw(),
                file.as_ptr(),
                context.as_ptr(),
            )
        })?;
        Ok(())
    }

    /// Create a sub-directory `name` inside `folder`.
    pub fn make_dir(&self, context: &Context, folder: &str, name: &str) -> Result<()> {
        let cfolder = to_cstring(folder)?;
        let cname = to_cstring(name)?;
        check(unsafe {
            sys::gp_camera_folder_make_dir(
                self.inner,
                cfolder.as_ptr(),
                cname.as_ptr(),
                context.as_ptr(),
            )
        })?;
        Ok(())
    }

    /// Remove the sub-directory `name` inside `folder`.
    pub fn remove_dir(&self, context: &Context, folder: &str, name: &str) -> Result<()> {
        let cfolder = to_cstring(folder)?;
        let cname = to_cstring(name)?;
        check(unsafe {
            sys::gp_camera_folder_remove_dir(
                self.inner,
                cfolder.as_ptr(),
                cname.as_ptr(),
                context.as_ptr(),
            )
        })?;
        Ok(())
    }

    /// Delete all files in `folder` (non-recursively).
    pub fn delete_all(&self, context: &Context, folder: &str) -> Result<()> {
        let cfolder = to_cstring(folder)?;
        check(unsafe {
            sys::gp_camera_folder_delete_all(self.inner, cfolder.as_ptr(), context.as_ptr())
        })?;
        Ok(())
    }

    /// Every file under `folder`, recursing into sub-folders (depth-first).
    pub fn list_files_recursive(
        &self,
        context: &Context,
        folder: &str,
    ) -> Result<Vec<CameraFilePath>> {
        let mut out = Vec::new();
        self.walk(context, folder, &mut out)?;
        Ok(out)
    }

    fn walk(&self, context: &Context, folder: &str, out: &mut Vec<CameraFilePath>) -> Result<()> {
        for name in self.list_files(context, folder)? {
            out.push(CameraFilePath {
                folder: folder.to_owned(),
                name,
            });
        }
        for sub in self.list_folders(context, folder)? {
            let child = if folder.ends_with('/') {
                format!("{folder}{sub}")
            } else {
                format!("{folder}/{sub}")
            };
            self.walk(context, &child, out)?;
        }
        Ok(())
    }

    /// A driver-provided "about" text (authors, credits, notes).
    pub fn about(&self, context: &Context) -> Result<String> {
        let mut text = sys::CameraText {
            text: [0; 32 * 1024],
        };
        check(unsafe { sys::gp_camera_get_about(self.inner, &mut text, context.as_ptr()) })?;
        Ok(fixed_buf_to_string(&text.text))
    }

    /// A driver-provided manual / usage notes text.
    pub fn manual(&self, context: &Context) -> Result<String> {
        let mut text = sys::CameraText {
            text: [0; 32 * 1024],
        };
        check(unsafe { sys::gp_camera_get_manual(self.inner, &mut text, context.as_ptr()) })?;
        Ok(fixed_buf_to_string(&text.text))
    }

    /// Information about the camera's storages (capacity, free space, type).
    pub fn storage_info(&self, context: &Context) -> Result<Vec<StorageInfo>> {
        let mut ptr: *mut sys::CameraStorageInformation = core::ptr::null_mut();
        let mut count: c_int = 0;
        check(unsafe {
            sys::gp_camera_get_storageinfo(self.inner, &mut ptr, &mut count, context.as_ptr())
        })?;
        if ptr.is_null() || count <= 0 {
            return Ok(Vec::new());
        }
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            // SAFETY: libgphoto2 returned an array of `count` valid structs.
            out.push(StorageInfo::from_raw(unsafe { &*ptr.add(i) }));
        }
        // SAFETY: the array was malloc'd by libgphoto2 for the caller to free.
        unsafe { libc_free(ptr.cast::<c_void>()) };
        Ok(out)
    }

    /// List the sub-folders of `folder` (an absolute camera path, e.g. `"/"`).
    pub fn list_folders(&self, context: &Context, folder: &str) -> Result<Vec<String>> {
        let folder = to_cstring(folder)?;
        let list = List::new()?;
        check(unsafe {
            sys::gp_camera_folder_list_folders(
                self.inner,
                folder.as_ptr(),
                list.as_ptr(),
                context.as_ptr(),
            )
        })?;
        list.names()
    }

    /// List the files in `folder` (an absolute camera path, e.g. `"/"`).
    pub fn list_files(&self, context: &Context, folder: &str) -> Result<Vec<String>> {
        let folder = to_cstring(folder)?;
        let list = List::new()?;
        check(unsafe {
            sys::gp_camera_folder_list_files(
                self.inner,
                folder.as_ptr(),
                list.as_ptr(),
                context.as_ptr(),
            )
        })?;
        list.names()
    }

    /// Read the camera's full configuration tree (settings: ISO, aperture,
    /// capture target, date/time, …). See [`Widget`].
    pub fn config(&self, context: &Context) -> Result<Widget> {
        let mut root: *mut sys::CameraWidget = core::ptr::null_mut();
        check(unsafe { sys::gp_camera_get_config(self.inner, &mut root, context.as_ptr()) })?;
        Ok(Widget::from_root(root))
    }

    /// Apply a (modified) configuration tree to the camera.
    ///
    /// Accepts any handle from the tree returned by [`Camera::config`]; the
    /// whole tree is applied.
    pub fn set_config(&self, context: &Context, config: &Widget) -> Result<()> {
        check(unsafe {
            sys::gp_camera_set_config(self.inner, config.root_ptr(), context.as_ptr())
        })?;
        Ok(())
    }

    /// Read a single named configuration widget — cheaper than fetching the
    /// whole tree when you only need one setting.
    pub fn get_single_config(&self, context: &Context, name: &str) -> Result<Widget> {
        let cname = to_cstring(name)?;
        let mut w: *mut sys::CameraWidget = core::ptr::null_mut();
        check(unsafe {
            sys::gp_camera_get_single_config(self.inner, cname.as_ptr(), &mut w, context.as_ptr())
        })?;
        Ok(Widget::from_root(w))
    }

    /// Apply a single named configuration widget.
    pub fn set_single_config(&self, context: &Context, name: &str, config: &Widget) -> Result<()> {
        let cname = to_cstring(name)?;
        check(unsafe {
            sys::gp_camera_set_single_config(
                self.inner,
                cname.as_ptr(),
                config.as_ptr(),
                context.as_ptr(),
            )
        })?;
        Ok(())
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        // Unref releases our reference; when the count hits zero libgphoto2
        // runs the driver's exit handler and frees the camera.
        unsafe { sys::gp_camera_unref(self.inner) };
    }
}

fn to_cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| Error::new(sys::GP_ERROR_BAD_PARAMETERS))
}
