# Releasing 0.1.0

The prepared packages are `gphoto2-bindings-sys` and `gphoto2-bindings`. These
names distinguish this repository from other published camera bindings. Name
availability was checked during preparation; crates.io assigns names only on
publication. Existing Git dependency users must add the matching `package`
field after updating to this revision; Rust imports remain unchanged.

This procedure prepares and verifies artifacts. Publishing and creating the
public release are separate, deliberate actions.

## Before publishing

1. Run CI on the release commit, including Linux, macOS, MinGW, Rust 1.82,
   docs.rs, virtual-camera, Miri, AddressSanitizer, and package verification.
2. Run the following on Linux/macOS with native libgphoto2 and libclang installed:

   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
   cargo test --workspace --all-targets --all-features --locked
   cargo test --workspace --all-targets --no-default-features --locked
   cargo test --workspace --doc --all-features --locked
   DOCS_RS=1 cargo doc --workspace --no-deps --all-features --locked
   bash ./scripts/check-packages.sh
   ```

3. Inspect both `target/package-check/package/*.crate` archives. Each must contain
   `LICENSE-MIT` and `LICENSE-APACHE` identical to this repository's license
   texts. Neither archive should contain `vendor/`, native C implementation
   files, native binaries, or RapidRAW source. Check normalized manifests for
   version 0.1.0, Rust 1.82, the correct repository, and `MIT OR Apache-2.0`.
4. Review the changelog and native deployment requirements. The native library
   retains its own upstream licensing; the binding licenses do not replace it.
5. Run `cargo publish -p gphoto2-bindings-sys --dry-run`. The wrapper's individual
   publish dry run requires the sys package to be available from crates.io;
   package-check script builds the pair from their extracted archives with a
   temporary local dependency patch before that first publish. The patch is
   only used during verification and is not shipped in either crate.

## Publishing order

After the release candidate has been reviewed and publication authorized:

```sh
cargo publish -p gphoto2-bindings-sys
# Wait until crates.io can resolve gphoto2-bindings-sys 0.1.0.
cargo publish -p gphoto2-bindings --dry-run
cargo publish -p gphoto2-bindings
```

Verify both docs.rs builds and install a small consumer using the published
safe package. Then tag the reviewed commit `v0.1.0` and create a GitHub release
with the changelog. No native C library or driver binaries are bundled.
