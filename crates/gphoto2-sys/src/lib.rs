//! Raw, unsafe FFI bindings to [libgphoto2](http://www.gphoto.org/) and
//! libgphoto2_port.
//!
//! This crate is the bottom layer of `vibe-libgphoto2-rs`. It contains only the
//! machine-generated declarations and the linkage to the native library — no
//! abstractions, no safety. Use the `gphoto2` crate for a safe API.
//!
//! By default the bindings are generated at build time against the libgphoto2
//! installed on the host (see `build.rs`). With `--no-default-features` the
//! committed bindings in `bindings_pregenerated.rs` are used instead.
#![allow(
    non_upper_case_globals,
    non_camel_case_types,
    non_snake_case,
    dead_code,
    rustdoc::all,
    clippy::all,
    clippy::pedantic
)]

#[cfg(feature = "buildtime-bindgen")]
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(not(feature = "buildtime-bindgen"))]
include!("bindings_pregenerated.rs");
