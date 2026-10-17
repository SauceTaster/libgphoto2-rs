#!/usr/bin/env bash
# Assemble and build both unpublished crates from their packaged contents.
# Requires a recent stable Cargo with workspace packaging support, tar, and the
# normal native libgphoto2 build dependencies. --check-only skips executable
# linking, for source checks on hosts without a supported native installation.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
cargo_bin="${CARGO:-cargo}"
package_target="${GPHOTO2_PACKAGE_TARGET_DIR:-$root/target/package-check}"
if command -v cygpath >/dev/null 2>&1; then
  package_target="$(cygpath -u "$package_target")"
fi
package_options=()
build_options=()
build_command=build
for option in "$@"; do
  case "$option" in
    --allow-dirty) package_options+=("$option") ;;
    --offline)
      package_options+=("$option")
      build_options+=("$option")
      ;;
    --check-only) build_command=check ;;
    *) echo "usage: $0 [--allow-dirty] [--offline] [--check-only]" >&2; exit 2 ;;
  esac
done

# Cargo's temporary-registry verification can fail for unpublished workspace
# dependencies on some hosts. Build the extracted archives directly instead.
"$cargo_bin" package --workspace --all-features --locked --no-verify \
  --target-dir "$package_target" "${package_options[@]}"

temp_root="$(cd "${TMPDIR:-/tmp}" && pwd -P)"
stage="$(mktemp -d "$temp_root/gphoto2-package-check.XXXXXX")"
stage="$(cd "$stage" && pwd -P)"
if [[ "$(dirname "$stage")" != "$temp_root" || "${stage##*/}" != gphoto2-package-check.* ]]; then
  echo "refusing cleanup of unexpected temporary directory: $stage" >&2
  exit 1
fi
trap 'rm -rf -- "$stage"' EXIT
for package in gphoto2-bindings gphoto2-bindings-sys; do
  version="$("$cargo_bin" pkgid -p "$package")"
  version="${version##*@}"
  directory="${package}-${version}"
  archive="$package_target/package/$directory.crate"
  listing="$stage/$package.files"
  tar -tzf "$archive" > "$listing"
  if grep -E '/(vendor|target)/|\.(c|cc|cpp|cxx|o|a|so(\.[0-9]+)*|dylib|dll|lib|exe)$' "$listing"; then
    echo "unexpected native source, binary, or build output in $archive" >&2
    exit 1
  fi
  tar -xzf "$archive" -C "$stage"
  cmp "$root/LICENSE-MIT" "$stage/$directory/LICENSE-MIT"
  cmp "$root/LICENSE-APACHE" "$stage/$directory/LICENSE-APACHE"
  if [[ "$package" == gphoto2-bindings ]]; then
    safe_dir="$stage/$directory"
  else
    sys_dir="$stage/$directory"
  fi
done
if command -v cygpath >/dev/null 2>&1; then
  safe_dir="$(cygpath -m "$safe_dir")"
  sys_dir="$(cygpath -m "$sys_dir")"
fi

# Only the extracted sys crate is patched locally; every other dependency is
# resolved normally. The normalized wrapper manifest must therefore contain
# the correct registry dependency name, version, and feature forwarding.
patch="patch.crates-io.gphoto2-bindings-sys.path=\"$sys_dir\""
echo ">> $build_command packaged crates with all features"
"$cargo_bin" "$build_command" --manifest-path "$safe_dir/Cargo.toml" \
  --all-targets --all-features --config "$patch" \
  --target-dir "$package_target" "${build_options[@]}"
echo ">> check packaged crates with committed bindings"
"$cargo_bin" check --manifest-path "$safe_dir/Cargo.toml" \
  --all-targets --no-default-features --config "$patch" \
  --target-dir "$package_target" "${build_options[@]}"
