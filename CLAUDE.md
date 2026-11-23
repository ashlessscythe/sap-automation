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

### Per-day iteration, merge, xlsx output (built 2026-11-12, NOT yet run live by the owner)

`--rname=<r> --run-date=<A..B|WKnn> --loop [--merge]` and `--output-format=xlsx` (plan: `docs/private/abc_iteration_plan.md`). `--loop`/`--merge` are
unrelated to `--run-loop`/`--run-sequence`/`--iterations`. Modules (registered in `src/abc/mod.rs`): `iterate.rs` (`run_iteration[_with]`,
`DayRunner` seam so tests use a fake, `IterOpts`; `today` is captured **once** per iteration so all days share one output folder; policy:
`NoDataError` day skipped, other error stops with no merge, all days skipped = no merge/no error; merge gets done + skipped days),
`merge.rs` (`merge_report`/`merge_chain`: all output tables of each report in the chain, `Table::concat` by header name, `run_date` column in
ISO `yyyy-mm-dd`; per-day FILE names stay `MM-DD-YYYY`), `xlsx_out.rs` (`rust_xlsxwriter`, pinned; one sheet per table). Registry:
`chain`, `tables`, `workbook_path`, `merged_path` (`<DATE>_<RNAME>_MERGED.xlsx`). Tests: `merge_golden` (anonymised real fixtures as two days),
`iterate` unit tests (skip / stop / chain order), clap tests in `cli.rs`.
Tables named `pivot` (`merge::is_pivot_table`) are wide cross-tabs with day-dependent plant columns: merged with `Table::concat_union` (first table's columns, then new ones, blanks); all other tables keep the strict `Table::concat`. A failed merge (any report, or the whole merge) makes `run_iteration_with` print the full summary and then return `Err` ("N merge(s) failed"), so the process exits non-zero; good workbooks stay.

Weekly pivot (phase 6, `abc/weekly_pivot.rs`, registered in `abc/mod.rs`): `merge_report` appends a computed `weekly_pivot` sheet LAST (`MergeOutcome.tables` lists it, `MergeOutcome.notes` carries its size or the skip reason; a missing/empty source or column never fails the merge). `weekly_pivot_spec(rname)` = source table, plant column, measure: xdock/toyota/k2xx `vl06o` / `plant` / sum `total`; misc `vl06o` / `shipping_pointreceiving_pt` / sum `number_of_packages`; relabel `combined` / `plant` / sum `_of_labels`; repack `snr_repack` / `plant` / count non-blank `lm_serial_number`; mustgo `snr_copy` / `plant` / same count. Rows = `export_date` else ISO `run_date` (ISO ascending, else first appearance), columns plants A-Z with `(blank)` last, `0` for empty cells, `Total` row/column. Tests: `weekly_pivot` unit tests, `merge_golden`.

Phase 7 (built 2026-11-12, NOT yet run live): `--no-sap`, `--skip-existing`, `--output-date=YYYY-MM-DD` (TSV files only, no re-processing from raw exports; not in the TUI). `registry::output_complete(reports_dir, def, folder_date, &RunDate) -> Completeness{Complete, Missing(tables), None}` is the ONE completeness rule (ALL `registry::tables` present, header-only counts); `merge_report` uses it too. `DayRunner` gained `complete(report, &RunDate, today)` and `run` now takes `&RunDate`; `IterOpts{merge, skip_existing, no_sap}` (`from_overrides`). `--skip-existing`: complete day+report logged "exists, skipped" (`IterationSummary.existing`), not run, still done/merged; partial = rerun. `--no-sap`: `iterate_core` first checks every day x chain report (prerequisites too), one error listing all gaps before any merge, then only `merge` runs. Single runs: `run_single_with` (`SingleOutcome`). **No SAP session under `--no-sap`**: `main::run_unattended_rname` calls `unattended_runner::run_rname_entry` -> `iterate::run_rname_routed(&mut OfflineDayRunner, .., live)`; the `live` closure (keep-awake + `init_sap_connection` + `run_rname_unattended`) is invoked only if `needs_sap` (false for `--no-sap`; with `--skip-existing` false when everything is complete). `OfflineDayRunner::run` errors. `--output-date` = `cli_overrides::effective_today()` (used by `dispatch::run_report` and `run_iteration`; one capture per run). `--no-sap` + `--output-format=xlsx` is a validation error. clap: `--no-sap` conflicts with `--skip-existing`, all three require `--rname`. Tests: `iterate` unit tests (fake runner), `merge_golden` (temp reports dir under an arbitrary date folder, disk-backed runner), clap tests in `cli.rs`.

Phase 8 (built 2026-11-12, v0.16.0, NOT yet run live): `--with-prereqs` (bool, `requires = rname`, so it conflicts with `--run-loop`/`--run-sequence`; not in the TUI). `IterOpts.with_prereqs` / `cli_overrides::with_prereqs()` / in `summary_line`. A SINGLE run then covers `registry::chain(rname)` for that one `RunDate` via `iterate::run_single_chain`, which shares `run_day_chain` (the per-report policy of `iterate_core`: skip-existing, NoData cuts the rest of the chain, other errors hard) with the loop; outcome `SingleOutcome::NoData` is Ok (like a skipped loop day). Never runs reports downstream. `--no-sap`: every chain member must be `Complete`, one error lists each gap, else "ready". `needs_sap` checks the whole chain for single `--skip-existing`. With `--loop` the flag is ignored (the loop includes the prerequisites). Without the flag nothing changed. The `--help` text is guarded by `cli::tests::help_text_documents_with_prereqs`.

### Parallel SAP sessions (built 2026-11-12, run live by the owner: two reports and `--conn-count` over days)

All opt-in; without the flags nothing changes. `--multi-conn` (bool-ish, `requires = rname`) and `--conn-count=2..6` (`requires = loop`, conflicts `--no-sap`); `--rname` is a repeatable `Vec<String>` in `Cli` (a single use is unchanged; the process-wide `CliOverrides.rname` keeps the first, `Cli::rname` has the list; several need `--multi-conn` and can't be looped). `--test --tcodes=a,b` is a hidden spike (open a session per tcode).
Module `src/parallel_test.rs` (in lib.rs only, `main.rs` calls `sap_automation::parallel_test::...`): `run_jobs` creates one NEW session per job on connection 0 (`GuiSession::create_session`, ids diffed from the connection's children, 6-session limit via `check_limit`), runs each job on its own thread with its own `SAPComInstance` (COM proxies are not `Send`), closes the sessions it created and prints a table. `run_reports` (several rnames, `run_rname_unattended`) and `run_loop_parallel` (days of a `--loop` range, `--conn-count` or one per day capped by free slots). `iterate.rs`: `ParallelShared` (day queue, results, first hard error, abort flag), `parallel_worker`, `finish_parallel` (same policy as `iterate_core`: NoData skipped, hard error = no merge; merge once via `OfflineDayRunner`, `merge_tail` is shared with `iterate_core`). Tests: `iterate` parallel tests (fake runner), clap tests (`multi_conn_is_opt_in`, `conn_count_is_opt_in_and_validated`).
Status panel (v0.17.0, NOT yet run live): `logging.rs` writer thread also pins a grid under the scrolling output when stdout is a TTY (and `SAP_NO_STATUS` unset): `Panel` (rows per session tag: state, task, timer, latest log line; header: elapsed, `days d/t`), erased and redrawn once per batch (30 ms coalescing, synchronized-update frame, 1 s idle refresh). API: `logging::status` / `task` / `progress` (no-ops outside parallel runs). `iterate.rs` is compiled in the bin too, so it calls `sap_automation::logging::...` (`extern crate self as sap_automation;` in lib.rs).
Cross-cutting, all inert outside parallel runs: (1) per-thread session tag `sap_export_utils::set_session_tag` -> export names `<ts>-s3_<TCODE>.<ext>` (both `export_file_name` and `get_tcode_file_path`), (2) `sap_file_utils::normalize_export_dir` applied in `export_to_target` only when a tag is set (clean absolute `C:\a\b`, stops SAP's confirmation prompt), (3) `src/logging.rs`: crate-wide shadowing `println!`/`eprintln!` macros (declared first in lib.rs with `#[macro_use]`) that are plain std printing until `logging::start()`; then lines go through a channel to a buffered writer thread with a `[s3]` prefix, flushed when the guard drops (`print!` not covered; bin modules keep std printing, the parallel code runs in lib copies). `--use-clipboard` is refused (one shared Windows clipboard).

## LT03 / manual_to (`--tcode lt03`)

Port of the `manual_to` macro's LT03 flow (VBA in `C:\temp\repo\bas_files\manual_to`, `lt03_.bas`): shipments -> VL06O -> ZMDESNR by
material (`ZMDESNRParams.materials`, tab 2 `S_MATNR`) -> LT03 per delivery. Built 2026-11-13 (v0.18.0). **Status: run live by the owner, both `--dry-run` (VL06O, ZMDESNR by material, stock table read, planner, exit back to the start screen) and real transfer order creation (entering quantities, saving the TO, the save popup / TO status text).** Still do one delivery first on a new setup.

**v0.21.1 (run live by the owner 2026-11-18, TO created in SAP):** (1) the stock table columns (`RL03T-SELMG`, `LQUA-LGPLA`, `LQUA-VERME`, `LQUA-WDATU`) are resolved by field name from the first row's cell names (`RealLt03Screen::resolve_cols`, fallback to the old indices 0/2/4/9, map printed once per stock read as `LT03 stock table columns: ...`). (2) `parse_sap_date` treats a leading four-digit year as year-month-day whatever the pattern says: SAP showed `2026-11-14` while the config said `mm/dd/yyyy`, the old parser read 2026 as the month, every bin got `GR age cutoff: missing GR` and the material was skipped. (3) SAP date format resolution: `--lt03-sap-date-format` > `[abc.manual_to].sap_date_format` > global `--date-format` > `[global].date_format` > `yyyy-mm-dd` (in `--json` settings as `sap_date_format`). Diagnosing "GR age cutoff" skips: `log.tsv` `Bin Skipped` rows say `missing GR` (blank/unparseable date or wrong column) vs `older than <date>`.

**Two-pass entry (v0.22.0, `--lt03-two-pass`, `[abc.manual_to].two_pass`, built 2026-11-18, default off). Status: a `--dry-run` of this build was run live by the owner and works, but a dry run ignores the flag and enters nothing, so the checked entry (`enter_pick_strict`) and the verify re-read have NOT been run live. Do one delivery with `--lt03-two-pass` first and read `log.tsv` for `Pick Verified`.** `Lt03Config.two_pass`; in `lt03_runner::Run::material` the picks are sorted by `abs_row`, `Lt03Screen::begin_pick_pass` + `enter_pick_strict(&PickTarget)` (trait defaults map to the legacy `enter_pick`; real impl in `lt03.rs` checks bin / available / selected quantity against the read pass, reads the typed value back, never searches forward), then a second `read_stock` goes through `verify_selection`; `EnterOutcome::Mismatch` or a failed verify = `skip` (material cancelled via `skip_material`, red `Pick Pass Aborted` / `Pick Verification Failed`, nothing logged as `Bin Selection`). Unknown until live: whether SAP moves the viewport after Enter (the code re-reads the scrollbar top before every pick, so it copes either way). Dry run ignores the flag.

**VT11 display step (v0.22.3, built 2026-11-23, NOT run live):** opt-in `--lt03-show-vt11[=bool]` / `[abc.manual_to].show_vt11` plus `--lt03-vt11-variant` / `vt11_variant` (`Lt03Cli.show_vt11`/`vt11_variant`, `ManualToSettings`). `lt03_module::vt11_step_wanted` (flag on, variant set; dry runs included) gates `vt11::show_shipments` in `run_lt03_auto`, called before `finish_run` (JSON stays the last line; never changes the outcome; log action `VT11 Display`): VT11, variant, clipboard paste of the run's shipments into `K_TKNUM` (fallback row by row), execute, `tbar[1]/btn[30]` = tick all (port of the macro's `vt11_display_screen_info`, whose F7/hierarchy tail is NOT ported). Not in parallel runs. Default = unchanged behaviour.

Modules: `lt03_plan` (pure planner, no SAP/clock/console), `lt03_db` (stock DB / VL06O / shipment-list readers, columns matched by header
name ignoring underscores), `lt03_runner` (`Lt03Screen` seam + `run_delivery`, tested with a fake screen in `lt03_runner::fakes`), `lt03`
(`RealLt03Screen` over the GUI), `lt03_module` (settings, `ManualToSap`, `run_unit`, outputs, `run_lt03_auto`, `--show-last`),
`parallel_test::run_lt03_parallel` (shipment queue over `--conn-count` / `--multi-conn`), `tui::show_table_view` (dry-run viewer).

Planner rules (`lt03_plan`), in order: blocked bins are always excluded (`[abc.manual_to]` column `blocked` = X/TRUE marks the whole bin);
with fifo/lifo the GR age cutoff drops old and undated bins; ship-to (whitelist) bins before other ship-tos, which are only used when
`allow_relabel` is on and the ship-to's bins cannot cover the open quantity; **pallet size = the most common bin quantity of the
snapshot (all bins, ties -> larger)**; pass 1 whole bins that are a multiple of the pallet; then an exact bin, then several whole bins that
add up exactly to the rest (0/1 knapsack, `Ctx::whole_cover`), and only then a split of the smallest large-enough bin or the largest one
that fits. A pick is `[RELABEL]` when its bin is not on the ship-to whitelist and `[REPACK]` when its quantity is not a multiple of the
pallet. Deviation from the macro: a bin used in pass 1 is not offered again with its full stock. Known quirk kept from the macro: when
"smallest bin >= rest" has equal candidates a non-whitelist bin wins the tie.

Defaults (CLI > `[abc.manual_to]` > default): `fifo`, `age_cutoff` 30, `allow_relabel` true, `skip_blacklist` true,
`blacklist_material_policy` `allow_material_skip_bins`, `shipto_strategy` first, `proceed_without_gr` true (never asks), warehouse
placeholder. The effective settings are printed and written as the first `log.tsv` line (`Run Settings`) of every run.

Headless: never prompts, files in `<reports_dir>\MANUAL_TO\<date>\run_HHMMSS\` (v0.20.0: one folder per run, `ManualToSettings.run_id` set
in `settings_from_environment`, `run_dir(st)`; an empty `run_id` = the day folder, which is the pre-0.20 layout and what tests use;
`run_folders(day)` = day folder + `run_*`, used by `--lt03-resume` via `read_done_deliveries_day` and by `--show-last`) (`log.tsv`, `result.tsv`, `summary.json`, the VL06O / `to_sn` tables of
the run as `<unit>_vl06o.tsv` / `<unit>_to_sn.tsv`; `dryrun_` prefix for `--dry-run`), non-zero exit on failures (no data / no items to
pick are not failures). The VL06O step checks that SAP's shipment list starts with the first requested shipment (scrolled back to the
top first). One VL06O with all shipments is the default; `vl06o_per_shipment = true` runs it per shipment. The shipment column of the
files is filled when the VL06O layout has a `Shipment Number` column (or in per-shipment mode); otherwise it is blank.

`--dry-run` (interactive output, not a headless contract) drives the GUI to the stock overview, enters nothing, leaves with Cancel (F12)
until the start screen (delivery field present, `tabsFUNC_TABSTRIP` absent) is back, and shows a scrollable table (sticky header,
shipments line, status line; relabel yellow, repack magenta, skipped red; no shipment column on screen) before printing the plain table.
`--show-last` re-opens the last dry run from `dryrun_picks.tsv` / `dryrun_summary.json` without SAP.

HTML summary (built 2026-11-16, v0.20.0; `src/lt03_summary.rs`, registered in lib.rs AND main.rs): `finish_run` writes
`<prefix>summary.html` into the run folder after every run (best effort; opened with `cmd /C start` after a REAL run when stdin+stdout are
terminals, `SAP_NO_OPEN` disables). `--tcode lt03 --summarize[=YYYY-MM-DD]` (`Lt03Cli.summarize` / `summarize_date`, handled in `main.rs`
next to `--show-last`, before any SAP connection) rebuilds it from disk only: `list_runs(day_dir)` -> `RunRef{dir, dry_run}` (a run = a
folder with `<prefix>summary.json`; "summarized" = its `summary.html` exists), one run = written directly, several = list + `dialoguer`
pick when `can_prompt()`, else `pick_unattended` (all without a summary, else the newest). The page is pure `render_html(&RunData, generated)`
from `summary.json` (deliveries, picks; `gr_date` per pick since v0.20.0), `log.tsv` and every other `.tsv` of the same kind in the
folder (source tables); everything is HTML-escaped, CSS/JS inline, no external resources. Run live by the owner on the new `run_HHMMSS` folders (summary included) and tried on the old day-folder layout.

**API for the GUI (v0.21.0, `--json`, offline-tested, NOT run live):** the GUI wraps the executable (no separate service), so the CLI is the API; contract documented in COMMAND_LINE_USAGE.md ("Driving LT03 from a GUI or script"). `--json` (`Lt03Cli.json`): the LAST stdout line is one JSON object: `result_json(st, rep, dir, files)` (= `RunReport::summary_json` + `schema`, `version`, `run_id`, `run_dir`, `settings` (`settings_json`), `files`); `write_outputs_ctx` writes the same into `summary.json`; `--show-last` / `--summarize` print their own `kind` objects; errors print `{"ok":false,"error"}` via `lt03_module::print_json_error` called from `main`'s wrapper (`main` -> `real_main`; a `JSON_PRINTED` flag stops a second line after a result line). Every `[abc.manual_to]` setting now has a flag (`--shipto-strategy`, `--vl06o-per-shipment`, `--proceed-without-gr`, `--lt03-warehouse`, `--lt03-vl06o-*`, `--lt03-zmdesnr-*`), resolved CLI > config > default in `resolve_manual_to`. **Pitfall (bit `--show-last`/`--summarize` before 0.21.0):** `main.rs` stores the CLI overrides in the LIB's static, so bin-only code must reach `cli_overrides()`-reading code through `sap_automation::...` (the bin copy of `lt03_module` sees defaults). Keep the JSON shape additive (`schema` 1): a GUI depends on it.

Control ids come from the macro and a recording / screenshot (`wnd[0]` is a `GuiMainWindow`, keys go through it). `resources/*.xlsx` /
`*.txt` and `/reports/` are git-ignored (real data); anonymise before any fixture.

## MM60 (LX03 material list)

`sap_automation --tcode mm60 --layout <L> [--variant <V>] [--export-type 1] [--lx03-file <slug|path>] [--rname-dir <D>]` has been **run live by the owner** (2026-11-11). Standalone tcode (`src/mm60.rs` + `mm60_module.rs`): it pastes the
`Material` column of the newest LX03 export (`.txt`/`.xlsx`, columns by header name; default dir = the LX03 export dir) into `MS_MATNR`, **clears** the plant selection, runs, picks the layout (grid flow, matched by `TEXT` then `VARIANT`),
and exports; no join, formulas or formatting (a future program does that). Dedupe only by (material, plant). Layout/variant/export type apply to MM60 only; omit export type for the Excel `.xlsx` export. Fixtures: `tests/fixtures/mm60/`
(anonymised, byte-exact), test `mm60_golden`. Prompts (e.g. "enter another layout name") only when `utils::cli_overrides::can_prompt()`: never in unattended runs (`--tcode`/loop/sequence/`--rname`), where a missing layout is an error.

## Layout creation (`--create-layout`, built 2026-11-16, v0.19.0)

**Status (v0.19.2): run live by the owner 2026-11-16 on VL06O (list editor, item view, locked `Delivery`), ZVT11 (grid editor, k2xx `ZVT11 Closed` chooser with
auto-create inside a `--rname` run) and ZMDESNR (grid editor, blank-titled column). NOT run live: VT11, LT23, LX03/LX02, the 149 report, the VL06O header view. Control ids come from the
VBA macro and the owner's recordings** (`Control_id_.bas`: `check_select_layout` `setup:` branch,
`SetupLayout`, `SetupLayout_li`; read-only reference in `c:\temp\repo\bas_files\abc`). `sap_automation --tcode <T> --create-layout <NAME> [--overwrite]`
creates + saves an ALV layout on the CURRENT screen of the attached session (the report's result list must be displayed; it checks
`session.info().transaction()`); also the main-menu item "Create / save layout >" (`src/layout_menu.rs`, index 31, Exit moved to 32; not a sequence
option) and opt-in `--create-missing-layout` / `[global] create_missing_layout` (CLI > config > off).

Modules (all under `src/utils/`, registered once in `mod.rs`): `layout_store` (pure: `[layouts.<TCODE>.<NAME>]` with `columns` + `description`, case-insensitive
lookup, validation; `config_ops::save_to_path` keeps `[layouts.*]` through the preserved-sections writer and now also keeps non-string `[global]`
keys such as the boolean `create_missing_layout`; round-trip test), `layout_editors` (the column pickers behind the seams `ListScreen` / `GridScreen`
with fakes in `layout_editors::fakes`; `RealListScreen` over `GuiTableControl` re-finds the table on every access, scrollbar via
`GuiScrollbar::from(get_idispatch())`; `RealGridScreen` over two `GuiGridView`s), `layout_create` (profile table per tcode, `create_core` over the seam
`CreateGui`, `create_layout(session, tcode, &LayoutSpec, CreateOpts{overwrite})`, `apply_columns` (no save), `layout_exists_in_sap`, `auto_create_on_missing`).
`setup_layout` / `setup_layout_li` are now thin wrappers (legacy: skip missing columns, overwrite = yes). Rules: an existing layout is skipped unless
`--overwrite`; SAP's overwrite popup is answered No without it (`decide_overwrite`); a requested column SAP does not offer cancels the editor and saves
nothing; a column already displayed after clearing (key column that cannot be removed) counts as present; the name field is read back after typing (a name SAP
cut is an error); one creation at a time per process (`CREATE_LOCK`, existence re-checked under it); the session is always left on the result list.
Auto-create hooks (one helper `auto_create_on_missing`) sit in the not-found branches of `choose_layout` (generic, 149, zvt11), `select_layout_for`
(grid + list chooser, used by `check_select_layout`) and `lt23::select_layout`; when the feature is on but no section exists the not-found error says so.

VL06O view (2026-11-16, after the owner's first live try): the item view and the header view have different columns and separate layouts, so
`[layouts.VL06O.<NAME>]` takes `view = "item" | "header"` (`LayoutSpec.view`, `LayoutView`). `create_layout` detects the view on screen from the toggle
button label (`detect_vl06o_view`: `tbar[1]/btn[18]`, else a toolbar scan; the button names the OTHER view, `view_from_toggle_text`) and `check_view` makes a
mismatch an error, an undetectable view a warning. The label while the item view is shown is UNVERIFIED. Locked rows (owner: `Delivery` in the VL06O item view
cannot be moved out): `ListScreen::select_row` returns `false` for a non-selectable row, and when a bulk move-right moves nothing `clear_left` falls back to
`clear_one_by_one` (one row per press; a row still there after its own move is kept). A message popup after a move is confirmed with Enter.

ZVT11 (2026-11-16, owner recordings): the k2xx `ZVT11 Closed` chooser (`zvt11::select_closed_layout`) now pages the grid while reading names
(`sap_ctrl_utils::grid_column_values`: an ALV grid only serves rows near its visible window; also used by the existence check and LT23), lists the
names it saw when the layout is missing and calls `auto_create_on_missing` (it had no hook before). Grid editor per the owner's recording: a column is
added by scrolling to its row, selecting it and pressing `btnAPP_WL_SING` (was a double-click); save dialog ids confirmed by the recording. Run live by the owner.
Blank titles: an entry `""` in `columns` picks a column whose `SELTEXT` is blank (ZMDESNR's customer column); `normalize_columns` keeps blank entries (never
deduplicated, an all-blank list is invalid), messages show `(blank title)` (`show_column`); the list editor reports `""` as missing (a blank row ends its table).

Live checklist (use a THROWAWAY layout name, one grid tcode and one list tcode first): 1. grid tcode ZMDESNR or ZVT11: editor opens (`btn[32]` /
menu fallback), left grid cleared, right sorted by `SELTEXT`, columns land in order, save dialog (`btn[5]`, `wnd[2]` fields, overwrite popup `wnd[3]`),
editor closes; run `--overwrite` once and the same name without it (must say "exists, skipped"). 2. list tcode VT11 or VL06O: editor opens, clearing the left table
over several pages, sort button `btn%#AUTOTEXT002`, right-table scan and move, key columns (Shipment Number / Delivery) reported as present, Enter, menu save
dialog A (`chkG_FOR_USER`, `ctxtLTDX-VARIANT`), overwrite popup in `wnd[2]`. 3. LX03: save via `btn[36]`, dialog B, 12-character limit. 4. LT23 and LX02, the 149
report, `--create-missing-layout` in an export run. Unverified: the choose-layout dialogs of the existence check (grid ids, classic-list Find flow), the
`VARIANT` column name, whether row selection persists while scrolling the table control, `GetAbsoluteRow` for selection, the sort button, status texts.

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
