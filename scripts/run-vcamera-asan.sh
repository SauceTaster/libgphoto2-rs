#!/usr/bin/env bash
#
# Build libgphoto2 (vusb + ptp2) with AddressSanitizer and run the vcamera tests
# instrumented, so out-of-bounds / use-after-free in our FFI marshalling — or in
# libgphoto2 itself — are caught at runtime during real capture/config/events.
#
# Linux-focused (the CI `asan` lane). Needs: a nightly toolchain with `rust-src`,
# clang, meson, ninja, libltdl. The libgphoto2 .so is built with clang's ASan so
# its interceptors resolve against the Rust binary's (LLVM) ASan runtime.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/vendor/libgphoto2"
build="$root/target/asan-build"
prefix="$root/target/asan-prefix"

if [ ! -f "$src/meson.build" ]; then
  echo "error: $src is empty — run: git submodule update --init" >&2
  exit 1
fi

host="$(rustc +nightly -vV | sed -n 's/^host: //p')"

setup_args=(
  -Diolibs=vusb -Dcamlibs=ptp2 -Ddefault_library=shared
  -Db_sanitize=address
  -Db_lundef=false   # ASan interceptor symbols are resolved at load time
)
case "$(uname -s)" in
  Darwin)
    lt="$(brew --prefix libtool)"
    setup_args+=("-Dc_args=-I$lt/include" "-Dc_link_args=-L$lt/lib")
    loader_var=DYLD_LIBRARY_PATH
    ;;
  *)
    loader_var=LD_LIBRARY_PATH
    ;;
esac

echo ">> building ASan libgphoto2 into $prefix (clang)"
rm -rf "$build" "$prefix"
CC=clang meson setup "$build" "$src" --prefix="$prefix" --libdir=lib "${setup_args[@]}"
ninja -C "$build" install

iolibs="$(echo "$prefix"/lib/libgphoto2_port/*)"
camlibs="$(echo "$prefix"/lib/libgphoto2/*)"

export GPHOTO2_LIB_DIR="$prefix/lib"
export GPHOTO2_INCLUDE_DIR="$prefix/include"
export CAMLIBS="$camlibs"
export IOLIBS="$iolibs"
export GPHOTO2_VUSB_TESTS=1
existing="${!loader_var:-}"
export "${loader_var}=$prefix/lib${existing:+:$existing}"
# We want OOB/UAF, not leaks: libgphoto2/ptp2 keep one-shot state. The camlibs
# are dlopened, so relax ASan's initial-link-order check.
export ASAN_OPTIONS="detect_leaks=0:verify_asan_link_order=0:abort_on_error=1"

echo ">> running vcamera tests under -Zsanitizer=address (target $host)"
cd "$root"
# Passing --target explicitly keeps RUSTFLAGS off host build scripts / proc-macros
# (so bindgen isn't instrumented); -Zbuild-std instruments std too.
RUSTFLAGS="-Zsanitizer=address" \
  cargo +nightly test -Zbuild-std --target "$host" \
  -p gphoto2 --test vcamera -- --test-threads=1
