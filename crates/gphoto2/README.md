# gphoto2-bindings

Safe, idiomatic Rust bindings to [libgphoto2](http://www.gphoto.org/) — detect,
capture from, and download files off thousands of digital cameras.

The safe layer of [`libgphoto2-rs`](https://github.com/SauceTaster/libgphoto2-rs),
built on the raw FFI in [`gphoto2-bindings-sys`](https://crates.io/crates/gphoto2-bindings-sys). Every resource
(context, camera, file) is an RAII type that frees itself on drop, and every
fallible call returns a `Result` carrying libgphoto2's structured `ErrorKind`.

Requires Rust **1.82+**. For the prepared 0.1.0 release:

```toml
[dependencies]
gphoto2 = { package = "gphoto2-bindings", version = "0.1" }
```

The library import remains `gphoto2`. This is a separate project from the
existing `gphoto2` package on crates.io.

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
cargo run -p gphoto2-bindings --example detect
```

## Requirements

libgphoto2 must be installed (e.g. `brew install libgphoto2`,
`apt install libgphoto2-dev`). See the
[workspace README](https://github.com/SauceTaster/libgphoto2-rs#building) for platform notes and build configuration.

## Threading

libgphoto2 is not thread-safe across a camera session, so `Context` and `Camera`
are intentionally neither `Send` nor `Sync`.

Use `CameraThread` to share a session safely between application threads. Its
blocking `run` method accepts jobs that return owned `Send` values. Calling
`run` from its own worker returns `ErrorKind::CameraBusy` instead of blocking.
See the `threaded` and `tether` examples in the source repository.

## Features

- `buildtime-bindgen` (default): generate bindings against installed headers;
  requires libclang. Disable default features to use committed bindings.
- `log`: forward native logging through the `log` crate. Handles can be added
  and dropped concurrently without modifying the native callback registry.

## License

Dual-licensed **MIT OR Apache-2.0**. Linking libgphoto2 (LGPL-2.1-or-later)
imposes LGPL obligations on the *distributed binary* — see the
[workspace README](https://github.com/SauceTaster/libgphoto2-rs#licensing).
Both license texts are included in this package. The native C library is a
separate dependency and is not bundled in the crate archive.
