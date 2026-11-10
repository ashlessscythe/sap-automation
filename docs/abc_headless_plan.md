# abc headless runner: work plan

Companion to `abc_headless_migration_spec.md` (the "spec"). Each chunk is sized for **one session** and is independent
once its dependencies are ticked. Tick the box and add a one-line note when a chunk is done.

**Session protocol (keeps usage low):** read `CLAUDE.md`, this file, and *only* the spec sections the chunk names.
Do not re-read the whole spec. One chunk per session; end by updating this file.

## RESUME HERE (state as of 2026-11-10)

Working mode: **step by step; Claude codes, the owner runs it against live SAP and brings back the real exports.**
`mustgo`, `xdock`, `toyota` and `misc` are runnable end to end in code: `sap_automation --rname=mustgo --run-date=2026-11-05`,
`sap_automation --rname=xdock --run-date=2026-11-05`, `sap_automation --rname=toyota --run-date=2026-11-05`, `sap_automation --rname=misc --run-date=2026-11-05`. Everything that does not need SAP is unit/golden tested on the real
exports (`mustgo` reproduces spec 9.3; `xdock` spec 9.6, with the two documented differences to the macro xlsx, see
"Design notes: `xdock`"). **Nothing SAP-facing has been run live.** Next for the owner: the live verification checklists
below (mustgo, then xdock). Next for Claude: `k2xx` (3.3), then `relabel`, `repack`.

Done in code (fmt/clippy/test green; SAP parts not smoke-tested):
* Foundation (4.0): `src/utils/export_outcome.rs` (`ExportOutcome`, P6 classifier, `classify_after_execute`,
  `normalize_value`, `paste_value_list`); `run_export_outcome` on vt11/zvt11/vl06o/lx03/zmdesnr (`run_export` wraps it;
  y_149* not converted); ALV variant popup path in `variant_select`.
* Pure table layer: `src/abc/table.rs` (TSV, header sanitise, `customer` blank fill, normalisation, selection mask),
  `src/abc/ops.rs` (filter, adv_filter, dedup, lookup, add_col, morph_datetime, count_same, sort, remove_one, copy),
  `src/abc/run.rs` (week/range/date parsing), `src/abc/registry.rs` (rName registry, output paths).
* `src/abc/mustgo.rs`: pure stages (TPDC selection, delivery/TO lists, `filter_sn_del`, `lt23_postprocess`,
  `mark_sn_del_by_lt23`, `snr_copy`, `pivot_plant_by_date`, `write_outputs`).
* `src/abc/raw.rs`: loader for SAP "text with tabs" list exports, derived from the real files; header aliases map the short
  list-screen titles to the canonical (macro/xlsx) names; canonical names pass through, extra columns are ignored.
* `src/abc/runner.rs`: `SapSteps` seam (one method per SAP call, no history call), `run_mustgo`, `outcome_to_output`,
  `NoDataError`. Output is written only after the last step.
* `src/abc/adapters.rs`: `RealSapSteps` + `MustgoSapConfig` + pure `*_params` builders over the tcode steps; `src/lt23.rs`
  (new tcode, registered in lib.rs and main.rs); headless export plumbing (`ExportTarget`, `export_to_target`) in
  `sap_export_utils.rs`; new optional params on VT11/VL06O/ZMDESNR (`export_target`, ..., defaults keep the old behaviour).
* `src/abc/dispatch.rs`: resolver (CLI > `[abc.mustgo]` > `[global]` > defaults), `run_report`, summary and error text.
  CLI `--rname` / `--run-date`; TUI ABC Headless > "mustgo". No sequence menu id (the sequence runner has no run date).
* Fixtures: `tests/fixtures/mustgo/` (xlsx-derived TSVs + README) and `real/` (the owner's real exports, byte-exact,
  `-text` in `.gitattributes`). Tests: `mustgo_golden`, `mustgo_pipeline`, `mustgo_pipeline_real`, `mustgo_runner`.

Decisions made while building (all tested):
* **476 vs 128 (spec 9.3 step 17-19).** The 476 is the SAP result of the deliveries + TOs call, i.e. the rows of the
  *unfiltered* `sn_del` (`snr_orig`) whose delivery and TO are in the lists. The lists for that call come from `sn_del`
  after the row-18 deletion: 128 rows, 10 deliveries, 15 TOs (the same sets as the real 2a export), not from the 1011-row
  table (107 deliveries / 87 TOs).
* **`export_date`:** `yyyy-mm-dd` for a single date, `WK_<n>` for a range/week; one run per call, no per-day loop; VT11 gets
  the whole range.
* **Empty LT23 result = failure** (no delivery with two or more TOs): the run stops with "NO DATA at step 'lt23
  post-process'"; any empty list or no-data/error stops the block and writes nothing.
* **Blank plant** in the pivot is labelled `(blank)` (Excel label); blank `lm_serial_number` rows are not counted.
* **Headless export plumbing:** `export_target: Some(..)` switches a tcode to a local text-with-tabs export
  (`<export_dir>\<TCODE>\<yyyymmddhhmmss>_<TCODE>.txt`), no config reads, no Excel fallback, file must exist and be
  non-empty; the "save list in file" dialog title is checked. `None` = legacy behaviour.
* **Alias table** (`raw::*_ALIASES`): real list headers (`TPPt`, `Parent no`, `Plnt`, `Cust. Part`, `TO Number`,
  `Source Bin`, ...) -> canonical names. The real VT11 free text is truncated to 14 characters (unused by mustgo).
* ZMDESNR by deliveries uses `scroll_paste` (the legacy per-cell loop only fills the visible dialog rows); a missing layout
  is a step error when headless; LT23 creates a missing layout only if `lt23_create_layout = true`.

* **xdock (3.2, 4.5, 5.1 for xdock):** `src/zmdetpc.rs` (new tcode, summarised only, registered in lib.rs and main.rs),
  `abc/inv_runner.rs` (`InvSteps` seam of its own so the mustgo seam is untouched, `run_xdock`), `RealSapSteps` implements both
  seams, `dispatch` resolves `[abc.xdock]` (`vt11_variant`, `vt11_layout`, `vl06o_layout`, `zmdetpc_variant`, `delete_exports`,
  `ignore_display_error`), registry `XDOCK` (folder `INV`), ABC menu entry. `raw.rs`: `zmdetpc` and `vt11_inv` alias sets,
  VL06O aliases `gm`, `plnt`, and a header-detection fix for preambles of equally wide lines (ZMDETPC). Fixtures:
  `tests/fixtures/xdock/` (+ `real/`, byte-exact, `-text`). Tests: `xdock_golden`, `xdock_runner`.

* **toyota (3.2):** `inv_runner::run_toyota` shares the xdock flow (`run_inv`, only the report definition and the trailers
  table differ), registry `TOYOTA` (folder `INV`), `[abc.toyota]` (same keys as xdock, `vt11_variant` default `FG_TOYO_XD`),
  ABC menu entry. Outputs `vt11`, `vl06o`, `pivot`, `trailers` as TSV under `...\INV\<today>\toyota\`. Tests: `toyota_golden`
  (real tiny export + xlsx, synthetic edge cases), `toyota_runner`.

* **misc (3.2):** `inv_runner::run_misc` + its own `MiscSteps` seam (VT11, VL06O only; xdock/toyota `InvSteps` untouched,
  `RealSapSteps` implements both), registry `MISC` (folder `INV`), `[abc.misc]` (`vt11_variant` `MISC_EPDC_`, `vt11_layout`,
  `vl06o_layout` `inv`), ABC menu entry, `raw::VL06O_ALIASES` `shpt` -> `shipping_pointreceiving_pt`. Tests: `misc_golden`.

Suggested next steps: (1) owner runs the checklists below; fix control ids from the findings, keep the exports as fixtures;
(2) `k2xx`, `relabel`, `repack` (3.3-3.5); (3) the v2 list (ZVT11 Closed, VT22, VT03N);
(4) per-day loop and `rname_overrides` if wanted (2.3 remainder).

Offline = no SAP needed (unit/golden tests only). SAP = needs a live SAP GUI to verify.

---

## Live verification checklist (mustgo)

Owner, in this order. "Spec" = id taken from the VBA spec only, not seen in this repo's code. Exports are text with tabs
(export type 1) at `<reports_dir>\<TCODE>\<yyyymmddhhmmss>_<TCODE>.txt` (e.g. `reports\VT11\20261110101550_VT11.txt`); they
are deleted after a successful load unless `[abc.mustgo] delete_exports = false` (a file that failed to load is kept).
Outputs: `<reports_dir>\K2XX\<today>\mustgo\<date>_MUSTGO_{LT23,SNR_COPY,PIVOT}.tsv`.

**A. Pre-flight (no SAP actions)**
1. `[global].date_format` equals the SAP user's date format (the same text is typed into VT11 `K_DATEN`; the default is
   `mm/dd/yyyy`). The observed exports show ISO dates, which pass through unchanged.
2. Variants and layouts exist in SAP: VT11 variant `XD_ALL_EPDC_`; ZMDESNR variant `mg_k2xx` and layout `mg_view` (a missing
   layout is a step error; the existing layout code can also prompt on the console); LT23 layout `to_mg`. Override names in
   `[abc.mustgo]` if they differ.
3. Layouts deliver the canonical columns (a missing one stops the run naming it, extra columns are ignored): vt11
   `shipment_number`, `transportplanningpt`; vl06o `delivery`; zmdesnr by deliveries `delivery`, `transfer_order_number`,
   `parent_serial_number`, `status`; lt23 `delivery`, `transfer_order_number`, `creation_date`, `creation_time`,
   `source_storage_bin`; zmdesnr by deliveries+TOs `lm_serial_number`, `parent_serial_number`, `plant`, `created_on`
   (`week` is inserted at position 19, so use the 19-column `mg_view`).
4. SAP GUI scripting enabled, logged in, one session open; the first runs use `--keep-awake` off and `delete_exports = false`.

**B. First run: single known date**
`sap_automation --rname=mustgo --run-date=2026-11-05`. Expected on the data of the verification run (counts differ if SAP data
changed since): 54 TPDC shipments, 107 deliveries, 87 TOs, `lt23` 15 rows, `snr_copy` 92 rows, pivot FV52 2, FV55 30, FV56 46,
FV59 6, FW61 5, FW65 3 (total 92); the step-19 call receives 10 deliveries and 15 TOs. Compare the five export files with
`tests/fixtures/mustgo/real/` (VT11 91 rows, VL06O 834, ZMDESNR 5796, LT23 1071, ZMDESNR 2a 476) and the outputs with
`tests/fixtures/mustgo/expected_*.tsv`. A failure message names the step; "NO DATA at step ..." is a clean no-data, anything
else is `ERROR: ...`. Exit code is non-zero for both.

**C. Controls, per step (check in the order the steps run; each is a likely first failure)**
* Common: the local-file menu paths (VT11 `menu[0]/menu[9]/menu[2]`, then `[9]/[0]`, `[3]/[2]`, `[3]/[0]`; VL06O
  `menu[0]/menu[5]/menu[2]`, then `[5]/[0]`, `[3]/[2]`, `[3]/[0]`; ZMDESNR `menu[0]/menu[3]/menu[2]`; LT23
  `menu[0]/menu[1]/menu[2]`, spec) are accepted only if the dialog is titled "save list in file" (case-insensitive); radio
  `radSPOPLI-SELFLAG[1,0]`, save dialog `ctxtDY_PATH` / `ctxtDY_FILENAME`; the file must appear within 15 s.
* "No list after execute" detection (`classify_after_execute`): the selection screen must keep VL06O
  `btn%_IT_TKNUM_%_APP_%-VALU_PUSH`, ZMDESNR `tabsTABSTRIP_TABB1`, LT23 `ctxtT1_LGNUM` when SAP finds nothing; message wording
  unknown. Test with a date that has no data.
* VT11: `K_DATEN-LOW/HIGH` get the run date (HIGH cleared for a single date), variant applied, no layout; export 91 rows.
* VL06O: `btnBUTTON6`, `IT_WADAT` cleared, shipments pasted with scrolling (cells `ctxt`), item view `btn[18]` is the default
  view of the export (834 rows, `Item` column present).
* ZMDESNR by deliveries: tab `tabpUCOMM2`, variant via `btn[17]`, all 107 deliveries arrive (`scroll_paste`, cells `txt`),
  layout via `menu[4]/menu[0]/menu[1]` + grid. The legacy code presses execute twice; unchanged.
* ZMDESNR by deliveries + TOs (spec): `btn%_S_TANUM_%_APP_%-VALU_PUSH` under `.../tabpUCOMM2/ssub%_SUBSCREEN_TABB1:ZMDE_SERIALNUMBER_HISTORY:9002/`,
  value cells `txt` or `ctxt` (both tried), `wnd[2]/usr/btnBUTTON_1` warnings answered (up to 5), row-0 check, close with F8.
* LT23 (all spec; the `/usr/` prefix is assumed): `ctxtT1_LGNUM` (E01), `btn%_T1_TANUM_%_APP_%-VALU_PUSH`, `radT1_QUITA`,
  execute `btn[8]`; no default date/status limit on the selection screen hides TOs; layout `to_mg` via `btn[33]` and grid
  `wnd[1]/usr/cntlGRID/shellcont/shell` (column `VARIANT`). Layout creation (`lt23_create_layout = true`: `btn[32]` +
  `setup_layout_li` on `/usr/tabsTS_LINES/tabpLI01/ssubSUB810:SAPLSKBH:0810`, not saved) has never been run; the header
  titles of the real LT23 export are the short list titles, not the spec's long names.

**D. Then**
Run a week (`--run-date=WK45`, one run, `export_date` = `WK_45`), then keep a failing export as a new fixture. Report the
control ids that needed changes so they can be fixed in code and recorded here.

---

## Live verification checklist (xdock)

Owner, after the mustgo checklist (A.1 and A.4 of it apply unchanged). Exports are text with tabs at
`<reports_dir>\<TCODE>\<yyyymmddhhmmss>_<TCODE>.txt` for VT11, VL06O and ZMDETPC; deleted after a successful load unless
`[abc.xdock] delete_exports = false`. Outputs: `<reports_dir>\INV\<today>\xdock\<date>_XDOCK_{VT11,VL06O,PIVOT}.tsv`.

**A. Pre-flight.** Variants/layouts exist in SAP: VT11 variant `XD_ALL_EPDC_` and layout `scorecard`; VL06O layout `xdock`;
ZMDETPC variant `EPDC_TONY` (**taken from the toyota row, spec 5.2 row 28; spec 5.3 only says "same as toyota"**, so confirm it
is the variant the xdock macro uses and set `[abc.xdock] zmdetpc_variant` if not). Layouts must deliver the canonical columns
(a missing one stops the run naming it): vt11 `shipment_number`; vl06o `delivery`, `number_of_packages`, `plant` (plus
`shipto_party`, `name_of_the_shipto_party`, `external_delivery_id`, `total_gds_mvt_stat` for the output); zmdetpc `delivery`,
`pallet`, `child`.

**B. First run: single known date.** `sap_automation --rname=xdock --run-date=2026-11-05`. Expected on the data of the
verification run (counts differ if SAP data changed): VT11 91 shipments, VL06O 1068 item rows = 148 deliveries, ZMDETPC 140
rows (8 deliveries without one), pivot 13 rows (12 plants + `(blank)`), total 1493. Compare the three export files with
`tests/fixtures/xdock/real/` and the outputs with `tests/fixtures/xdock/expected_*.tsv` (differences listed in the design notes).

**C. Controls, per step.** All ZMDETPC ids are from the VBA spec only (nothing in this repo confirms them), in the order the
step uses them:
* Transaction start (`assert_tcode ZMDETPC`); variant via the shared `variant_select` (`btn[17]`, popup `txtV-LOW`; the ALV popup
  path if the title contains `abap`); a missing variant is a step error.
* `wnd[0]/usr/ctxtS_DATE-LOW`, `ctxtS_DATE-HIGH` (typed in the `[global].date_format` / `--date-format`; HIGH is always filled,
  repeated for a single date).
* `wnd[0]/usr/btn%_S_VBELN_%_APP_%-VALU_PUSH`, then in `wnd[1]`: clear `sendVKey 16`, paste cells `ctxt` or `txt`
  (`tabsTAB_STRIP/tabpSIVA/ssubSCREEN_HEADER:SAPLALDB:3010/tblSAPLALDBSINGLE`), `wnd[2]/usr/btnBUTTON_1` answered up to 5 times,
  row 0 non-empty, close with `sendVKey 8`. The spec's "delete a non-numeric first row (`wnd[1]/tbar[0]/btn[14]`)" is not
  implemented (no header row is pasted).
* `radR_SUMM`, `ctxtS_DATE-HIGH` again, `radR_COMPL`, `radR_SUMM` (the macro's refresh sequence), execute `wnd[0]/tbar[1]/btn[8]`.
* "No list after execute" detection: the selection screen must keep `/usr/radR_SUMM` when SAP finds nothing; the popup text in
  `wnd[1]/usr/txtMESSTXT1` is read by the shared classifier. Test with a date that has no pallets.
* Export menu `wnd[0]/mbar/menu[0]/menu[3]/menu[1]` (`menu[2]` as a second candidate), accepted only if the dialog is titled
  "save list in file"; then radio `radSPOPLI-SELFLAG[1,0]` and the shared save dialog.
* VT11 layout `scorecard` via the existing layout path (a missing layout is only logged by VT11 and the export goes ahead with
  the current layout: check the VT11 export has the 13 columns). VL06O layout `xdock` via the existing `check_select_layout`
  (same caveat, nothing fails when it is missing; check the 7 columns).
* The real list headers map through `raw::VT11_INV_ALIASES`, `VL06O_ALIASES` (`GM`, `Plnt`, `No.Pk`, `Ship-to`) and the
  canonical ZMDETPC titles; free-text VT11 columns (`seal_number_2`, `descripof_shipment`, `forwarding_agent_tracking_id`) are
  truncated to 14 characters by the list width.

**D. Then** run a week (`--run-date=WK45`: VT11 and ZMDETPC get the whole range, the `date` column is `WK_45`), and keep a
failing export as a new fixture.

---

## Live verification checklist (toyota)

As the xdock checklist (A.1, A.4 of the mustgo one and everything under xdock C apply unchanged: same VL06O layout `xdock`,
same ZMDETPC step and variant). Differences: VT11 variant `FG_TOYO_XD` (layout `scorecard`; override in `[abc.toyota]`), run
`sap_automation --rname=toyota --run-date=2026-11-05`, outputs under `<reports_dir>\INV\<today>\toyota\<date>_TOYOTA_{VT11,VL06O,PIVOT,TRAILERS}.tsv`.
Expected on the verification data (tiny): 3 shipments, 3 deliveries, 3 ZMDETPC rows, pivot `FV55` 4, trailers `FV55` 0.
Compare with `tests/fixtures/toyota/`. No new control ids.

## Live verification checklist (misc)

As the xdock checklist, minus ZMDETPC (misc has no ZMDETPC step). New unverified items, only these: VT11 variant `MISC_EPDC_`
with layout `scorecard`, and VL06O layout `inv` (header `ShPt` = shipping point; a missing layout is only logged by the
existing layout code, so check the export has the 7 columns incl. `ShPt` and `No.Pk`: a missing `shipping_pointreceiving_pt`
column stops the run naming it). Run `sap_automation --rname=misc --run-date=2026-11-05`; outputs under
`<reports_dir>\INV\<today>\misc\<date>_MISC_{VT11,VL06O,PIVOT}.tsv`. Expected on the verification data: 3 shipments, 3
deliveries, pivot `FV55` 1, `FV59` 1. **The `EPDC` replacement has never been seen on real data** (the export has no `EPDC`
shipping point): when a run has such rows, compare the pivot labels with the macro's.

## Design notes: `misc`

Flow (`run_misc`, spec 5.4): VT11 -> distinct shipments -> VL06O (layout `inv`) -> `prepare_vl06o` (first row per delivery,
`date`) -> `inv::misc_ship_to_regex` -> `pivot_sum(shipping_pointreceiving_pt, number_of_packages)`. Pivot shape equals xdock's:
long table, columns named after the source columns, label order case-insensitive ascending (`(` sorts before letters), `(blank)`
last, labels differing only in case are one group (as in an Excel pivot), no grand total. **Differences to
`11-05-2026_MISC.xlsx` (asserted in `tests/misc_golden.rs`):** (1) `number_of_packages` of delivery 85623472 is blank in the text
export and 0 in the xlsx (counts as 0 either way); (2) pivot header `Row Labels` / `Sum of number_of_packages` and a `Grand
Total` row in the xlsx; (3) the 14-character list truncation and Excel dates in the VT11 sheet. Everything else is identical.
Edge cases tested: `EPDC` with and without parentheses (the first `(...)` group, parentheses kept; the match is case sensitive),
rows without `EPDC`, blank shipping point, dedup by delivery (first row), empty VT11 / VL06O, missing column.

## Design notes: `toyota`

Same flow and stages as xdock (see below) plus `inv::trailers(pivot)`: `plant`, `trailers` = `ROUND(total / 14.5)` (halves away
from zero) for every pivot row, `(blank)` included. **Differences to `11-05-2026_TOYOTA.xlsx` (asserted in
`tests/toyota_golden.rs`):** (1) the xlsx pivot has the header `Row Labels` / `Sum of total` and a `Grand Total` row, ours is
the long table `plant`, `total`; (2) the macro's trailer cells E/F sit on the pivot sheet and contain artefacts: E1/F1 =
`Row Labels` / 0 (the header cell of column B went through the formulas) and E3 = `Grand Total` with a blank F; ours is a
separate table with the plant rows only (`FV55` 0 = F2); (3) as in xdock, the sheet's trailing columns M:O repeat
`delivery_`/`pallet`/`child` (equal to I:K here because every delivery has a ZMDETPC row), and Excel dates / the 14 character
list truncation differ cosmetically. The first 12 columns of the vl06o sheet and the VT11 sheet are otherwise identical; the
spec-vs-xlsx `total` question of xdock does not arise (no delivery lacks a ZMDETPC row). Edge cases covered by tests:
single plant, several plants with blank plant, deliveries without ZMDETPC row, an empty ZMDETPC table, `0 -> 1`, rounding.

## Design notes: `xdock` (`src/abc/inv.rs`, `inv_runner.rs`, `zmdetpc.rs`)

Flow (`run_xdock`, spec 5.3 = toyota rows 25-30): VT11 (variant, layout) -> distinct `shipment_number` -> VL06O (layout `xdock`,
item rows) -> `prepare_vl06o` (first row per delivery, `date` column = `yyyy-mm-dd` or `WK_<n>`) -> distinct deliveries ->
ZMDETPC summarised (variant, run date) -> `merge_zmdetpc` -> `pivot_sum(plant, total)` -> outputs. The ZVT11 Closed step is
dropped (item 10). Seam: `inv_runner::InvSteps` (`vt11`, `vl06o_by_shipments`, `zmdetpc_summarised`); the variant/layout names
travel in `InvSelection` so a fake seam can record them. An empty shipment or delivery list, SAP no-data or any error stops the
run; nothing is written.

**Differences between our output and `11-05-2026_XDOCK.xlsx` (all asserted in `tests/xdock_golden.rs`):**
1. The xlsx sheet has 3 extra trailing columns (`delivery_`, `pallet`, `child` again, M:O): leftovers of the macro's positional
   append. Not data; our table ends at `total`.
2. **For the 8 deliveries without a ZMDETPC row the xlsx has `total` = 0**, ours has `number_of_packages` (spec 4.12 step 5 and
   9.6). In the xlsx `total` is the formula `SUM(pallet:child)` = 0, not blank, so the macro's "blank total takes
   number_of_packages" did not fire, and the `0 -> 1` replace did not touch the formula cells either. Spec 9.6 states the
   fallback and a Grand Total of 1493; **the xlsx we were given has 1328 and `(blank)` 0** (ours: 1493, `(blank)` 4; plants
   `FV33` +1 etc.). We keep the spec rule. If the macro result must be reproduced instead, `merge_zmdetpc` has to leave `total`
   at 0 for those rows (decision for the owner).
3. The xlsx pivot has a `Grand Total` row and the header `Row Labels` / `Sum of total`; ours is a long table `plant`, `total`
   without a grand total.
4. Cosmetic: the xlsx stores dates as Excel dates (`actualshipmentend`, `date`), and turned the date-only `descripof_shipment`
   texts into dates; the converter wrote them back as `yyyy-mm-dd` / `MM/DD/YYYY` text. The real list truncates three VT11 text
   columns to 14 characters (our value is a prefix of the xlsx one).
Everything else (148 rows, order, plants incl. the first-row rule `85614256`, `85650066`, `8012734818`) is identical.

## Design notes: `mustgo` stages (`src/abc/mustgo.rs`, `runner.rs`)

Stages (pure over `Table`, columns by sanitised name; golden-tested on the fixtures in `tests/fixtures/mustgo/`):
* `select_tpdc_shipments(vt11)` (row 10) -> distinct shipment list for VL06O; `delivery_list(vl06o)` -> list for ZMDESNR.
* `filter_sn_del(sn_del)` (rows 13-15) + `transfer_order_list` -> 1011 visible rows, 87 TOs (list for LT23).
* `lt23_postprocess(lt23)` (row 16 a-f) -> 15 rows x 13 cols, equal to `expected_lt23.tsv`.
* `mark_sn_del_by_lt23(sn_del, lt23)` (rows 17-18) -> 128 rows, 10 deliveries, 15 TOs (inputs of step 19).
* `snr_copy(sn_to, run_date)` (rows 19b-21) -> 92 rows x 21 cols (`week` at 19, `export_date` last), equal to
  `expected_snr_copy.tsv`; `pivot_plant_by_date` -> long table plant / export_date / count; `write_outputs` writes
  `lt23`, `snr_copy`, `pivot` TSV via `registry::output_path`.

SAP-step seam: `runner::SapSteps` has one method per call (`vt11`, `vl06o`, `zmdesnr_by_deliveries`, `lt23`,
`zmdesnr_by_deliveries_and_tos`), each returning `StepOutput::{Table, NoData}` with an already loaded table; `RealSapSteps`
implements it over the tcode steps, tests use a fake that serves the real fixtures and records its inputs. A new report adds
its stage module, a runner over the same seam, and an entry in `dispatch::runnable_reports`.

## Phase 0: owner answers (blocking input)

- [x] 0.1 Questions answered: this crate; v1 = mustgo, xdock, toyota; defaults for items 7-18 accepted; relabel
      `GM55` reproduced without the leak (fixed schema).
- [ ] 0.2 *(mustgo set received 2026-11-10: VT11, VL06O, ZMDESNR x2, LT23, in `tests/fixtures/mustgo/real/`)* Provide raw text-with-tabs exports: ZMDESNR, VT11, VL06O, ZVT11, LT23, ZMDETPC (same query as an xlsx export).
      **Can come from the Phase 4 smoke runs** (`export_type 1`), so not a blocker for starting Phase 4.
- [ ] 0.3 Provide a VT03N sample (what the CM20 pallets note text looks like).
- [x] 0.4 *(mustgo only: `resources/mustgo.zip`, `mustgo_tsv.zip` -> `tests/fixtures/mustgo/`)* Say where `abc/resources/*.zip` live and allow copying into `tests/fixtures/`.
- [ ] 0.5 `inb_rs.exe` TSV contract (columns, path argument), or declare it out of scope.

## Phase 1: verify (read-only, one short session)

- [x] 1.1 (done for tcode runners; findings in Phase 4) Check spec section 6 against the real source: for each tcode module list which controls/params actually exist
      (`vt11.rs`, `zvt11.rs`, `vl06o.rs`, `zmdesnr.rs`, `y_149*.rs`, `lx03.rs`).
      Also read `sap_export_utils.rs` (`export_local_file`: file path, header, types 0-4), `sap_tcode_utils.rs`
      (`variant_select`, paste helpers) and `unattended_runner.rs` (how auto flows get params).
      **Output:** a "verified gaps" table appended to section 6 of the spec. No code changes.

## Phase 2: offline foundation (no SAP)

Dependencies: 0.1 for naming/format decisions. Order is flexible except 2.4 needs 2.1.

- [x] 2.1 *(done)* **Table core** (`src/abc/table.rs`): TSV read/write (UTF-8, escaping), header sanitise + trim + blank-fill
      `customer`, value normalisation (strip leading zeros, no trailing `.0`, date/time formats), selection mask
      (hide/delete/reset/materialise). Spec: 1.3, 4 intro, 7.0 items 25-26. Done when: unit tests cover each rule.
- [x] 2.2 *(subset used by mustgo done; compare/regex/merge-ZMDETPC/generic pivot pending)* **Table ops** (`src/abc/ops.rs`): filter (Excel criteria incl. wildcards), adv-filter, dedup (+visible),
      lookup, add_col, compare, regex, remove_one (fixed intent), copy, sort, merge-ZMDETPC, pivot long-form.
      Spec: 4.1-4.14, 7.0 items 9 and 11. Done when: unit tests per op, incl. the `remove_one` 1218-row case shape.
- [ ] 2.3 *(partial: week/range/date parsing done in `src/abc/run.rs`)* **Run config, dates, naming** (`src/abc/run.rs`): `get_json_date` (single / week range / start-end / diff),
      per-day loop, folder layout, `rname_overrides`, file naming `<date>_<name>_<table>.tsv`, collision suffix.
      Spec: 1.1, 1.4, 7.0 items 2 and 11. Done when: week 31/2026 -> 2026-07-26..08-01 and week 45 -> 11-01..11-07 tested.
- [x] 2.4 *(mustgo: `tests/common`, `tests/mustgo_*`)* **Fixture harness**: convert the xlsx outputs/raw files in the zips to golden TSV under `tests/fixtures/`
      (one-off script in the scratchpad, not the repo), plus a compare helper (row-exact diff with a readable report).
      Spec: section 9. Needs 0.4.

## Phase 3: offline report logic (pure functions: raw TSV tables in, output tables out)

Each chunk is independent, depends on 2.1-2.4, and is verified against the section 9 golden data. Order by value:

- [x] 3.1 *(done, matches 9.3 on xlsx-derived and real data)* `mustgo` post-process + `snr_copy` + `week` + pivot (5.1, 9.3).
- [x] 3.2 *(xdock, toyota and misc done. xdock: matches 9.6 on the real exports, see "Design notes: `xdock`"; stages for toyota trailers and misc regex
      exist in `abc/inv.rs` but have no runner)* `xdock` / `toyota` / `misc` (5.2-5.4, 9.6): VL06O dedup, ZMDETPC merge rules,
      pivot, toyota trailers table.
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
- [x] 4.1 *(code done, used by the mustgo adapter via `closed_range`; awaiting live smoke; `shipment_status`, `by_created_date`, `shipment_numbers` not wired to config/CLI)* **VT11 parity** (3.1, P6): `K_STTRG`/`K_ERDAT`, optional `K_TKNUM` paste, `monday_include_sunday`,
      `ignore_display_error`, no-layout export, single-date rule (`date1 == date2` clears `-HIGH`). Smoke run:
      variant `XD_ALL_EPDC_`, check outcome and raw header against the `mustgo` sample (91 rows, 18 default columns).
- [x] 4.2 *(code done 2026-11-10: headless `export_target`; awaiting live smoke)* **VL06O parity** (3.3): confirm item view, layout menu path `menu[3]/menu[2]/menu[1]`, no-layout export,
      distinct delivery/shipment paste, outcome/path return. Smoke run with shipments from 4.1.
- [x] 4.3 *(by deliveries / deliveries + TOs done in code, awaiting live smoke; serial/history tab 1 not done)* **ZMDESNR parity, v1 modes** (3.4, P2, P3): by deliveries with variant (no date field), by deliveries + TOs
      (`S_TANUM`, clear `S_PALLTD`/`S_VBELN` range, answer `wnd[2]` warnings), serial/history tab 1. Layout by name
      (`mg_view`) via `btn[33]`; keep `pre_export_back`. Smoke run both mustgo calls (5796 / 476 row shapes).
- [x] 4.4 *(code done: `lt23.rs`, no `lt23_module.rs` and no menu entry; all ids unverified)* **LT23, new** (3.5): `lt23.rs` + `lt23_module.rs` following the pair pattern, `run_lt23_export_outcome`.
      Warehouse `E01`, TO multi paste, `radT1_QUITA`, layout via `btn[33]` + grid `cntlGRID` (else `btn[32]` +
      `setup_layout_li`), export `menu[0]/menu[1]/menu[1]`. Register in `lib.rs` and `main.rs`. No menu option in v1.
      Smoke run with TOs from 4.3 (expect the 87-TO shape).
- [x] 4.5 *(code done 2026-11-10 for the summarised flow, all ids unverified, see the xdock checklist)* **ZMDETPC, new, summarised only** (3.6): `zmdetpc.rs` + module; variant (P2), `S_DATE-LOW/HIGH`,
      `S_VBELN` paste with the non-numeric first-row delete (`btn[14]`) and `wnd[2]` answers, `radR_SUMM`/`radR_COMPL`
      refresh sequence, execute, `MESSTXT1` popup, export `menu[0]/menu[3]/menu[1]`. Output columns `delivery, pallet,
      child`. Detail flow and `pallet_pick_date` are v2.
- [ ] 4.6 *(v2)* ZVT11 Closed (3.2), ZMDESNR user-activity / by-date / exclude extras, VT22 export (3.7), VT03N read
      (3.8), ZMDETPC detail + `pallet_pick_date`.

Per chunk: code, pure-function unit tests, `cargo fmt/clippy/test`, a live smoke run with the exported raw file kept as
a fixture, then tick the box with the control IDs that needed fixing (the spec IDs come from the VBA, not from a
live SAP, so some may differ).

## Phase 5: orchestration

- [x] 5.1 *(mustgo: `abc/runner.rs`; per-report runners for the others pending)* Report runners composing SAP step + table logic per report, tables persisting across blocks in one run
      (decision 15), error semantics (skip rest of block, non-zero exit), export cleanup, logging.
- [x] 5.2 *(done for one run per call: `--rname`/`--run-date`, `[abc.mustgo]`, TUI entry; per-day loop not done)* CLI/config: subcommand and flags for report list, date/range, per-day loop; TUI entry optional.
- [ ] 5.3 First end-to-end live run: `mustgo` (needs 3.1, 4.1, 4.3, 4.4, 5.1, 5.2). Then widen report by report (`xdock` next:
      needs 3.2, 4.1, 4.2, 4.5; its checklist is below).

## Phase 6: close-out

- [ ] 6.1 `inb_rs.exe` TSV reader (separate repo; per 0.5) and hand-off note.
- [ ] 6.2 *(CLAUDE.md, README, CONFIG.md, COMMAND_LINE_USAGE.md updated 2026-11-10 for mustgo)* Update `CLAUDE.md` and README with the new module layout and commands; resolve section 7 leftovers in the spec.

---

## Dependency summary

```
0.x --> 2.x --> 3.x --\
1.1 --> 4.x ----------+--> 5.x --> 6.x
```

Fastest vertical slice: 1.1 -> 2.1-2.4 -> 3.1 -> 4.1/4.3/4.4 -> 5.x for `mustgo`.
