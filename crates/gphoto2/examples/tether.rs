//! Capture and download every reported file, including RAW + JPEG pairs.
//!
//! Run with: `cargo run -p gphoto2-bindings --example tether -- [port] [output-directory]`
//!
//! With one camera, both arguments are optional. With several cameras, provide
//! an exact port from the list printed by this example. Files remain on the
//! camera, and existing local files are never overwritten.
//!
//! Collection ends after 30 seconds, or after two quiet seconds following a
//! file or capture-complete event. That settle period is a heuristic: some
//! cameras report additional files later. These limits bound event polling;
//! an individual libgphoto2 capture or download call may block longer.

use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use gphoto2::{Camera, CameraEvent, CameraFilePath, Context, ErrorKind};

const MAX_EVENT_WAIT: Duration = Duration::from_secs(30);
const SETTLE_TIME: Duration = Duration::from_secs(2);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let requested_port = args.next();
    let output_dir = args.next().map(PathBuf::from).unwrap_or_else(|| ".".into());
    if args.next().is_some() {
        return Err("usage: tether [port] [output-directory]".into());
    }

    let context = Context::new()?;
    let cameras = Camera::autodetect(&context)?;
    for camera in &cameras {
        println!("{} @ {}", camera.model, camera.port);
    }

    let descriptor = match requested_port {
        Some(port) => cameras
            .iter()
            .find(|camera| Some(camera.port.as_str()) == port.to_str())
            .ok_or("the requested camera port was not detected")?,
        None if cameras.len() == 1 => &cameras[0],
        None if cameras.is_empty() => return Err("no camera detected".into()),
        None => return Err("several cameras detected; pass the desired port".into()),
    };
    let camera = Camera::open(&context, &descriptor.model, &descriptor.port)?;
    std::fs::create_dir_all(&output_dir)?;

    let mut paths = Vec::new();
    match camera.trigger_capture(&context) {
        Ok(()) => println!("Capture triggered; collecting file events..."),
        Err(error) if error.kind() == ErrorKind::NotSupported => {
            // A blocking capture gives one path; still collect events for any
            // additional files. The returned path may also appear as an event.
            paths.push(camera.capture_image(&context)?);
        }
        Err(error) => return Err(error.into()),
    }

    collect_files(&camera, &context, &mut paths)?;
    if paths.is_empty() {
        return Err(io::Error::new(io::ErrorKind::TimedOut, "no capture file was reported").into());
    }

    for path in paths {
        let file = camera.download(&context, &path)?;
        let data = file.data()?;
        let destination = save_new(&output_dir, &path.name, data)?;
        println!("Saved {} ({} bytes).", destination.display(), data.len());
    }
    Ok(())
}

fn collect_files(
    camera: &Camera,
    context: &Context,
    paths: &mut Vec<CameraFilePath>,
) -> gphoto2::Result<()> {
    let deadline = Instant::now() + MAX_EVENT_WAIT;
    let mut last_activity = if paths.is_empty() {
        None
    } else {
        Some(Instant::now())
    };
    loop {
        let now = Instant::now();
        let remaining = deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            eprintln!("Event collection reached its time limit.");
            break;
        }
        let quiet_remaining = last_activity
            .map(|last| SETTLE_TIME.saturating_sub(now.duration_since(last)))
            .unwrap_or(remaining);
        if !paths.is_empty() && quiet_remaining.is_zero() {
            break;
        }
        let timeout = remaining
            .min(quiet_remaining)
            .min(Duration::from_millis(250));
        let event = match camera.wait_for_event(context, timeout) {
            Ok(event) => event,
            Err(error) if error.kind() == ErrorKind::NotSupported && !paths.is_empty() => {
                eprintln!("This driver has no event source; saving the blocking capture result.");
                break;
            }
            Err(error) => return Err(error),
        };
        match event {
            CameraEvent::FileAdded(path) => {
                // Compare the full camera path: equal filenames in separate
                // folders are distinct files. Repeated events are harmless.
                if !paths.contains(&path) {
                    println!("New file: {}/{}", path.folder, path.name);
                    paths.push(path);
                    last_activity = Some(Instant::now());
                }
            }
            CameraEvent::CaptureComplete => {
                // Completion is not necessarily the last event for RAW+JPEG.
                // Keep listening briefly rather than stopping at this marker.
                if !paths.is_empty() {
                    last_activity = Some(Instant::now());
                }
            }
            CameraEvent::Timeout => {}
            other => println!("(event: {other:?})"),
        }
    }
    Ok(())
}

fn save_new(output_dir: &Path, name: &str, data: &[u8]) -> io::Result<PathBuf> {
    let mut components = Path::new(name).components();
    if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid camera filename",
        ));
    }
    // Prefix colliding names, preserving both captures and the file extension.
    for suffix in 0u64.. {
        let name = if suffix == 0 {
            name.to_owned()
        } else {
            format!("{suffix}-{name}")
        };
        let destination = output_dir.join(name);
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
        {
            Ok(mut output) => {
                output.write_all(data)?;
                return Ok(destination);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    unreachable!("exhausted all filename suffixes")
}
