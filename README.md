# vibe-libgphoto2-rs

[![CI](https://github.com/SauceTaster/vibe-libgphoto2-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/SauceTaster/vibe-libgphoto2-rs/actions/workflows/ci.yml)

**Safe Rust bindings to [libgphoto2](https://github.com/gphoto/libgphoto2)** — the
C library behind `gphoto2`, digiKam, Darktable tethering, Entangle and friends,
which talks to [~2700 camera models](http://www.gphoto.org/proj/libgphoto2/support.php)
over USB, PTP/MTP, serial and IP.

**What this is today:** a raw FFI crate (`gphoto2-sys`) and a safe, idiomatic
wrapper (`gphoto2`) that link and call into the existing C library. It does
**not** reimplement libgphoto2 in Rust — it *binds* it. If you want to talk to a
camera from Rust today, that's exactly what this provides.

**Where it's heading:** the long-term aspiration is a pure-Rust implementation
that could replace the C internals behind this same safe API. That's a big,
open-ended direction and is **not started** — so for now, treat this as bindings,
not a rewrite.

> **Status:** Usable bindings. Enumerate cameras, read abilities and the config
> (widget) tree, capture, and download — all through the C library. See the
> [roadmap](#roadmap).

## Workspace layout

```
crates/
  gphoto2-sys/   Raw FFI bindings + native linkage to libgphoto2 & libgphoto2_port
  gphoto2/       Safe, idiomatic Rust API (RAII, Result, structured errors)
vendor/
  libgphoto2/    Pinned upstream source (v2.5.34), as a submodule — reference only
```

The two-crate split follows the standard Rust `foo-sys` + `foo` convention (à la
`libgit2-sys` / `git2`) and mirrors libgphoto2's own `libgphoto2` /
`libgphoto2_port` separation. Keeping a stable safe API on top leaves room to
swap the implementation underneath later — but today every call goes straight
through to the C library.

## Quick start

```rust
use gphoto2::{Camera, Context};

let context = Context::new()?;

for cam in Camera::autodetect(&context)? {
    println!("{} @ {}", cam.model, cam.port);
}

let camera = Camera::new(&context)?;        // open the attached camera
let path = camera.capture_image(&context)?; // shutter
let file = camera.download(&context, &path)?;
file.save(&path.name)?;
# Ok::<(), gphoto2::Error>(())
```

```console
$ cargo run -p gphoto2 --example detect
```

## Building

You need libgphoto2 (+ libgphoto2_port) and a C toolchain.

| Platform        | Install                                                     |
| --------------- | ---------------------------------------------------------- |
| macOS           | `brew install libgphoto2 pkgconf`                          |
| Debian / Ubuntu | `sudo apt install libgphoto2-dev pkg-config clang`         |
| Fedora          | `sudo dnf install libgphoto2-devel pkgconf-pkg-config clang` |
| Windows (MSYS2) | In a MINGW64 shell: `pacman -S mingw-w64-x86_64-libgphoto2 mingw-w64-x86_64-pkgconf mingw-w64-x86_64-rust` |

```console
cargo build
cargo test          # smoke tests run without a camera attached
```

By default `gphoto2-sys` regenerates its bindings with `bindgen` at build time
so they always match the installed ABI (needs `libclang`). To build against the
committed bindings instead (no `libclang` required):

```console
cargo build --no-default-features    # uses src/bindings_pregenerated.rs
```

### Build configuration

| Knob                              | Effect                                                        |
| --------------------------------- | ------------------------------------------------------------ |
| feature `buildtime-bindgen` (default) | Regenerate bindings via `bindgen` against installed headers |
| `--no-default-features`           | Use committed `bindings_pregenerated.rs` (no `libclang`)      |
| feature `log`                     | Bridge libgphoto2's logging to the [`log`](https://crates.io/crates/log) crate (see `log_bridge`) |
| env `GPHOTO2_LIB_DIR`             | Link dir override when `pkg-config` is absent (e.g. Windows)  |
| env `GPHOTO2_INCLUDE_DIR`         | Header dir override for bindgen                               |
| env `PKG_CONFIG_PATH`             | Standard pkg-config search path                               |

## Testing

The test suite runs **without any camera attached**:

- **Unit tests** cover the pure logic — error-code → `ErrorKind` mapping, C-string
  marshalling (NUL handling, lossy UTF-8), enum/bitmask decoding.
- **Integration tests** drive libgphoto2's built-in **"Directory Browse" virtual
  camera** (the `disk:` port) — the same hardware-free mechanism libgphoto2's own
  CLI test suite uses. They exercise the full FFI path end to end: resolve a
  model + port, `init`, list folders/files, and download a file with byte-for-byte
  verification. (The `directory` camlib only *lists* files with recognised media
  extensions, e.g. `.jpg` — a quirk the tests account for.)

```console
cargo test                       # everything, no camera needed
cargo test --no-default-features # same, against the committed bindings
```

- **Virtual PTP camera (vcamera)** — for the paths the directory camera can't
  reach (capture, the config widget tree, events), `tests/vcamera.rs` drives
  libgphoto2's built-in **virtual PTP camera** (the `vusb` driver + `ptp2`
  camlib). It needs a libgphoto2 built with `--enable-vusb`, so it's gated behind
  `GPHOTO2_VUSB_TESTS` and skips otherwise. One command builds that libgphoto2
  from the vendored source and runs the suite (also a CI lane):

  ```console
  git submodule update --init        # vendor/libgphoto2
  ./scripts/run-vcamera-tests.sh     # needs meson + ninja + libltdl
  ```

## Platform support

| Platform | State | Notes |
| -------- | ----- | ----- |
| Linux    | ✅ supported | `pkg-config` finds the system package. |
| macOS    | ✅ supported | `build.rs` auto-adds the Homebrew keg's `pkgconfig` dir. Note: capture/control of cameras can require quitting Photos/`PTPCamera`, which claim the USB device. |
| Windows (MinGW) | ✅ supported | Build with the **GNU toolchain** in [MSYS2](https://www.msys2.org/): install `mingw-w64-x86_64-libgphoto2` and `pkg-config` resolves it normally. Covered by CI. |
| Windows (MSVC) | ❌ unsupported | libgphoto2 has [no MSVC port](https://github.com/gphoto/libgphoto2/issues/33) and isn't in vcpkg — a libgphoto2 limitation, not this crate's. Use the MinGW path above. |
| docs.rs  | ✅ | Builds against committed bindings with linking disabled. |

## Dependency choices (the "be smart for LTS" part)

Picked deliberately for a library meant to live a long time:

- **`pkg-config`** + **`bindgen`** (build-time only) — the boring, universal way
  to find and bind a C library. Bindings are also committed so the crate builds
  where `libclang` isn't, and so `docs.rs` works.
- **No runtime dependencies** in `gphoto2-sys` beyond `libc`-style `core::ffi`
  types from std. Fewer moving parts = fewer LTS liabilities.
- **`thiserror`/`anyhow` deliberately avoided** in the core — the error type is a
  thin hand-rolled wrapper over libgphoto2's integer codes, so the public API
  carries no third-party type in its signatures.
- **Aspirational (only if a pure-Rust port ever happens):**
  [`nusb`](https://crates.io/crates/nusb) (pure-Rust USB, no system libusb) over
  `rusb`/`libusb`, and [`serialport`](https://crates.io/crates/serialport). These
  are **not** current dependencies — listed only to record the intended direction.

## Roadmap

Near-term work stays in the bindings layer. The pure-Rust items are a long-term
aspiration — a direction, not a commitment or a schedule.

**Now — bindings & safe wrapper**

1. **Done** — `-sys` bindings, safe RAII wrapper, autodetect / open-by-model+port
   / abilities / capture / download / `supported_models` / config (widget) tree /
   events (`wait_for_event`) + tethered `trigger_capture` + liveview
   `capture_preview` / context callbacks (progress, cancel, message/status/error)
   as Rust closures / **filesystem management (list, typed downloads, file info,
   delete, upload, mkdir/rmdir, delete-all, recursive walk, storage info) +
   about/manual text / capability introspection (per-model operation support) /
   libgphoto2 logging → `log` bridge (`log` feature) / **`CameraThread`: a
   `Send + Sync` session pinned to its own thread**, cross-platform `build.rs`,
   plus unit tests (the unsafe callback bridging is Miri-checked) and hardware-free
   integration tests against **both** the directory camera and the **virtual PTP
   camera (vcamera)** — the latter covering real capture / config / events.
2. **Next** — partial/range reads (`gp_camera_file_read`) and publish prep (final
   crate name, CHANGELOG, MSRV job).

**Maybe later — pure Rust (aspirational, not started)**

3. A pure-Rust I/O port layer (USB via `nusb`, serial via `serialport`).
4. A pure-Rust PTP/MTP driver (the `ptp2` camlib covers most modern cameras).
5. The long tail of drivers, keeping the C library as the fallback.

Until and unless that happens, this is a binding to the C library — and that's
the honest framing.

## Licensing

The original Rust code here is dual-licensed **MIT OR Apache-2.0** (your choice).
That covers `gphoto2-sys` (FFI bindings) and `gphoto2` (the safe wrapper), which
only *link* libgphoto2 and contain no code copied from it.

Two things to know:

- **Linking libgphoto2 (LGPL-2.1-or-later) imposes obligations on the
  *distributed binary*, not on this source.** Anyone shipping a program linked
  against libgphoto2 must let end users relink against a modified libgphoto2
  (dynamic linking satisfies this) and include the LGPL notice. The permissive
  license on our source is unaffected.
- **Ported code will be LGPL.** Any future crate that *translates* libgphoto2's
  implementation into Rust (e.g. a pure-Rust PTP driver or port layer) is a
  derivative work and will be licensed **LGPL-2.1-or-later** in its own crate,
  keeping the permissive bindings/wrapper cleanly separated from derived code.

Upstream libgphoto2 is referenced as a git submodule (`vendor/libgphoto2`,
pinned to v2.5.34) under its own LGPL-2.1-or-later license — it is not vendored
into this repo's history.

## Credits

All camera-talking smarts come from the [gphoto project](http://www.gphoto.org/)
and its contributors. This repo just gives it a Rust coat.
