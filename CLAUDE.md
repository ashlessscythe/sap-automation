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

**Multi-selection paste.** Every multi-value field goes through `utils::sap_ctrl_utils::paste_values_with_scroll[_kind]` (the popup
table is the generic `SAPLALDB:3010/tblSAPLALDBSINGLE`, the same for every tcode). Default = type the values row by row with
Page Down. `--use-clipboard` (`CliOverrides.use_clipboard`, read through `cli_overrides::use_clipboard()`, default off, accepts
bare / `true` / `1`) first puts the values on the Windows clipboard (`clipboard-win`) and sends **VKey 24** (Shift+F12, upload from
clipboard) to the popup window, then verifies row 0, the row count and the last entry (table scrolled via the table's own
`VerticalScrollbar` property: `GuiScrollbar` has no `Type`, so wrap it with `get_idispatch()`, never `downcast`); on any failure it
clears (VKey 16) and falls back to the row-by-row entry. `PasteKind::Numeric` (TOs in `lt23`/`zmdesnr`) strips non-digits, only
when the flag is on, to keep the legacy path unchanged. Confirmed working live on MM60. Closing the popup with `close()` raises a
"save changes?" popup; the paste paths confirm with VKey 8 instead.

## ABC headless work

`mustgo`, `xdock`, `toyota`, `misc`, `k2xx`, `relabel` and `repack` are runnable and **have been run live by the owner** (k2xx and relabel confirmed working 2026-11-11, repack 2026-11-11 after): `sap_automation --rname=<mustgo|xdock|toyota|misc> --run-date=<DATE|A..B|WKnn>`
(also TUI > ABC Headless; optional `--date-range-buffer=<n>`). Code is unit/golden tested offline (`mustgo` reproduces spec 9.3,
`xdock` spec 9.6 with documented differences to the xlsx). `k2xx` (v0.8.0) is golden-tested offline against the owner's **real exports and macro xlsx** of 2026-11-05 (`tests/fixtures/k2xx/real/`; VL06O/ZMDETPC merge, VT11 and ZVT11 equal the macro sheets) and has run live. Owner decision: `k2xx` also writes a plant pivot of `total` (departs from spec 7.0 item 1). `repack` (built 2026-11-11) is golden-tested offline against the owner's **real exports and macro xlsx** of 2026-11-05 (`tests/fixtures/repack/real/`, 280 rows equal). The
"Live verification checklist" sections in `docs/private/abc_headless_plan.md` list the SAP control ids that come from the spec only.
Exports are read as UTF-8, else Windows-1252 (`raw::decode_export`). Owner decisions and live findings: "RESUME HERE" in the plan.

Modules in `src/abc/`: `run` (date/week parsing), `registry` (rName registry, output paths), `table` + `ops` (in-memory table,
selection mask, filter/dedup/lookup/sort), `mustgo` (pure stages), `raw` (loader for SAP text-with-tabs exports; header
`*_ALIASES` map the short list titles to canonical names), `runner` (the `SapSteps` seam, `run_mustgo`, `NoDataError`),
`adapters` (`RealSapSteps` + `MustgoSapConfig` over the tcode steps), `dispatch` (CLI > `[abc.<report>]` > `[global]` resolver,
`run_report`), `inv` (pure stages of the INV reports: VL06O prepare, ZMDETPC merge, pivot, trailers, misc regex), `inv_runner`
(`InvSteps` seam of its own, `run_xdock` / `run_toyota` over one shared flow; `MiscSteps` + `run_misc` without ZMDETPC), `relabel` / `relabel_runner`, `repack` / `repack_runner` (see below). Also `src/lt23.rs` and `src/zmdetpc.rs` (new tcodes), `src/utils/export_outcome.rs` (`ExportOutcome`, outcome classifiers, paste-list
helpers) and `src/abc_menu.rs` (TUI submenu).

Rules: tcode steps expose `run_export_outcome` (the bool `run_export` wraps it) and are config-agnostic: everything comes in via
the `*Params` struct (new optional fields such as `export_target`; `None` keeps the legacy behaviour), with CLI > config >
default resolved by the caller. Columns are matched by sanitised header name, never by position. Any empty list, no-data or error
stops a report run and nothing is written.

Fixtures: `tests/fixtures/mustgo/` holds xlsx-derived TSVs (approximate stand-ins) and `real/`, the owner's exports (anonymised),
**byte-exact** (`-text` in `.gitattributes`: do not re-encode or normalise line endings). Tests: `mustgo_golden`,
`mustgo_pipeline`, `mustgo_pipeline_real`, `mustgo_runner` (a fake `SapSteps` serving the real fixtures). `xdock`:
`tests/fixtures/xdock/` (expected TSVs from the macro xlsx, `real/` byte-exact), `xdock_golden`, `xdock_runner`; `toyota`: `tests/fixtures/toyota/` (tiny: 3 deliveries, one plant), `toyota_golden`
(also synthetic edge cases), `toyota_runner`; `misc`: `tests/fixtures/misc/` (tiny), `misc_golden` (real data + edge cases through
`run_misc`); `k2xx`: `src/abc/k2xx.rs` + `k2xx_runner.rs` (`K2xxSteps` seam, `run_k2xx`; submodules are registered once in `src/abc/mod.rs`, not in lib.rs/main.rs), `tests/fixtures/k2xx/`
(`real/` byte-exact exports, `expected_real_*.tsv` from the macro xlsx, pivot computed; ZVT11 expected = macro sheet WITH spec 7.0 item 30 applied, `date` = `shipment_end_date`, a deliberate owner-approved deviation; an empty ZVT11 list does not stop k2xx, header-only output + warning), `k2xx_golden_real`, `k2xx_golden`, `k2xx_runner`. `relabel` (`abc/relabel.rs` pure stages, `relabel_runner.rs`: `RelabelSteps` seam, `run_relabel`; `--rname=relabel`, `[abc.relabel]`, single run date only)
re-reads the k2xx `vl06o` TSV of the same run (owner decision) and stops with "Run k2xx first" if it is missing/empty. Spec 9.5 wins over 5.6; the
macro's AL55 leaks, the extra `sn_del` row and `save full_label` are deliberately not reproduced; yellow is an explicit `yellow` column of `combined`.
`ZMDESNRParams.history_serials` + `tab_number = Some(1)` is the by-serial/History flow (spec-only control ids). Fixtures `tests/fixtures/relabel/`
are synthetic plus `real/` (byte-exact, anonymised); tests `relabel_golden`, `relabel_real`, `relabel_runner`.

`repack` (`abc/repack.rs` pure stages, `repack_runner.rs`: `RepackSteps` seam, `run_repack`; `--rname=repack`, `[abc.repack]` with the relabel plant keys plus `history_warehouse`, single run date only)
re-reads the k2xx `vl06o` TSV (deliveries) **and** relabel's `combined` TSV (`yellow = 1` rows = the parent exclusion list, only sent with at least two) of the same run and stops with "Run k2xx first" / "Run relabel first" if missing/empty. Flow: ZMDESNR by deliveries excluding parents
-> blocks R1 (non-label plant, no parent, net weight < 50), History rows (ZMDESNR by serial / History for R2 = the same with > 50; keep `history_warehouse` + `UNP`), R3 (`PAL`), KC*/SC* container deletion from those, R4 (label plant, no pallet, not `KC*`) -> `export_date` -> pivot count by customer x plant;
outputs `snr_repack`, `repack_sn`, `pivot`. Block order, no sorting, no alias marks or extra row (decisions 33-35). Fixtures `tests/fixtures/repack/` (`real/` byte-exact, anonymised; `expected_real_*` from the macro xlsx; stand-in `k2xx_vl06o_stand_in.tsv` and `relabel_combined_stand_in.tsv`; `synthetic/` edge cases),
tests `repack_real`, `repack_golden`, `repack_runner`. The parent-exclusion typing in `zmdesnr.rs` is one serial per visible row (no scroll paste); worked live with the real yellow serials.

**Resume from `docs/private/abc_headless_plan.md` ("RESUME HERE")**; the spec is `docs/private/abc_headless_migration_spec.md`.

## MM60 (LX03 material list)

`sap_automation --tcode mm60 --layout <L> [--variant <V>] [--export-type 1] [--lx03-file <slug|path>] [--rname-dir <D>]` has been **run live by the owner** (2026-11-11). Standalone tcode (`src/mm60.rs` + `mm60_module.rs`): it pastes the
`Material` column of the newest LX03 export (`.txt`/`.xlsx`, columns by header name; default dir = the LX03 export dir) into `MS_MATNR`, **clears** the plant selection, runs, picks the layout (grid flow, matched by `TEXT` then `VARIANT`),
and exports; no join, formulas or formatting (a future program does that). Dedupe only by (material, plant). Layout/variant/export type apply to MM60 only; omit export type for the Excel `.xlsx` export. Fixtures: `tests/fixtures/mm60/`
(anonymised, byte-exact), test `mm60_golden`. Prompts (e.g. "enter another layout name") only when `utils::cli_overrides::can_prompt()`: never in unattended runs (`--tcode`/loop/sequence/`--rname`), where a missing layout is an error.

## Data hygiene (no site data in the repo)

- **No real SAP data, names, plants, variants or layouts are committed.** Site-specific names come from the git-ignored `config.toml`
  (every one is a config key; `config.toml.example` shows them with placeholder values, and the built-in defaults are the same
  neutral placeholders such as `VT11_XDOCK_VARIANT`, `PL55`, `ALIAS_A*`). Relabel's plant codes and alias customer patterns
  (`label_plant`, `alias_plant`, `alias_customers`) and misc's `ship_point_marker` are config keys too (`RelabelRules`, `InvSelection`).
- Keep real values in the git-ignored `config.private.toml` / `config.toml` and `docs/private/` (the migration spec and the plan
  live there, local only). Never put them in code, tests, docs, commit messages or fixtures.
- Fixtures under `tests/fixtures/` are **anonymised** copies (consistent, shape- and order-preserving substitutions of ids, plants,
  names, users, ...). `tests/fixtures/relabel/real/alias_customers.txt` lists the anonymised customer names that the real-export
  relabel tests use as alias patterns. New fixtures must be anonymised before they are added.

## Workflow notes

- Branches: work on `dev`; `public` is the release mirror, force-reset to `dev` by `sync_public.sh` / `sync_public.bat` (these rewrite and force-push `public` — never run without being asked).
- `docs/private/abc_headless_migration_spec.md` is a spec for re-implementing an Excel/VBA macro as a headless runner that uses this repo as the SAP automation reference; it is not documentation of existing code.
- Line endings are managed via `.gitattributes` (see README "Line Endings").
