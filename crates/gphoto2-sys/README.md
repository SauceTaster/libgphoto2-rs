# gphoto2-sys

Raw, unsafe FFI bindings to the [libgphoto2](http://www.gphoto.org/) C library
(`libgphoto2` + `libgphoto2_port`).

This is the bottom layer of [`vibe-libgphoto2-rs`](https://github.com/SauceTaster/vibe-libgphoto2-rs).
It contains only machine-generated declarations and the native linkage — no
abstractions, no safety. For a usable API, depend on the
[`gphoto2`](../gphoto2) crate instead.

## How it links

`build.rs` locates the native library, in order:

1. `GPHOTO2_LIB_DIR` / `GPHOTO2_INCLUDE_DIR` env overrides (for systems without
   `pkg-config`, e.g. Windows).
2. `pkg-config` (Linux, and macOS/Homebrew — the Homebrew keg's `pkgconfig` dir
   is added automatically).

## Bindings

- **Default** (`buildtime-bindgen` feature): regenerated with `bindgen` at build
  time against the installed headers, so they always match the host ABI.
  Requires `libclang`.
- **`--no-default-features`**: uses the committed `src/bindings_pregenerated.rs`.
  No `libclang` needed. This is also what `docs.rs` builds against (with linking
  disabled).

Regenerate the committed bindings after a libgphoto2 upgrade by copying the
build output:

```console
cargo build -p gphoto2-sys
cp "$(find target -name bindings.rs -path '*gphoto2-sys*' | head -1)" \
   crates/gphoto2-sys/src/bindings_pregenerated.rs
```

## License

Dual-licensed **MIT OR Apache-2.0** (the bindings only link libgphoto2; they
copy no code from it). Note that linking libgphoto2 (LGPL-2.1-or-later) imposes
LGPL obligations on the *distributed binary* — see the
[workspace README](../../README.md#licensing).
