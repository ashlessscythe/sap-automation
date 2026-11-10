# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A Windows-only Rust tool (`sap_automation`) that drives SAP GUI via the SAP GUI Scripting COM API (`sap-scripting` crate + `windows`). It has an interactive ratatui/dialoguer menu and an unattended CLI (loop / sequence / single-shot) that exports SAP reports (VT11, ZVT11, VL06O, LX03, ZMDESNR, "149" reports, inbound shipment) to files. Needs a running SAP GUI to do real work; user docs are in `README.md`, `CONFIG.md`, `COMMAND_LINE_USAGE.md`, `TROUBLESHOOTING.md`, `DEBUG.md`.

## Commands

The toolchain is pinned to Rust 1.88.0 (`rust-toolchain.toml`) and `.cargo/config.toml` forces `target = "i686-pc-windows-msvc"` (32-bit, to match SAP GUI's COM). Run `rustup target add i686-pc-windows-msvc` once. Output lands in `target/i686-pc-windows-msvc/`, not `target/debug/`.

CI (`.github/workflows/ci.yml`, windows-latest, on `dev`) runs exactly:

```
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo build --locked --all-targets
cargo test --locked
```

Single test: `cargo test --test config_tests some_test_name` (integration files are in `tests/`), or `cargo test --lib name` for unit tests in `src/`.

Run: `cargo run -- --help`; `cargo run --example keep_awake_demo`. Copy `config.toml.example` to `config.toml` (git-ignored) for local use.

Dependency pins matter: `aes = "=0.9.2"` is pinned because newer needs rustc 1.89; keep `Cargo.lock` committed (CI uses `--locked`). CI also runs an osv-scanner on `Cargo.lock`.

## Architecture

**Lib + bin duplication.** `src/lib.rs` and `src/main.rs` each declare the same module tree (`mod app; mod vt11; ...`). Adding a module means adding it to **both** files. Integration tests in `tests/` import via the lib (`sap_automation::...`); the binary compiles its own copy. Because of this, many items are only used by one target, hence the `#![allow(dead_code)]` headers (referred to as "policy 4.A") — don't "fix" them by deleting code.

**Per-tcode pair pattern.** Each SAP transaction has `<tcode>.rs` (low-level SAP GUI steps/params, e.g. `run_export`) and `<tcode>_module.rs` (interactive prompts plus `run_<tcode>_auto` for unattended mode, driven by config). `y_149*` has variants (`_material`, `_rcv`) and `vl06o` has a `_delivery_module`.

**Dispatch.** `main.rs` parses `cli::Cli`, converts to `CliOverrides` (`utils/cli_overrides.rs`, a process-wide `OnceLock` set before any config load), validates cross-flag rules, then either shows the TUI menu or calls `utils/unattended_runner.rs`. Menu options are numeric string IDs mapped to `run_*_auto` functions in `utils::sequence_config::execute_menu_option`; the interactive menu in `main.rs` and the sequence runner must stay in sync with these IDs (see "Sequence Menu Option IDs" in README).

**Config layering.** `config.toml` → `utils/config_types.rs` (`SapConfig`), `config_ops.rs`/`config_handlers.rs` (load/save/edit, per-tcode `[tcode.XXX]` sections, `[global]`, `[loop]`, `[sequence]`, `[inbond]`), `loop_config.rs` / `sequence_config.rs`. CLI flags always win over config (merged inside each `*Config::load`). Some flows write back to `config.toml` (e.g. confirmed layout names). `migrate_config.rs` at the root is a legacy-format migration helper.

**SAP abstraction.** `utils/sap_interfaces.rs` defines `SapSession` / `SapComponent` traits; `sap_real_impl.rs` wraps the COM objects, `sap_mock_impl.rs` is a recording mock (`MockSapSession`, `MockEvent`, `create_test_session`) used by tests. Note that most tcode flows take the concrete `&GuiSession` from `sap_scripting` directly, so only code written against the traits is mock-testable. The rest of `utils/` is shared SAP helpers: `sap_wnd_utils` (popups), `sap_ctrl_utils` (controls), `sap_tcode_utils`, layout/variant selection (`choose_layout`, `select_layout_utils`, `setup_layout*`), export + Excel post-processing (`sap_export_utils`, `excel_*` via calamine), `sap_crypto_utils` (AES-GCM credential storage), `keep_awake` (Windows power API).

## ABC headless work (in progress)

`src/abc/` (run config, date resolution) and `src/utils/export_outcome.rs` (`ExportOutcome`, outcome classifier,
paste-list helpers) are the start of a headless report runner driven by report name (rName, e.g. `mustgo`). Tcode steps
expose `run_export_outcome` (the bool `run_export` wraps it) and are meant to be config-agnostic: everything comes in
via the `*Params` struct, with CLI > config > default resolved by the caller. `src/abc_menu.rs` is the TUI submenu.
**Resume from `docs/abc_headless_plan.md` ("RESUME HERE")**; the spec is `docs/abc_headless_migration_spec.md`.

## Workflow notes

- Branches: work on `dev`; `public` is the release mirror, force-reset to `dev` by `sync_public.sh` / `sync_public.bat` (these rewrite and force-push `public` — never run without being asked).
- `docs/abc_headless_migration_spec.md` is a spec for re-implementing an Excel/VBA macro as a headless runner that uses this repo as the SAP automation reference; it is not documentation of existing code.
- Line endings are managed via `.gitattributes` (see README "Line Endings").
