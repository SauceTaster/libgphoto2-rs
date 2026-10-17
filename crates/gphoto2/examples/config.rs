//! Print the configuration tree of the attached camera, and demonstrate
//! reading a single setting.
//!
//! Run with: `cargo run -p gphoto2-bindings --example config`

use gphoto2::widget::{Widget, WidgetType};
use gphoto2::{Camera, Context};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::new()?;

    let camera = match Camera::new(&context) {
        Ok(camera) => camera,
        Err(e) => {
            println!("No camera available ({e}).");
            return Ok(());
        }
    };

    println!("Configuration tree:");
    let config = camera.config(&context)?;
    print_tree(&config, 1)?;

    // Single-setting access (common widgets; ignore if a model lacks them).
    for name in ["iso", "shutterspeed", "aperture", "capturetarget"] {
        if let Ok(widget) = camera.get_single_config(&context, name) {
            println!("\n{name} = {:?}", widget.value()?);
            if matches!(widget.widget_type()?, WidgetType::Radio | WidgetType::Menu) {
                println!("  choices: {:?}", widget.choices()?);
            }
        }
    }

    Ok(())
}

fn print_tree(widget: &Widget, depth: usize) -> gphoto2::Result<()> {
    let indent = "  ".repeat(depth);
    let name = widget.name()?;
    let ty = widget.widget_type()?;

    match ty {
        WidgetType::Window | WidgetType::Section => println!("{indent}[{name}] ({ty:?})"),
        _ => println!("{indent}{name} ({ty:?}) = {:?}", widget.value()?),
    }

    for child in widget.children()? {
        print_tree(&child, depth + 1)?;
    }
    Ok(())
}
