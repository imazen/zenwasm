# Changelog

All notable changes to the zenwasm crates are documented here. The three crates
(`zenwasm-api`, `zenwasm-abi`, `zenwasm-types`) are versioned together.

## [Unreleased]

### Fixed
- **Pushes to `main` now cancel their superseded CI runs.** `ci.yml` keyed its concurrency group on `${{ github.head_ref || github.run_id }}`. `github.head_ref` is populated only for `pull_request` events, so on a push it was empty and the group fell through to `github.run_id` — unique per run, so no two pushes ever shared a group and `cancel-in-progress` could never fire. Every push started a full matrix that ran to completion even when several commits landed seconds apart. Now keyed on `${{ github.ref }}`, which is set for both event types (`refs/heads/main` on push, `refs/pull/N/merge` on a PR), so PR cancellation is unchanged and consecutive pushes supersede each other.

### Added
- Split crates.io README: `README.crates.md` is generated from `README.md`, and each published crate's `readme` field points at it. Full README overhaul — badge row, quick start against the current host API, crate/ABI reference, and the shared crosslink footer (docs only).

### Changed
- MSRV raised to 1.91 for wasmtime 43 (4b4a2ae).
- wasmtime bumped to 43.0.2 (99ea641).

## [0.1.0] - 2026-03-02

### Added
- Initial release (54b87d4):
  - `zenwasm-types` — shared `#[repr(C)]` types (`ImageInfo`, `WasmBuffer`, `DecodeOutput`, `EncodeOutput`, `ErrorCode`) and WASM export-name conventions; `no_std` + `#![forbid(unsafe_code)]`.
  - `zenwasm-abi` — `cdylib` wrapping wasmtime behind a versioned C ABI.
  - `zenwasm-api` — host loader via `libloading`, with no wasmtime dependency, plus the `WasmCodec` convenience wrapper and zero-copy `memory_slice` reads.
- GitHub Actions CI across Linux/macOS/Windows on x86-64 and ARM (c8e0d72).
