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

/// ABI guards: catch any drift between the (possibly committed) bindings and the
/// real libgphoto2 layout — especially `time_t`, which is 64-bit on every
/// supported target and must not be silently narrowed (see the `time_t` note in
/// `bindings_pregenerated.rs`). These run on every target in CI, including the
/// Windows `--no-default-features` path.
#[cfg(test)]
mod abi_layout {
    use super::*;

    #[test]
    fn time_t_is_64_bit() {
        assert_eq!(core::mem::size_of::<time_t>(), 8);
        assert_eq!(core::mem::align_of::<time_t>(), 8);
    }

    #[test]
    fn camera_file_info_file_has_room_for_64bit_mtime() {
        // `mtime` is the last (8-byte-aligned) field; an undersized `time_t`
        // shrinks the struct and lets the C library write past it.
        assert_eq!(core::mem::size_of::<CameraFileInfoFile>(), 104);
        let uninit = core::mem::MaybeUninit::<CameraFileInfoFile>::uninit();
        let base = uninit.as_ptr();
        // SAFETY: addr_of! does not read the uninitialized memory.
        let off = unsafe {
            core::ptr::addr_of!((*base).mtime)
                .cast::<u8>()
                .offset_from(base.cast::<u8>())
        };
        assert_eq!(off, 96);
    }
}
