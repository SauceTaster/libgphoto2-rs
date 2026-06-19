//! End-to-end tests against libgphoto2's hardware-free **"Directory Browse"**
//! virtual camera (the `disk:` port) — the same mechanism libgphoto2's own CLI
//! test suite uses. These exercise the full stack through the FFI: model/port
//! resolution, `gp_camera_init`, folder/file listing, and a download
//! round-trip.
//!
//! Requires the `directory` camlib and `disk` iolib, which ship with a normal
//! libgphoto2 install. If they're absent the first test reports that clearly.

use std::cell::{Cell, RefCell};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use gphoto2::{
    supported_models, AccessType, Camera, CameraFile, CameraFilePath, Context, ErrorKind,
    ProgressHandler,
};

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
fn wait_for_event_is_unsupported_on_directory_camera() {
    let _g = lock();
    let tmp = TempDir::new("events");
    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    // The directory driver has no event source, so the core returns
    // GP_ERROR_NOT_SUPPORTED — a deterministic exercise of the wait_for_event
    // FFI path and error mapping on a real camera handle.
    match camera.wait_for_event(&ctx, std::time::Duration::from_millis(10)) {
        Err(e) => assert_eq!(e.kind(), gphoto2::ErrorKind::NotSupported),
        Ok(other) => panic!("expected NotSupported, got {other:?}"),
    }
}

#[test]
fn cancel_handler_aborts_a_listing() {
    let _g = lock();
    let tmp = TempDir::new("cancel");
    fs::write(tmp.path().join("a.jpg"), b"a").unwrap();
    fs::write(tmp.path().join("b.jpg"), b"b").unwrap();

    let mut ctx = Context::new().unwrap();
    // Open first — a cancel handler that always fires would otherwise abort init.
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    let polls = Rc::new(Cell::new(0u32));
    {
        let polls = polls.clone();
        ctx.set_cancel_handler(move || {
            polls.set(polls.get() + 1);
            true
        });
    }

    // The directory driver polls cancel while listing; returning true aborts it.
    match camera.list_files(&ctx, "/") {
        Err(e) => assert_eq!(e.kind(), ErrorKind::Cancel),
        Ok(files) => panic!("expected cancellation, got {files:?}"),
    }
    assert!(polls.get() >= 1, "cancel handler should have been polled");
}

#[derive(Default)]
struct ProgressLog {
    started: bool,
    updates: u32,
    stopped: bool,
}

struct RecordingProgress {
    log: Rc<RefCell<ProgressLog>>,
}

impl ProgressHandler for RecordingProgress {
    fn start(&mut self, _target: f32, _message: &str) -> u32 {
        self.log.borrow_mut().started = true;
        1
    }
    fn update(&mut self, _id: u32, _current: f32) {
        self.log.borrow_mut().updates += 1;
    }
    fn stop(&mut self, _id: u32) {
        self.log.borrow_mut().stopped = true;
    }
}

#[test]
fn progress_handler_observes_a_listing() {
    let _g = lock();
    let tmp = TempDir::new("progress");
    for i in 0..3 {
        fs::write(tmp.path().join(format!("f{i}.jpg")), b"x").unwrap();
    }

    let mut ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    let log = Rc::new(RefCell::new(ProgressLog::default()));
    ctx.set_progress_handler(RecordingProgress { log: log.clone() });

    let _ = camera.list_files(&ctx, "/").unwrap();

    let log = log.borrow();
    assert!(log.started, "progress start should have fired");
    assert!(log.stopped, "progress stop should have fired");
}

#[test]
fn file_info_reports_size_and_mime() {
    let _g = lock();
    let tmp = TempDir::new("info");
    let bytes = b"pretend jpeg bytes, only the extension matters here";
    fs::write(tmp.path().join("photo.jpg"), bytes).unwrap();

    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");
    let info = camera.file_info(&ctx, "/", "photo.jpg").unwrap();

    assert_eq!(info.size, Some(bytes.len() as u64));
    assert!(
        info.mime_type.as_deref().unwrap_or("").contains("jpeg"),
        "mime: {:?}",
        info.mime_type
    );
}

#[test]
fn upload_then_download_round_trip() {
    let _g = lock();
    let tmp = TempDir::new("upload");
    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    let payload = b"freshly uploaded contents";
    let file = CameraFile::from_bytes("up.jpg", "image/jpeg", payload).unwrap();
    camera.upload(&ctx, "/", &file).unwrap();

    // Independent of libgphoto2's cache: the bytes really landed on disk.
    assert_eq!(fs::read(tmp.path().join("up.jpg")).unwrap(), payload);

    // And the camera can read them back.
    let path = CameraFilePath {
        folder: "/".into(),
        name: "up.jpg".into(),
    };
    let downloaded = camera.download(&ctx, &path).unwrap();
    assert_eq!(downloaded.data().unwrap(), payload);
}

#[test]
fn delete_file_removes_it() {
    let _g = lock();
    let tmp = TempDir::new("delete");
    fs::write(tmp.path().join("doomed.jpg"), b"x").unwrap();

    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");
    assert!(camera
        .list_files(&ctx, "/")
        .unwrap()
        .iter()
        .any(|f| f == "doomed.jpg"));

    camera.delete_file(&ctx, "/", "doomed.jpg").unwrap();
    assert!(!tmp.path().join("doomed.jpg").exists());
}

#[test]
fn make_and_remove_dir() {
    let _g = lock();
    let tmp = TempDir::new("dirs");
    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    camera.make_dir(&ctx, "/", "newdir").unwrap();
    assert!(tmp.path().join("newdir").is_dir());

    camera.remove_dir(&ctx, "/", "newdir").unwrap();
    assert!(!tmp.path().join("newdir").exists());
}

#[test]
fn storage_info_describes_the_directory() {
    let _g = lock();
    let tmp = TempDir::new("storage");
    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    let storages = camera.storage_info(&ctx).unwrap();
    assert!(!storages.is_empty(), "directory driver reports one storage");
    assert_eq!(storages[0].base_dir.as_deref(), Some("/"));
    assert_eq!(storages[0].access, Some(AccessType::ReadWrite));
}

#[test]
fn about_and_manual_are_available() {
    let _g = lock();
    let tmp = TempDir::new("about");
    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");

    assert!(!camera.about(&ctx).unwrap().is_empty());
    assert!(!camera.manual(&ctx).unwrap().is_empty());
}

#[test]
fn recursive_listing_finds_nested_files() {
    let _g = lock();
    let tmp = TempDir::new("walk");
    fs::write(tmp.path().join("top.jpg"), b"a").unwrap();
    fs::create_dir(tmp.path().join("sub")).unwrap();
    fs::write(tmp.path().join("sub").join("inner.jpg"), b"b").unwrap();

    let ctx = Context::new().unwrap();
    let camera = open_directory_camera(&ctx, tmp.path()).expect("open directory camera");
    let all = camera.list_files_recursive(&ctx, "/").unwrap();

    let paths: Vec<String> = all
        .iter()
        .map(|p| format!("{}/{}", p.folder.trim_end_matches('/'), p.name))
        .collect();
    assert!(paths.iter().any(|p| p == "/top.jpg"), "{paths:?}");
    assert!(paths.iter().any(|p| p == "/sub/inner.jpg"), "{paths:?}");
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
