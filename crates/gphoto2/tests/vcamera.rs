//! End-to-end tests against libgphoto2's built-in **virtual PTP camera** (the
//! `vusb` I/O driver + `ptp2` camlib). Unlike the directory camera, this is a
//! real PTP device, so it exercises capture, the config (widget) tree, and
//! events — the paths that matter most and are otherwise impossible to cover
//! without hardware.
//!
//! These require a libgphoto2 built with `--enable-vusb` and are gated behind
//! the `GPHOTO2_VUSB_TESTS` env var (set by the vcamera CI lane and
//! `scripts/run-vcamera-tests.sh`). Without it they skip, so the normal test
//! run against a stock libgphoto2 stays green.

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gphoto2::{Camera, Context, WidgetType};

// The vcamera registers itself as this model on this fixed port.
const VCAMERA_MODEL: &str = "Nikon DSC D750";
const VCAMERA_PORT: &str = "usb:001,001";

static GP_LOCK: Mutex<()> = Mutex::new(());

fn lock() -> std::sync::MutexGuard<'static, ()> {
    GP_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a vusb-enabled libgphoto2 is active (the lane sets this).
fn vusb_enabled() -> bool {
    std::env::var_os("GPHOTO2_VUSB_TESTS").is_some()
}

/// Point `VCAMERADIR` at a fresh dir holding one capture-able `.jpg` (the
/// vcamera duplicates it on capture). Returns the dir.
fn setup_vcameradir(tag: &str, payload: &[u8]) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "gphoto2-rs-vcam-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("source.jpg"), payload).unwrap();
    // Read by the vcamera at camera init; safe under GP_LOCK (serialized).
    std::env::set_var("VCAMERADIR", &dir);
    dir
}

fn open_vcamera(ctx: &Context) -> gphoto2::Result<Camera> {
    Camera::open(ctx, VCAMERA_MODEL, VCAMERA_PORT)
}

#[test]
fn vcamera_is_detected() {
    if !vusb_enabled() {
        eprintln!("skipping: GPHOTO2_VUSB_TESTS not set (needs a --enable-vusb build)");
        return;
    }
    let _g = lock();
    let ctx = Context::new().unwrap();
    let cameras = Camera::autodetect(&ctx).unwrap();
    assert!(
        cameras
            .iter()
            .any(|c| c.model == VCAMERA_MODEL && c.port == VCAMERA_PORT),
        "vcamera not autodetected: {cameras:?}"
    );
}

#[test]
fn vcamera_reports_capture_and_config_capabilities() {
    if !vusb_enabled() {
        return;
    }
    let _g = lock();
    let _dir = setup_vcameradir("caps", b"x");
    let ctx = Context::new().unwrap();
    let camera = open_vcamera(&ctx).expect("open vcamera");
    let ops = camera.abilities().unwrap().camera_operations();
    assert!(
        ops.capture_image,
        "a PTP camera should support image capture"
    );
    assert!(ops.config, "a PTP camera should expose configuration");
}

#[test]
fn vcamera_exposes_a_config_tree() {
    if !vusb_enabled() {
        return;
    }
    let _g = lock();
    let _dir = setup_vcameradir("config", b"x");
    let ctx = Context::new().unwrap();
    let camera = open_vcamera(&ctx).expect("open vcamera");
    let config = camera.config(&ctx).expect("read config");
    assert_eq!(config.widget_type().unwrap(), WidgetType::Window);
    assert!(
        config.child_count().unwrap() > 0,
        "the PTP config tree should have children"
    );
}

#[test]
fn vcamera_capture_then_download_round_trips() {
    if !vusb_enabled() {
        return;
    }
    let _g = lock();
    let payload = b"vibe-libgphoto2-rs vcamera capture payload";
    let _dir = setup_vcameradir("capture", payload);
    let ctx = Context::new().unwrap();
    let camera = open_vcamera(&ctx).expect("open vcamera");

    // Capture duplicates the source file; downloading it yields the same bytes.
    let path = camera.capture_image(&ctx).expect("capture image");
    let file = camera
        .download(&ctx, &path)
        .expect("download captured image");
    assert_eq!(file.data().unwrap(), payload);
}

#[test]
fn vcamera_supports_events() {
    if !vusb_enabled() {
        return;
    }
    let _g = lock();
    let _dir = setup_vcameradir("events", b"x");
    let ctx = Context::new().unwrap();
    let camera = open_vcamera(&ctx).expect("open vcamera");

    // Contrast with the directory camera, which returns NotSupported: a PTP
    // device has an event source, so waiting succeeds (Timeout or an event).
    let result = camera.wait_for_event(&ctx, Duration::from_millis(50));
    assert!(
        result.is_ok(),
        "wait_for_event should be supported: {result:?}"
    );
}
