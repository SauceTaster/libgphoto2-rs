#!/usr/bin/env bash
#
# Build libgphoto2 from the vendored source with its virtual PTP camera enabled
# (--enable-vusb, ptp2 camlib) into target/vusb-prefix, then run the vcamera
# integration tests against it. Used both locally and by CI.
#
# Requires: meson, ninja, a C toolchain, libltdl, pkg-config. On macOS these
# come from Homebrew (`brew install meson libtool`); on Debian/Ubuntu from
# `meson ninja-build libltdl-dev pkg-config gettext`.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
src="$root/vendor/libgphoto2"
build="$root/target/vusb-build"
prefix="$root/target/vusb-prefix"

if [ ! -f "$src/meson.build" ]; then
  echo "error: $src is empty — run: git submodule update --init" >&2
  exit 1
fi

# Platform specifics: where ltdl lives, and the runtime loader env var.
setup_args=(-Diolibs=vusb -Dcamlibs=ptp2 -Ddefault_library=shared)
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

echo ">> building vusb-enabled libgphoto2 into $prefix"
rm -rf "$build" "$prefix"
meson setup "$build" "$src" --prefix="$prefix" --libdir=lib "${setup_args[@]}"
ninja -C "$build" install

# Discover the versioned driver dirs rather than hard-coding versions.
iolibs="$(echo "$prefix"/lib/libgphoto2_port/*)"
camlibs="$(echo "$prefix"/lib/libgphoto2/*)"

# Build the crate against this libgphoto2 (env override bypasses pkg-config),
# and point the runtime at its drivers + the virtual camera's content.
export GPHOTO2_LIB_DIR="$prefix/lib"
export GPHOTO2_INCLUDE_DIR="$prefix/include"
export CAMLIBS="$camlibs"
export IOLIBS="$iolibs"
export GPHOTO2_VUSB_TESTS=1
existing="${!loader_var:-}"
export "${loader_var}=$prefix/lib${existing:+:$existing}"

echo ">> running vcamera tests"
cd "$root"
cargo test -p gphoto2 --test vcamera -- --test-threads=1
