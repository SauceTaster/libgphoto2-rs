# gphoto2-bindings-sys

Raw, unsafe FFI bindings to the [libgphoto2](http://www.gphoto.org/) C library
(`libgphoto2` + `libgphoto2_port`).

This is the bottom layer of [libgphoto2-rs](https://github.com/SauceTaster/libgphoto2-rs).
It contains machine-generated declarations and native linkage. For a safe API,
depend on [gphoto2-bindings](https://crates.io/crates/gphoto2-bindings) instead.

Requires Rust **1.82+**. The library import remains `gphoto2_sys`:

```toml
[dependencies]
gphoto2-sys = { package = "gphoto2-bindings-sys", version = "0.1" }
```

This package is separate from the existing `gphoto2-sys` and `libgphoto2_sys`
projects. The native `links = "gphoto2"` identifier is retained so Cargo can
prevent conflicting copies of that native library in one dependency graph.

## How it links

`build.rs` locates the native library, in order:

1. `GPHOTO2_LIB_DIR` / `GPHOTO2_INCLUDE_DIR` overrides for custom installations.
2. `pkg-config` on Linux, macOS/Homebrew, and Windows/MSYS2. The Homebrew keg's
   `pkgconfig` directory is added automatically on macOS.

## Bindings

- **Default** (`buildtime-bindgen`): regenerate with bindgen against installed
  headers. Requires libclang.
- **`--no-default-features`**: use `src/bindings_pregenerated.rs`; no libclang.
- **docs.rs**: use committed bindings with native discovery and linking disabled,
  even when default features are enabled by a dependent crate.

Regenerate the committed bindings after a libgphoto2 upgrade by copying the
build output, then review platform-specific ABI definitions before committing:

```console
cargo build -p gphoto2-bindings-sys
cp "$(find target -name bindings.rs -path '*gphoto2-bindings-sys*' | head -1)" \
   crates/gphoto2-sys/src/bindings_pregenerated.rs
```

## License

Dual-licensed **MIT OR Apache-2.0**. Both license texts are included in this
package. It contains declarations and linkage support, and does not bundle
the native C library or its drivers. The separately installed native library
retains its own upstream license. See the
[project licensing notes](https://github.com/SauceTaster/libgphoto2-rs#licensing).
