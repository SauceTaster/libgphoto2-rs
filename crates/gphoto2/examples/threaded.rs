//! Share a camera session between application threads without moving its C objects.
//!
//! Run with: `cargo run -p gphoto2-bindings --example threaded`
//!
//! `CameraThread::spawn`, `run`, and dropping the final handle can block. In an
//! async application, call them from a blocking task rather than an executor
//! thread. Move an `Arc<CameraThread>` into that task; create the camera and
//! context in the `spawn` initializer, and return owned data from `run`.

use std::sync::Arc;

use gphoto2::{Camera, CameraThread, Context, Widget, WidgetType, WidgetValue};

// This snapshot contains only owned Rust data, so it can leave the camera thread.
struct SettingSnapshot {
    name: String,
    label: String,
    kind: WidgetType,
    value: WidgetValue,
    readonly: bool,
    choices: Vec<String>,
    range: Option<(f32, f32, f32)>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let session = Arc::new(CameraThread::spawn(|| {
        let context = Context::new()?;
        let camera = Camera::new(&context)?;
        Ok((context, camera))
    })?);

    // A caller thread may request work while another caller uses the handle.
    // CameraThread serializes both operations on the session's own thread.
    let preview_session = Arc::clone(&session);
    let preview_request = std::thread::spawn(move || {
        preview_session.run(|camera, context| {
            let file = camera.capture_preview(context)?;
            // CameraFile and its borrowed byte slice stay on this thread.
            // Copy the bytes before the file drops and return the owned Vec.
            Ok::<_, gphoto2::Error>(file.data()?.to_vec())
        })
    });

    let settings = session.run(|camera, context| {
        let root = camera.config(context)?;
        let mut settings = Vec::new();
        collect_settings(&root, &mut settings)?;
        Ok::<_, gphoto2::Error>(settings)
    });

    // Join before propagating an error, so the caller task always finishes.
    let preview = preview_request
        .join()
        .map_err(|_| "preview request thread panicked")?;

    // The outer Result reports dispatch failure; the inner Result reports the
    // camera operation's error. Keeping both makes that distinction explicit.
    for setting in settings?? {
        println!(
            "{} ({}, {:?}): {:?}; readonly={}; choices={:?}; range={:?}",
            setting.name,
            setting.label,
            setting.kind,
            setting.value,
            setting.readonly,
            setting.choices,
            setting.range
        );
    }
    println!("Preview contains {} bytes.", preview??.len());

    // All caller handles have gone away. This closes the queue and joins the
    // worker, releasing the camera and context on their original thread.
    drop(session);
    Ok(())
}

fn collect_settings(widget: &Widget, settings: &mut Vec<SettingSnapshot>) -> gphoto2::Result<()> {
    let kind = widget.widget_type()?;
    if !matches!(kind, WidgetType::Window | WidgetType::Section) {
        settings.push(SettingSnapshot {
            name: widget.name()?,
            label: widget.label()?,
            kind,
            value: widget.value()?,
            readonly: widget.readonly()?,
            choices: if matches!(kind, WidgetType::Radio | WidgetType::Menu) {
                widget.choices()?
            } else {
                Vec::new()
            },
            range: if kind == WidgetType::Range {
                Some(widget.range()?)
            } else {
                None
            },
        });
    }
    for child in widget.children()? {
        collect_settings(&child, settings)?;
    }
    Ok(())
}
