//! List the linked libgphoto2 version and any attached cameras.
//!
//! Run with: `cargo run -p gphoto2-bindings --example detect`

use gphoto2::{library_version, Camera, Context};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Linked libgphoto2:");
    for line in library_version() {
        println!("  {line}");
    }
    println!();

    let context = Context::new()?;
    let cameras = Camera::autodetect(&context)?;

    if cameras.is_empty() {
        println!("No cameras detected. Connect one (and ensure no other app, e.g. macOS Photos, has claimed it).");
        return Ok(());
    }

    println!("Detected {} camera(s):", cameras.len());
    for cam in &cameras {
        println!("  - {} @ {}", cam.model, cam.port);
    }

    Ok(())
}
