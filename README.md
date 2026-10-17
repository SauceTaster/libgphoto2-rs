# libgphoto2-rs

[![CI](https://github.com/SauceTaster/libgphoto2-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/SauceTaster/libgphoto2-rs/actions/workflows/ci.yml)

**Safe Rust bindings to [libgphoto2](https://github.com/gphoto/libgphoto2)** — the
C library behind `gphoto2`, digiKam, Darktable tethering, Entangle and friends,
which talks to [~2700 camera models](http://www.gphoto.org/proj/libgphoto2/support.php)
over USB, PTP/MTP, serial and IP.

**What this is today:** a raw FFI crate (`gphoto2-bindings-sys`) and a safe, idiomatic
wrapper (`gphoto2-bindings`) that link and call into the existing C library. It does
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

Requires Rust **1.82 or newer** and a separately installed libgphoto2. The
crates.io package names are `gphoto2-bindings` (safe wrapper) and
`gphoto2-bindings-sys` (raw FFI). They are separate from the existing `gphoto2`,
`gphoto2-sys`, and `libgphoto2_sys` projects. The Rust library names remain
`gphoto2` and `gphoto2_sys` for compatibility with existing Git users.

For the prepared 0.1.0 release:

```toml
[dependencies]
gphoto2 = { package = "gphoto2-bindings", version = "0.1" }
```

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
$ cargo run -p gphoto2-bindings --example detect
```

## Building

You need libgphoto2 (+ libgphoto2_port) and a C toolchain.

| Platform        | Install                                                     |
| --------------- | ---------------------------------------------------------- |
| macOS           | `brew install libgphoto2 pkgconf`                          |
| Debian / Ubuntu | `sudo apt install libgphoto2-dev pkg-config clang`         |
| Fedora          | `sudo dnf install libgphoto2-devel pkgconf-pkg-config clang` |
| Windows (MSYS2) | In a **UCRT64** shell: `pacman -S mingw-w64-ucrt-x86_64-libgphoto2 mingw-w64-ucrt-x86_64-pkgconf mingw-w64-ucrt-x86_64-rust` |

```console
cargo build
cargo test          # smoke tests run without a camera attached
```

By default `gphoto2-bindings-sys` regenerates its bindings with `bindgen` at build time
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

Run `cargo test --workspace --all-features` to include the log bridge and its
concurrent handle/native-operation regression test. CI also checks Rust 1.82,
docs.rs builds, and both packaged crate archives.

## Application integration

Keep each `Camera` and `Context` on its owning thread. For a GUI, async server,
or other application sharing a session, use `CameraThread` rather than adding
unsafe `Send` or `Sync` implementations. Its `run` method blocks the calling
thread, so invoke it from your runtime's blocking-work facility. Return owned
values, such as `Vec<u8>`, strings, or configuration snapshots, from each job.
See `cargo run -p gphoto2-bindings --example threaded`.

Calling `run` from the same worker returns `ErrorKind::CameraBusy`. Dropping
the final handle normally waits for queued work and releases the session; if
that happens on the worker itself, cleanup follows the current job.

The `tether` example selects an attached camera by port and collects multiple
file events, including RAW+JPEG captures, with bounded waiting:

```console
cargo run -p gphoto2-bindings --example tether
```

With the `log` feature, one permanent C callback is initialized before native
objects are created. `forward_to_log` handles only change synchronized Rust
forwarding state. Multiple handles forward each message once at the most
verbose active level. Other code using raw FFI must coordinate any changes it
makes to libgphoto2's process-global callback registry.

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

- **Miri** — the pure-unsafe paths (callback trampolines, event-data
  `malloc`/`free`, string marshalling, ABI guards) run under Miri/Stacked Borrows
  in CI (`cargo miri test --workspace --lib`); FFI-calling tests are excluded via
  `cfg(miri)`.
- **AddressSanitizer** — the `asan` CI lane builds libgphoto2 (vusb+ptp2) with
  ASan and runs the vcamera tests instrumented (`scripts/run-vcamera-asan.sh`),
  catching out-of-bounds / use-after-free across the Rust↔C boundary at runtime.

## Platform support

| Platform | State | Notes |
| -------- | ----- | ----- |
| Linux    | ✅ supported | `pkg-config` finds the system package. |
| macOS    | ✅ supported | `build.rs` auto-adds the Homebrew keg's `pkgconfig` dir. Note: capture/control of cameras can require quitting Photos/`PTPCamera`, which claim the USB device. |
| Windows (MSYS2 UCRT64) | ✅ supported | Use MSYS2's matching UCRT64 libgphoto2, pkgconf, and Rust packages together. Covered by CI. |
| Windows (MSVC) | ❌ unsupported | Use the MSYS2 UCRT64 toolchain described above. |
| docs.rs  | ✅ | Builds against committed bindings with linking disabled. |

On Windows, use the Rust compiler supplied by the UCRT64 package along with
its native libraries. Mixing a rustup GNU compiler, MINGW64 libraries, or a
different C runtime is not a supported configuration: C allocations returned
by libgphoto2 must be released through the same runtime. The older MINGW64
libgphoto2 package is no longer provided by current MSYS2 repositories.
The package command above supports `cargo build --no-default-features` without
libclang. For default-feature builds, also install
`mingw-w64-ucrt-x86_64-clang` in the same UCRT64 environment.

## Dependency choices (the "be smart for LTS" part)

Picked deliberately for a library meant to live a long time:

- **`pkg-config`** + **`bindgen`** (build-time only) — the boring, universal way
  to find and bind a C library. Bindings are also committed so the crate builds
  where `libclang` isn't, and so `docs.rs` works.
- **No runtime dependencies** in `gphoto2-bindings-sys` beyond `libc`-style `core::ffi`
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
2. **Next** — partial/range reads (`gp_camera_file_read`) and direct-to-disk
   downloads. The 0.1.0 release preparation includes distinct package names,
   a [changelog](CHANGELOG.md), MSRV/package/docs checks, and the
   [release procedure](RELEASING.md).

**Maybe later — pure Rust (aspirational, not started)**

3. A pure-Rust I/O port layer (USB via `nusb`, serial via `serialport`).
4. A pure-Rust PTP/MTP driver (the `ptp2` camlib covers most modern cameras).
5. The long tail of drivers, keeping the C library as the fallback.

Until and unless that happens, this is a binding to the C library — and that's
the honest framing.

## Licensing

The original Rust code here is dual-licensed **MIT OR Apache-2.0** (your choice).
That covers `gphoto2-bindings-sys` (FFI bindings) and `gphoto2-bindings` (the safe wrapper), which
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
