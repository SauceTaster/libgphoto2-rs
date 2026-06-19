//! Tethered capture: trigger a shot, wait for the file-added event, download it.
//!
//! Run with: `cargo run -p gphoto2 --example tether`

use std::time::Duration;

use gphoto2::{Camera, CameraEvent, Context};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::new()?;

    let camera = match Camera::new(&context) {
        Ok(camera) => camera,
        Err(e) => {
            println!("No camera available ({e}).");
            return Ok(());
        }
    };

    camera.trigger_capture(&context)?;
    println!("Capture triggered; waiting for the new file…");

    loop {
        match camera.wait_for_event(&context, Duration::from_secs(5))? {
            CameraEvent::FileAdded(path) => {
                println!("New file: {}/{}", path.folder, path.name);
                let file = camera.download(&context, &path)?;
                file.save(&path.name)?;
                println!("Saved {} ({} bytes).", path.name, file.data()?.len());
                break;
            }
            CameraEvent::CaptureComplete => println!("(capture complete)"),
            CameraEvent::Timeout => {
                println!("Timed out waiting for the file.");
                break;
            }
            other => println!("(event: {other:?})"),
        }
    }

    Ok(())
}
