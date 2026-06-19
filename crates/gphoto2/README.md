# gphoto2

Safe, idiomatic Rust bindings to [libgphoto2](http://www.gphoto.org/) — detect,
capture from, and download files off thousands of digital cameras.

The safe layer of [`vibe-libgphoto2-rs`](https://github.com/SauceTaster/vibe-libgphoto2-rs),
built on the raw FFI in [`gphoto2-sys`](../gphoto2-sys). Every resource
(context, camera, file) is an RAII type that frees itself on drop, and every
fallible call returns a `Result` carrying libgphoto2's structured `ErrorKind`.

```rust
use gphoto2::{Camera, Context};

let context = Context::new()?;

for cam in Camera::autodetect(&context)? {
    println!("{} @ {}", cam.model, cam.port);
}

let camera = Camera::new(&context)?;
let path = camera.capture_image(&context)?;
camera.download(&context, &path)?.save(&path.name)?;
# Ok::<(), gphoto2::Error>(())
```

```console
cargo run -p gphoto2 --example detect
```

## Requirements

libgphoto2 must be installed (e.g. `brew install libgphoto2`,
`apt install libgphoto2-dev`). See the
[workspace README](../../README.md) for platform notes and build configuration.

## Threading

libgphoto2 is not thread-safe across a camera session, so `Context` and `Camera`
are intentionally neither `Send` nor `Sync`.

## License

Dual-licensed **MIT OR Apache-2.0**. Linking libgphoto2 (LGPL-2.1-or-later)
imposes LGPL obligations on the *distributed binary* — see the
[workspace README](../../README.md#licensing).
