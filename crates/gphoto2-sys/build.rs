//! Build script for `gphoto2-sys`.
//!
//! Responsibilities, in order:
//!   1. Tell Cargo when to re-run.
//!   2. On docs.rs, do nothing but rely on the committed bindings (no system
//!      libgphoto2 is available there, and we must not try to link).
//!   3. Locate the native libgphoto2 + libgphoto2_port libraries and emit the
//!      `cargo:rustc-link-*` directives so downstream crates link correctly.
//!   4. (default feature) regenerate bindings against the located headers.
//!
//! Discovery strategy (first match wins):
//!   * Explicit `GPHOTO2_LIB_DIR` / `GPHOTO2_INCLUDE_DIR` env overrides — the
//!     escape hatch for Windows and bespoke installs where pkg-config is absent.
//!   * `pkg-config` (the normal path on Linux and macOS/Homebrew). On macOS we
//!     transparently add the Homebrew keg's `pkgconfig` dir to PKG_CONFIG_PATH
//!     so a plain `brew install libgphoto2` just works.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=GPHOTO2_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=GPHOTO2_LIB_DIR");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    // docs.rs: no system library; use the committed bindings, never link.
    if env::var_os("DOCS_RS").is_some() {
        return;
    }

    let include_paths = discover_and_link();

    #[cfg(feature = "buildtime-bindgen")]
    generate_bindings(&include_paths);

    // Without the bindgen feature we ship `src/bindings_pregenerated.rs`; the
    // include paths were only needed for codegen, so silence the unused warning.
    #[cfg(not(feature = "buildtime-bindgen"))]
    let _ = include_paths;
}

/// Emit link directives and return the include dirs to feed bindgen.
fn discover_and_link() -> Vec<PathBuf> {
    // 1. Explicit overrides.
    if let Some(lib_dir) = env::var_os("GPHOTO2_LIB_DIR") {
        let lib_dir = PathBuf::from(lib_dir);
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
        println!("cargo:rustc-link-lib=gphoto2");
        println!("cargo:rustc-link-lib=gphoto2_port");

        match env::var_os("GPHOTO2_INCLUDE_DIR") {
            Some(inc) => return vec![PathBuf::from(inc)],
            // With the default build-time bindgen, bindgen needs the headers; an
            // empty include list would fail later with a bare "gphoto2.h not
            // found". Fail now with guidance.
            None if cfg!(feature = "buildtime-bindgen") => panic!(
                "GPHOTO2_LIB_DIR is set but GPHOTO2_INCLUDE_DIR is not, so bindgen \
                 has no headers to parse. Set GPHOTO2_INCLUDE_DIR to the dir \
                 containing `gphoto2/gphoto2.h`, or build with \
                 `--no-default-features` to use the committed bindings."
            ),
            // Committed-bindings build doesn't run bindgen; no headers needed.
            None => return Vec::new(),
        }
    }

    // 2. pkg-config.
    #[cfg(target_os = "macos")]
    inject_homebrew_pkg_config_path();

    // libgphoto2.pc already lists libgphoto2_port in its link line, so probing
    // the umbrella package is sufficient for linking; we also probe the port
    // package to harvest its include dir (and to surface a clear error early).
    let lib = pkg_config::Config::new()
        .atleast_version("2.5")
        .probe("libgphoto2")
        .unwrap_or_else(|e| panic!("{}", discovery_error(&e.to_string())));

    let mut include_paths = lib.include_paths.clone();
    if let Ok(port) = pkg_config::Config::new().probe("libgphoto2_port") {
        include_paths.extend(port.include_paths);
    }
    include_paths
}

/// A platform-aware error message when libgphoto2 cannot be located.
fn discovery_error(detail: &str) -> String {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    let mut msg = format!("Could not find libgphoto2 via pkg-config: {detail}\n");
    if target_os == "windows" {
        if target_env == "msvc" {
            msg.push_str(
                "libgphoto2 has no native MSVC port; build for the GNU toolchain \
                 (x86_64-pc-windows-gnu) inside MSYS2 instead.\n",
            );
        }
        msg.push_str(
            "On Windows, use MSYS2/MinGW:\n  \
             pacman -S mingw-w64-x86_64-libgphoto2 mingw-w64-x86_64-pkgconf\n",
        );
    } else {
        msg.push_str(
            "Install it (Debian/Ubuntu: `libgphoto2-dev`, Fedora: `libgphoto2-devel`, \
             macOS: `brew install libgphoto2`).\n",
        );
    }
    msg.push_str("Or set GPHOTO2_LIB_DIR / GPHOTO2_INCLUDE_DIR to point at a custom install.");
    msg
}

/// On macOS, prepend the Homebrew libgphoto2 keg's pkgconfig dir to
/// `PKG_CONFIG_PATH` so pkg-config resolves it without manual env setup.
#[cfg(target_os = "macos")]
fn inject_homebrew_pkg_config_path() {
    use std::process::Command;

    let Ok(out) = Command::new("brew")
        .args(["--prefix", "libgphoto2"])
        .output()
    else {
        return;
    };
    if !out.status.success() {
        return;
    }
    let prefix = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if prefix.is_empty() {
        return;
    }
    let pc_dir = format!("{prefix}/lib/pkgconfig");
    let combined = match env::var("PKG_CONFIG_PATH") {
        Ok(existing) if !existing.is_empty() => format!("{pc_dir}:{existing}"),
        _ => pc_dir,
    };
    env::set_var("PKG_CONFIG_PATH", combined);
}

#[cfg(feature = "buildtime-bindgen")]
fn generate_bindings(include_paths: &[PathBuf]) {
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        // Public surface only: gp_* functions, the GP_* constants (incl. error
        // codes and GP_MIME_*), and the Camera*/GP*/Gphoto* types they touch.
        .allowlist_function("gp_.*")
        .allowlist_type("_?(Camera|GP|Gphoto).*")
        .allowlist_var("GP_.*")
        // NewType keeps FFI-returned enums sound even if the C side yields a
        // value outside the known variants (rustified enums would be UB).
        .default_enum_style(bindgen::EnumVariation::NewType {
            is_bitfield: false,
            is_global: false,
        })
        .prepend_enum_name(false)
        .derive_default(true)
        .derive_debug(true)
        .derive_copy(true)
        // Layout tests embed host-specific sizes; skip them so the same crate
        // builds when cross-compiling.
        .layout_tests(false)
        .generate_comments(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for path in include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }

    let bindings = builder
        .generate()
        .expect("failed to generate libgphoto2 bindings");
    let out = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set")).join("bindings.rs");
    bindings
        .write_to_file(&out)
        .expect("failed to write generated bindings");
}
