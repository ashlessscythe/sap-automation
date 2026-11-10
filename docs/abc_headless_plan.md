# abc headless runner: work plan

Companion to `abc_headless_migration_spec.md` (the "spec"). Each chunk is sized for **one session** and is independent
once its dependencies are ticked. Tick the box and add a one-line note when a chunk is done.

**Session protocol (keeps usage low):** read `CLAUDE.md`, this file, and *only* the spec sections the chunk names.
Do not re-read the whole spec. One chunk per session; end by updating this file.

## RESUME HERE (state as of 2026-11-09)

Working mode: **step by step; Claude codes, the owner runs it against live SAP and brings back the real exports.**
Next up: report-name (rName) driven runner, starting with `mustgo` (spec 5.1, verified numbers in 9.3).

Done in code (all unit-tested, fmt/clippy/test green, **not yet smoke-tested on live SAP**):
* 4.0 foundation: `src/utils/export_outcome.rs` (`ExportOutcome`, `classify_messages`/`classify_screen`,
  `normalize_value`, `paste_value_list`); `run_export_outcome` on vt11/zvt11/vl06o/lx03/zmdesnr (bool `run_export`
  is a wrapper; y_149* NOT converted); ALV variant popup path in `variant_select`.
* 4.1 VT11 parity (code): new `VT11Params` fields `shipment_status`, `by_created_date`, `shipment_numbers` (K_TKNUM
  paste), `monday_include_sunday`, `ignore_display_error`, `date_format`; classifier runs right after execute.
  Not yet wired to config/CLI: `shipment_status`, `by_created_date`, `shipment_numbers`, `monday_include_sunday`.
* TUI: main menu entry "ABC Headless >" (index 30, Exit moved to 31) -> `src/abc_menu.rs` submenu (existing auto runs
  + LT23/ZMDETPC placeholders).
* 2.3 (partial): `src/abc/run.rs` has `week_start`/`week_range` (Excel WEEKNUM, Sunday start; week 45/2026 =
  2026-11-01..11-07) and `parse_run_date`. NOT yet done: JSON date objects, `diff`, per-day loop, folder/file naming.
* CLI: `--vt11-closed-date` (K_DATEN) / `--vt11-created-date` (K_ERDAT) (`WK45`, `WK45/2026`, `YYYY-MM-DD`, `A..B`; replaced the generic `--date`; per-tcode flags `--<tcode>-date` to follow), `--ignore-display-error`
  (VT11 only), `--rname-dir` (relative = under reports_dir, files at `{reports_dir}\<rname-dir>\{TCODE}`; absolute used
  as is; applied in `get_tcode_file_path`). Date typed into SAP follows `[global].date_format` / `--date-format`
  (VT11 only so far; ZVT11 still hardcodes `%m/%d/%Y`).

Owner to verify live (VT11, variant `XD_ALL_EPDC_`, `--vt11-closed-date=WK45`, ignore display error): that it runs, the raw
export header matches the `mustgo` sample (91 rows, 18 columns), `K_DATEN` gets the right dates, no false aborts from
the new post-execute classifier (status bar `E:`). Control IDs `K_ERDAT`, `K_STTRG`, `K_TKNUM` come from the VBA, not
a live SAP. Known caveats: Monday rule compares dates after the Sunday shift (Mon start==end becomes Sun..Mon);
`--rname-dir` only changes where exports are written, readers that look in `{reports_dir}\{tcode}` (e.g. VT11
deliveries-from-ZMDESNR) still read the old place.

Suggested next steps: (1) rName registry + run config for `mustgo` (2.3 remainder, stub SAP steps), (2) table core/ops
for the mustgo steps only (2.1/2.2 subset), (3) mustgo post-process vs golden data (3.1; needs 0.4 fixtures),
(4) real SAP steps under the config-agnostic rule: VL06O (4.2), ZMDESNR by deliveries/TOs (4.3), LT23 (4.4).
Drop the wasted history call in mustgo step 19 (spec item 10).

**Update 2026-11-10 (mustgo steps 1-2 done offline):** fixtures `tests/fixtures/mustgo/` (xlsx-derived, approximate),
`tests/mustgo_golden.rs`, `src/abc/mustgo.rs` + `tests/mustgo_pipeline.rs` (pure stages match spec 9.3; see "Design
notes"). **Blocked on real exports (owner), do not guess formats:** the raw text-with-tabs loader, runner, SAP adapters,
CLI/menu wiring. Needed, each via `export_type 1` (text with tabs): VT11 (variant `XD_ALL_EPDC_`, no layout), VL06O
(item view, no layout), ZMDESNR by deliveries (variant `mg_k2xx`, layout `mg_view`), LT23 (E01, confirmed, layout
`to_mg`), ZMDESNR by deliveries+TOs (same variant/layout). Same query as the 2026-11-05 xlsx exports.

Offline = no SAP needed (unit/golden tests only). SAP = needs a live SAP GUI to verify; code is written and mock-tested
offline.

---

## Design notes: `mustgo` pure stages (`src/abc/mustgo.rs`)

Stages (all pure over `Table`, columns by sanitised name, golden-tested in `tests/mustgo_pipeline.rs` on the fixtures in
`tests/fixtures/mustgo/`):
* `select_tpdc_shipments(vt11)` (row 10) -> distinct shipment list for VL06O; `delivery_list(vl06o)` -> list for ZMDESNR.
* `filter_sn_del(sn_del)` (rows 13-15) + `transfer_order_list` -> 1011 visible rows, 87 TOs (list for LT23).
* `lt23_postprocess(lt23)` (row 16 a-f) -> 15 rows x 13 cols, equal to `expected_lt23.tsv`.
* `mark_sn_del_by_lt23(sn_del, lt23)` (rows 17-18) -> 128 rows, 10 deliveries, 15 TOs; `delivery_list` + `transfer_order_list`
  give the inputs of the ZMDESNR by deliveries + TOs query. NOTE: the 476 of spec 9.3 is the size of that SAP result,
  i.e. the rows of the *unfiltered* sn_del (`snr_orig`) whose delivery and TO are in these lists, not of the deduped table.
* `snr_copy(sn_to, run_date)` (rows 19b-21) -> 92 rows x 21 cols (`week` at 19, `export_date` last), equal to
  `expected_snr_copy.tsv`; `pivot_plant_by_date` -> long table plant / export_date / count; `write_outputs` writes
  `lt23`, `snr_copy`, `pivot` TSV via `registry::output_path`.

Planned SAP-step seam (not implemented): a runner owns a `HashMap<String, Table>` and a trait/closure per SAP step, e.g.
`fn export(&mut self, step: Step, pasted: &[String]) -> Result<Table>` returning an already imported table
(`Table::from_raw`). The runner calls: vt11 -> `select_tpdc_shipments` -> vl06o(ships) -> `delivery_list` -> zmdesnr(dels)
-> `filter_sn_del` -> lt23(`transfer_order_list`) -> `lt23_postprocess` -> `mark_sn_del_by_lt23` -> zmdesnr(dels, TOs)
-> `snr_copy`/`pivot_plant_by_date` -> `write_outputs`. Tests then use a fake seam returning the fixture tables. The raw
export reader (SAP text/xlsx -> headers + rows) is separate and not written yet: the real raw format is unverified.

* Real exports: `src/abc/raw.rs` loads SAP text-with-tabs exports (observed format in `tests/fixtures/mustgo/README.txt`);
  short list-screen headers are mapped to canonical names by `*_ALIASES`; real data reproduces spec 9.3
  (`tests/mustgo_pipeline_real.rs`). Real VT11 free text is truncated to 14 characters.

## Phase 0: owner answers (blocking input)

- [x] 0.1 Questions answered: this crate; v1 = mustgo, xdock, toyota; defaults for items 7-18 accepted; relabel
      `GM55` reproduced without the leak (fixed schema).
- [ ] 0.2 Provide raw text-with-tabs exports: ZMDESNR, VT11, VL06O, ZVT11, LT23, ZMDETPC (same query as an xlsx export).
      **Can come from the Phase 4 smoke runs** (`export_type 1`), so not a blocker for starting Phase 4.
- [ ] 0.3 Provide a VT03N sample (what the CM20 pallets note text looks like).
- [ ] 0.4 Say where `abc/resources/*.zip` live and allow copying into `tests/fixtures/`.
- [ ] 0.5 `inb_rs.exe` TSV contract (columns, path argument), or declare it out of scope.

## Phase 1: verify (read-only, one short session)

- [x] 1.1 (done for tcode runners; findings in Phase 4) Check spec section 6 against the real source: for each tcode module list which controls/params actually exist
      (`vt11.rs`, `zvt11.rs`, `vl06o.rs`, `zmdesnr.rs`, `y_149*.rs`, `lx03.rs`).
      Also read `sap_export_utils.rs` (`export_local_file`: file path, header, types 0-4), `sap_tcode_utils.rs`
      (`variant_select`, paste helpers) and `unattended_runner.rs` (how auto flows get params).
      **Output:** a "verified gaps" table appended to section 6 of the spec. No code changes.

## Phase 2: offline foundation (no SAP)

Dependencies: 0.1 for naming/format decisions. Order is flexible except 2.4 needs 2.1.

- [ ] 2.1 **Table core** (`src/abc/table.rs`): TSV read/write (UTF-8, escaping), header sanitise + trim + blank-fill
      `customer`, value normalisation (strip leading zeros, no trailing `.0`, date/time formats), selection mask
      (hide/delete/reset/materialise). Spec: 1.3, 4 intro, 7.0 items 25-26. Done when: unit tests cover each rule.
- [ ] 2.2 **Table ops** (`src/abc/ops.rs`): filter (Excel criteria incl. wildcards), adv-filter, dedup (+visible),
      lookup, add_col, compare, regex, remove_one (fixed intent), copy, sort, merge-ZMDETPC, pivot long-form.
      Spec: 4.1-4.14, 7.0 items 9 and 11. Done when: unit tests per op, incl. the `remove_one` 1218-row case shape.
- [ ] 2.3 *(partial: week/range/date parsing done in `src/abc/run.rs`)* **Run config, dates, naming** (`src/abc/run.rs`): `get_json_date` (single / week range / start-end / diff),
      per-day loop, folder layout, `rname_overrides`, file naming `<date>_<name>_<table>.tsv`, collision suffix.
      Spec: 1.1, 1.4, 7.0 items 2 and 11. Done when: week 31/2026 -> 2026-07-26..08-01 and week 45 -> 11-01..11-07 tested.
- [ ] 2.4 **Fixture harness**: convert the xlsx outputs/raw files in the zips to golden TSV under `tests/fixtures/`
      (one-off script in the scratchpad, not the repo), plus a compare helper (row-exact diff with a readable report).
      Spec: section 9. Needs 0.4.

## Phase 3: offline report logic (pure functions: raw TSV tables in, output tables out)

Each chunk is independent, depends on 2.1-2.4, and is verified against the section 9 golden data. Order by value:

- [ ] 3.1 `mustgo` post-process + `snr_copy` + `week` + pivot (5.1, 9.3).
- [ ] 3.2 `xdock` / `toyota` / `misc` (5.2-5.4, 9.6): VL06O dedup, ZMDETPC merge rules, pivot, toyota trailers table.
- [ ] 3.3 `k2xx` (5.5, 9.5).
- [ ] 3.4 `relabel` -> `combined` (5.6, 9.5): effective-plant rule, explicit history flag (decision 35).
- [ ] 3.5 `repack` -> `snr_repack` (5.7, 9.5).
- [ ] 3.6 `nc_tsp`, `nc_masterpack`, `redtag`, `busbar`, `inb_*`, `out_*` (5.8-5.17, 9.4): filters from 8.1.
- [ ] 3.7 `add_del`: ZVT11 format (decisions 28-30), VT22 raw parser + `changes` logic, `split`, `pallets` (5.18, 9.2-9.3).
      VT03N output shape depends on 0.3.
- [ ] 3.8 `rma` (5.14): lookup + merge + no pivot.

## Phase 4: SAP tcode runners (code offline, verify live)

**Decided (2026-11-09):** the runner lives in this crate. v1 scope = `mustgo`, `xdock`, `toyota`, so v1 needs
VT11, VL06O, ZMDESNR (by deliveries, by deliveries+TOs, serial/history), LT23 and ZMDETPC (summarised). ZVT11 Closed,
ZMDESNR user-activity/by-date, VT22 and VT03N are v2. Defaults for review items 7-18 accepted. Relabel `GM55` is
reproduced as an effective-plant rule with no leak into output (fixed schema); revisit in chunk 3.4.

**Findings from Phase 1 (verified against source, 2026-11-09):**
* Every existing `run_export` returns `Result<bool>` and calls `export_local_file(...).is_ok()`, **discarding the
  exported file path**. The runner needs the path (or a no-data / error outcome). Existing menu flows must keep working.
* Existing runners take whole `*Params` structs built from config/CLI/prompts. New inputs are added as optional fields.
* **Rule (decided 2026-11-09): tcode steps are config-agnostic.** A tcode step takes everything it needs in its
  `*Params` (including `export_type`, `date_format`, value lists) and never calls `SapConfig::load()` or
  `cli_overrides()` itself. Precedence CLI flag > config > default is resolved once, by the caller (the report runner,
  or a module's `run_*_auto`), which builds the params. Existing runners still read config internally
  (`export_type` in vt11/zvt11/vl06o/zmdesnr/lx03, `date_format` in vt11, delivery sources in vt11, extra loads in
  vl06o); move those into params as each tcode is touched in 4.1-4.3.
* Control IDs present in source vs the spec:
  * VT11: has `K_DATEN-LOW/HIGH`, `K_TPBEZ-LOW/HIGH`, `S_VBELN` multi, layout menu `menu[3]/menu[0]/menu[1]`,
    export `menu[0]/menu[10]/menu[0]`, `MESSTXT1`. **Missing:** `K_STTRG`, `K_ERDAT`, `K_TKNUM` multi, status-bar
    `E:` / "no shipments" outcome classification, `monday_include_sunday`.
  * VL06O: has `BUTTON6`, `IT_VBELN`, `IT_TKNUM` multi, `IT_WADAT`, item view `btn[18]`, export `menu[0]/menu[5]/menu[1]`.
    Closest to done; needs outcome/path return, header-free distinct paste check, no-layout path.
  * ZMDESNR: has tab select `tabpUCOMM{n}`, tab 2 (`9002`) `S_VBELN` multi, `S_VBELN-LOW/HIGH`, `S_PALLTD`, `S_LGPLA`,
    `S_PARENT` + exclude tab `tabpNOSV`, `btn[33]` layout, `menu[0]/menu[3]/menu[1]` export. **Missing:** `S_TANUM`
    multi (TOs), the whole tab-1 serial flow (`9001`: `radP_CURRNT/HISTRY/OLDNEW`, `chkP_PALLET`, `S_SERIAL`), tab 7
    user activity, by-date `S_ERDAT`, `S_TKNUM`, `S_MBLNR`.
  * LT23, ZMDETPC, VT22, VT03N: nothing in source.
  * Shared helpers present: `assert_tcode`, `variant_select`, `exist_ctrl`, `hit_ctrl`, `paste_values_with_scroll`,
    `check_export_window`, `select_layout` / `choose_layout` / `setup_layout[_li]`, `export_local_file`.
* The tcode modules take the concrete `&GuiSession`, so they are **not** mock-testable. Put pure logic (date maths such
  as +-100 days, the Monday rule, id-string building, value normalisation for pastes) in small pure functions with unit
  tests; everything else is verified with a live smoke run.
* The live smoke runs double as the source of the raw text-with-tabs samples (Phase 0.2): `export_type 1` writes
  `{reports_dir}\{TCODE}\{timestamp}_{TCODE}.txt`. Keep those files and copy them to `tests/fixtures/raw/`.

Dependencies: none on Phase 2/3. 4.0 first; 4.1-4.5 are independent of each other after 4.0.

- [ ] 4.0 *(code done 2026-11-09, awaiting live smoke)* **Foundation.** (a) Add `ExportOutcome { File(PathBuf), NoData(String), Error(String) }`. (b) Make each
      existing `run_export` keep its signature and add `run_export_outcome` returning `ExportOutcome` (the bool version
      becomes a thin wrapper, so menus/sequence do not change). (c) Return the path from `export_local_file` through
      those calls. (d) Shared P6 outcome classifier (status bar `E:`, `no display authorization`,
      `ignore_display_error`, "no shipments" popup, `No data exists`). (e) ALV-variant popup path (`abap` title) in
      `variant_select` (P2). (f) Distinct, header-free, blank-free value list helper + one normaliser (strip leading
      zeros, decision 25) used before every paste (P3, decision 32). Spec: P1-P3, P6, 7.0 items 25 and 32.
      **Done when:** helpers unit-tested; `cargo clippy` clean; existing flows untouched.
- [ ] 4.1 *(code done 2026-11-09, awaiting live smoke; config/CLI wiring of the new fields pending)* **VT11 parity** (3.1, P6): `K_STTRG`/`K_ERDAT`, optional `K_TKNUM` paste, `monday_include_sunday`,
      `ignore_display_error`, no-layout export, single-date rule (`date1 == date2` clears `-HIGH`). Smoke run:
      variant `XD_ALL_EPDC_`, check outcome and raw header against the `mustgo` sample (91 rows, 18 default columns).
- [ ] 4.2 **VL06O parity** (3.3): confirm item view, layout menu path `menu[3]/menu[2]/menu[1]`, no-layout export,
      distinct delivery/shipment paste, outcome/path return. Smoke run with shipments from 4.1.
- [ ] 4.3 **ZMDESNR parity, v1 modes** (3.4, P2, P3): by deliveries with variant (no date field), by deliveries + TOs
      (`S_TANUM`, clear `S_PALLTD`/`S_VBELN` range, answer `wnd[2]` warnings), serial/history tab 1. Layout by name
      (`mg_view`) via `btn[33]`; keep `pre_export_back`. Smoke run both mustgo calls (5796 / 476 row shapes).
- [ ] 4.4 **LT23, new** (3.5): `lt23.rs` + `lt23_module.rs` following the pair pattern, `run_lt23_export_outcome`.
      Warehouse `E01`, TO multi paste, `radT1_QUITA`, layout via `btn[33]` + grid `cntlGRID` (else `btn[32]` +
      `setup_layout_li`), export `menu[0]/menu[1]/menu[1]`. Register in `lib.rs` and `main.rs`. No menu option in v1.
      Smoke run with TOs from 4.3 (expect the 87-TO shape).
- [ ] 4.5 **ZMDETPC, new, summarised only** (3.6): `zmdetpc.rs` + module; variant (P2), `S_DATE-LOW/HIGH`,
      `S_VBELN` paste with the non-numeric first-row delete (`btn[14]`) and `wnd[2]` answers, `radR_SUMM`/`radR_COMPL`
      refresh sequence, execute, `MESSTXT1` popup, export `menu[0]/menu[3]/menu[1]`. Output columns `delivery, pallet,
      child`. Detail flow and `pallet_pick_date` are v2.
- [ ] 4.6 *(v2)* ZVT11 Closed (3.2), ZMDESNR user-activity / by-date / exclude extras, VT22 export (3.7), VT03N read
      (3.8), ZMDETPC detail + `pallet_pick_date`.

Per chunk: code, pure-function unit tests, `cargo fmt/clippy/test`, a live smoke run with the exported raw file kept as
a fixture, then tick the box with the control IDs that needed fixing (the spec IDs come from the VBA, not from a
live SAP, so some may differ).

## Phase 5: orchestration

- [ ] 5.1 Report runners composing SAP step + table logic per report, tables persisting across blocks in one run
      (decision 15), error semantics (skip rest of block, non-zero exit), export cleanup, logging.
- [ ] 5.2 CLI/config: subcommand and flags for report list, date/range, per-day loop; TUI entry optional.
- [ ] 5.3 First end-to-end live run: `mustgo` (needs 3.1, 4.1, 4.3, 4.4, 5.1, 5.2). Then widen report by report.

## Phase 6: close-out

- [ ] 6.1 `inb_rs.exe` TSV reader (separate repo; per 0.5) and hand-off note.
- [ ] 6.2 Update `CLAUDE.md` and README with the new module layout and commands; resolve section 7 leftovers in the spec.

---

## Dependency summary

```
0.x --> 2.x --> 3.x --\
1.1 --> 4.x ----------+--> 5.x --> 6.x
```

Fastest vertical slice: 1.1 -> 2.1-2.4 -> 3.1 -> 4.1/4.3/4.4 -> 5.x for `mustgo`.
