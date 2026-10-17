# Changelog

## 0.1.0 — release candidate

Initial crates.io release of the existing safe wrapper and raw FFI as
`gphoto2-bindings` and `gphoto2-bindings-sys`. Rust library imports remain
`gphoto2` and `gphoto2_sys`.

### Fixed

- Install one permanent native log callback before wrapper-created contexts or
  files can emit logs. Log handles now change Rust forwarding state rather than
  racing with native logging while adding or removing callbacks. Overlapping
  handles forward each message once at their most verbose active level.
- Reject nested `CameraThread::run` calls on the same worker with
  `ErrorKind::CameraBusy`; avoid joining the worker from itself during teardown.
  Shutdown also closes a poisoned sender before waiting for the worker.
- Build docs.rs documentation with default, all, or no default features, and
  rerun native discovery when leaving a docs.rs build environment.

### Release preparation

- Set the supported Rust minimum to 1.82 and add an MSRV CI lane.
- Add package verification, license archive checks, all-feature linting, and
  documentation build checks to CI.
- Include the unchanged MIT and Apache-2.0 license texts in both crate archives.
- Correct repository metadata and links in published READMEs.
- Expand tethering to collect multiple captured files with bounded waits, and
  add an example for shared camera-thread sessions and owned configuration data.

All new implementation work is original Rust code under the repository's
existing MIT OR Apache-2.0 license. Native libgphoto2 remains a separately
installed dependency under its own upstream license.
