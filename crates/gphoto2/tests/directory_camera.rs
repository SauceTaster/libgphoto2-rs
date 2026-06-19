//! End-to-end tests against libgphoto2's hardware-free **"Directory Browse"**
//! virtual camera (the `disk:` port) — the same mechanism libgphoto2's own CLI
//! test suite uses. These exercise the full stack through the FFI: model/port
//! resolution, `gp_camera_init`, folder/file listing, and a download
//! round-trip.
//!
//! Requires the `directory` camlib and `disk` iolib, which ship with a normal
//! libgphoto2 install. If they're absent the first test reports that clearly.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use gphoto2::{supported_models, Camera, CameraFilePath, Context, ErrorKind};

/// libgphoto2 loads drivers via global ltdl/dlopen state; serialise the tests
/// that spin up cameras so parallel runs can't race on it.
static GP_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    // Ignore poisoning: a panic in one test shouldn't cascade-fail the rest.
    GP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Minimal self-cleaning temp directory (no external crate).
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("gphoto2-rs-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn open_directory_camera(ctx: &Context, dir: &Path) -> gphoto2::Result<Camera> {
    Camera::open(ctx, "Directory Browse", &format!("disk:{}", dir.display()))
}

#[test]
fn directory_browse_driver_is_installed() {
    let _g = lock();
    let ctx = Context::new().unwrap();
    let models = supported_models(&ctx).unwrap();
    assert!(
        models.iter().any(|m| m == "Directory Browse"),
        "the `directory` camlib must be installed for the camera tests; \
         found {} models",
        models.len()
    );
}

#[test]
fn open_list_download_round_trip() {
    let _g = lock();
    let tmp = TempDir::new("browse");

    // The directory camlib only *lists* files whose extension is a recognised
    // media type (jpg, tif, mov, …); the bytes themselves are arbitrary. So we
    // use a `.jpg` name with non-image content.
    let payload = b"vibe-libgphoto2-rs directory-camera round-trip payload";
    fs::write(tmp.path().join("photo.jpg"), payload).unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();

    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    // The camera reports the directory driver's identity.
    assert_eq!(camera.abilities().unwrap().model(), "Directory Browse");

    // Folder + file listing of the root.
    let folders = camera.list_folders(&ctx, "/").unwrap();
    assert!(folders.iter().any(|f| f == "sub"), "folders: {folders:?}");

    let files = camera.list_files(&ctx, "/").unwrap();
    assert!(files.iter().any(|f| f == "photo.jpg"), "files: {files:?}");

    // Download round-trip: the bytes must match what we wrote.
    let path = CameraFilePath {
        folder: "/".to_string(),
        name: "photo.jpg".to_string(),
    };
    let file = camera.download(&ctx, &path).unwrap();
    assert_eq!(file.data().unwrap(), payload);

    // Saving to disk yields identical bytes too.
    let out = tmp.path().join("downloaded.bin");
    file.save(&out).unwrap();
    assert_eq!(fs::read(&out).unwrap(), payload);
}

#[test]
fn list_files_in_subfolder() {
    let _g = lock();
    let tmp = TempDir::new("subfolder");
    fs::create_dir(tmp.path().join("dcim")).unwrap();
    // Recognised media extensions so the directory camlib lists them.
    fs::write(tmp.path().join("dcim").join("a.jpg"), b"a").unwrap();
    fs::write(tmp.path().join("dcim").join("b.jpg"), b"b").unwrap();

    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    let mut files = camera.list_files(&ctx, "/dcim").unwrap();
    files.sort();
    assert_eq!(files, vec!["a.jpg".to_string(), "b.jpg".to_string()]);
}

#[test]
fn opening_unknown_model_is_model_not_found() {
    let _g = lock();
    let ctx = Context::new().unwrap();
    // `Camera` is intentionally not `Debug`, so match rather than `expect_err`.
    match Camera::open(&ctx, "Definitely Not A Real Camera 9000", "disk:/tmp") {
        Ok(_) => panic!("opening an unknown model should fail"),
        Err(e) => assert_eq!(e.kind(), ErrorKind::ModelNotFound),
    }
}
