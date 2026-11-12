# SAP Automation - Command Line Usage

## Overview

The SAP Automation tool supports both interactive and command-line modes:

- Run with **no arguments** → interactive TUI menu.
- Run with **subcommand or flags** → unattended mode (no prompts).

CLI flags always win over `config.toml`, and they work whether or not `config.toml` exists. When any override is in play, the runner prints a one-line summary right under the banner so you can see exactly what's applied before the SAP work starts:

```
CLI overrides applied (these win over config.toml): --tcode=VT11 --iterations=3
```

## Three Execution Modes

| Mode | Trigger | Notes |
| --- | --- | --- |
| **Loop** | `run-loop` subcommand or `--run-loop` flag | Repeats one TCode on a timer. Uses `[loop]` from `config.toml`; CLI flags override. |
| **Sequence** | `run-sequence` subcommand or `--run-sequence` flag | Runs a list of menu options in order. Always config-driven; per-tcode flags are rejected. |
| **Single-shot** | `--tcode=<X>` (no `--run-loop`/`--run-sequence`) | Runs the auto-flow for that TCode once and exits. |
| **Report** | `--rname=<name> --run-date=<date>` | Runs a headless report (all SAP steps, post-processing, TSV outputs) once and exits. See [Headless report](#headless-report---rname---run-date). |

```bash
./sap_automation.exe --help                   # full flag list
./sap_automation.exe run-loop                 # subcommand form
./sap_automation.exe --run-loop               # equivalent flag form
./sap_automation.exe --tcode=vt11             # single-shot
```

## Flag Reference

### Mode + connection

| Flag | Purpose |
| --- | --- |
| `--run-loop` | Equivalent to the `run-loop` subcommand. |
| `--run-sequence` | Equivalent to the `run-sequence` subcommand. |
| `--skip-sap-check` | Skip the "logged into SAP" check (useful for unit-style testing). |
| `--keep-awake` | Prevent the system from sleeping while the run is in progress. |

`--run-loop` and `--run-sequence` are mutually exclusive.

### Identity (which TCode runs)

| Flag | Required? | Notes |
| --- | --- | --- |
| `--tcode=<NAME>` | Required for loop / single-shot if `[loop].tcode` is unset | Case-insensitive (`vt11` → `VT11`). Supported in single-shot: `VT11`, `ZEXVT11`, `VL06O`, `ZEXSNR`, `Y_XX_99999999`. |
| `--tcode-run-type=<rcv\|mat\|tsp\|none>` | Optional for 149 | Selects the 149 sub-flow. Omit or `none` = regular plant-loop 149. |

If `--tcode` (and `[loop].tcode`) are both missing for a loop run:

```
Missing flag for tcode, enter with --tcode (or set [loop].tcode in config.toml)
```

### Per-TCode overrides

| Flag | Targets | Notes |
| --- | --- | --- |
| `--layout=<NAME>` | The TCode under `--tcode` | Overrides `[tcode.X].layout`. |
| `--variant=<NAME>` | The TCode under `--tcode` | Overrides `[tcode.X].variant`. |
| `--export-type=<0..4>` | The TCode under `--tcode` | 0=unconverted, 1=text-tabs, 2=rich-text, 3=HTML, 4=clipboard. For ZMDESNR/LX03, omit (and no config/global default) → Excel `.xlsx` menu export. |
| `--plants=<LIST>` | Regular 149 only | Comma-separated plants (e.g. `PLT1,PLT2`). Overrides `[tcode.y_xx_99999999].plants`. Empty list / omitted flag falls back to config; empty after both → one `ALL` export. |

### Loop / sequence timing

| Flag | Applies to | Notes |
| --- | --- | --- |
| `--iterations=<N>` | Loop, sequence | `0` = infinite (Ctrl+C to stop). |
| `--delay-seconds=<N>` | Loop, sequence | Seconds between iterations. |
| `--interval-seconds=<N>` | Sequence only | Seconds between steps within a single iteration. |

### Filter toggles (per-tcode)

All booleans are explicit — pass `=true` or `=false`. CLI value wins, so you can disable a `config.toml` filter from the command line.

| Flag | TCode scope | Notes |
| --- | --- | --- |
| `--by-date=<bool>` | VT11, ZEXVT11, VL06O | Filter the report by date range. |
| `--vt11-closed-date=<SPEC>` | VT11 (`K_DATEN`, actual shipment end) | `YYYY-MM-DD`, `YYYY-MM-DD..YYYY-MM-DD`, or a week `WK45` / `WK45/2026` (Sunday..Saturday, Excel WEEKNUM; `WK45` in 2026 = 2026-11-01..2026-11-07). Independent of `--by-date`. |
| `--vt11-created-date=<SPEC>` | VT11 (`K_ERDAT`, creation date) | Same formats as `--vt11-closed-date`. Both may be given together. |
| `--ignore-display-error[=<bool>]` | VT11 **only** | Treat a "no display authorization" status-bar message as harmless. Overrides `[tcode.VT11].ignore_display_error`. Rejected with `--tcode!=VT11`. |
| `--rname-dir=<dir>` | all exports | Export subfolder. Relative (`./mustgo`, `mustgo`) goes under the reports dir: `{reports_dir}\mustgo\{tcode}`. Absolute paths are used as is: `<dir>\{tcode}`. Only changes where files are written. |
| `--by-delivery=<bool>` | VT11, ZEXVT11, VL06O | Filter the report by delivery numbers. |
| `--by-shipment=<bool>` | VL06O **only** | Filter the report by shipment numbers. |
| `--limiter=<TYPE>` | VT11, ZEXVT11 | Currently only `date_range` is wired in code; other values are accepted but no-op. |

Two hard rules enforced before any SAP connection is attempted:

- `--by-delivery=true` and `--by-shipment=true` cannot both be set.
- `--by-shipment=true` requires `--tcode=VL06O`.

`--by-date` stacks with `--by-delivery` (or `--by-shipment` for VL06O); the SAP flow applies both filters.

### TCode-specific behavior toggles

| Flag | TCode scope | Notes |
| --- | --- | --- |
| `--pre-export-back=<bool>` | ZEXSNR **only** | Send vkey 3 (back) after export but before layout selection. Overrides `[tcode.ZEXSNR].pre_export_back`. Pass `=false` to disable a config-on value from the CLI. Rejected with `--tcode!=ZEXSNR`. |
| `--use-clipboard[=<bool>]` | all tcodes | Fill multi-selection popups (deliveries, shipments, TOs, materials, ...) from the Windows clipboard (Shift+F12, "upload from clipboard") instead of entering rows with Page Down. Bare flag, `true`, `1`, `yes` or `on` enables; default off. The paste is verified (first and last entry, row count) and falls back to row-by-row entry if it did not take. TO numbers are reduced to digits. |
| `--tab-number=<N>` | ZEXSNR **only** | Which results tab to select. Resolution order: **CLI override → `[tcode.ZEXSNR].tab_number` → in-code default `2`** (so a brand-new install with no config still works). Rejected with `--tcode!=ZEXSNR`. **Note:** SAP GUI requires plant (`WERKS`) to be valid before tab switching, so the report flow always selects the variant first (which fills `WERKS`) and only then switches to this tab. |
| `--bin-filter=<BIN>` | ZMDESNR **only** | Storage bin filter (e.g. `64EP*`). Sets `LGPLA-LOW` and **skips delivery paste**. Overrides `[tcode.ZMDESNR].bin_filter`. Rejected with `--tcode!=ZMDESNR`. In PowerShell, quote wildcards: `--bin-filter="64EP*"`. |

### Date range

| Flag | Format | Notes |
| --- | --- | --- |
| `--date-start=<YYYY-MM-DD>` | ISO date | Overrides `[tcode.X].date_range_start`. Validated up-front; bad input fails fast. |
| `--date-end=<YYYY-MM-DD>` | ISO date | Overrides `[tcode.X].date_range_end`. |

### Delivery / shipment source overrides

By default `--by-delivery=true` reads numbers from a hardcoded merge of:

- the newest ZEXSNR export under `<reports_dir>\zexsnr\`, plus
- the newest unused VT11 ListCheck CSV under `<reports_dir>\vt11_listcheck\` (skipping files marked with the `_.csv` "consumed" suffix).

The CLI lets you override that source per run:

| Flag | Default | Resolution |
| --- | --- | --- |
| `--delivery-file=<value>` | unset → legacy merge | See "Source resolution rules" below. |
| `--delivery-col=<HEADER>` | `Delivery` | Header column to read; only used when the source has headers. |
| `--shipment-file=<value>` | unset → `<reports_dir>\vt11\` newest `.xlsx` | VL06O only. |
| `--shipment-col=<HEADER>` | `Shipment Number` | VL06O only. |

When `--delivery-file` is set, the legacy ZEXSNR + ListCheck merge is **replaced** (and the `_.csv` rename of consumed ListCheck files is skipped — we didn't consume them). When unset, behavior is unchanged.

### Source resolution rules

`--delivery-file` and `--shipment-file` accept either:

- A **slug** (no `\` `/` `.`) → resolved to `<reports_dir>\<slug-lowercased>\`, picks the newest file in that directory. Files ending in `_.csv` (the "consumed" sentinel) are skipped.
- A **literal path** (contains `\` `/` `.`) → used as-is; checked for existence.

The file extension drives the parser:

| Ext | Parser |
| --- | --- |
| `.csv` | Comma-CSV; header on row 1. |
| `.tsv` `.txt` `.rtf` `.html` | Existing tab-delimited reader (lossy UTF-8 tolerant). |
| `.xlsx` `.xls` | Existing Excel reader (`Sheet1`). |

### Global overrides

| Flag | Overrides | Notes |
| --- | --- | --- |
| `--reports-dir=<PATH>` | `[global].reports_dir` | |
| `--date-format=<FMT>` | `[global].date_format` | e.g. `yyyy-mm-dd`, `mm/dd/yyyy`, `dd-mm-yy`. |
| `--timezone=<TZ>` | `[global].timezone` | e.g. `UTC`, `MDT`, `America/Denver`. |

### Headless report (`--rname`, `--run-date`)

| Flag | Meaning |
| --- | --- |
| `--rname=<name>` | Report to run. `mustgo`, `xdock`, `toyota`, `misc`, `k2xx`, `relabel` and `repack` have a runner today (case-insensitive). Unknown names fail with the list of known reports. |
| `--run-date=<DATE\|A..B\|WKnn>` | `YYYY-MM-DD`, `YYYY-MM-DD..YYYY-MM-DD`, or a week `WK45` / `WK45/2026` (Sunday..Saturday, Excel WEEKNUM; the year defaults to the current year). A range or week is **one run**: VT11 gets the whole range and `export_date` becomes `WK_<n>` (a single date gives `yyyy-mm-dd`). |

| `--date-range-buffer=<DAYS>` | Days added before the start and after the end of the run date range of every tcode that sets one (VT11, ZMDETPC). **Omitted**: ZMDETPC gets **3** days each side when the run is a week (`WKnn`, or the exact Sunday..Saturday range), everything else gets **0**, so a single date or other range is not widened. An explicit value (also `0`) applies to every such tcode and to single dates too. Overrides `[abc.<report>] date_range_buffer`. |

| `--loop` | With `--rname`: one run **per day** of the `--run-date` range or week (7 days for `WKnn`) instead of one run over the range. Per day the prerequisite reports run first (`k2xx` -> `relabel` -> `repack`). A day with no data is skipped; any other error stops the iteration with no merge. Needs a range or week. Not `--run-loop`. |
| `--merge` | With `--loop` (required): after the per-day runs write one `<DATE>_<RNAME>_MERGED.xlsx` per report of the chain, with every output table as a sheet and a leading `run_date` column (ISO `yyyy-mm-dd`). Not `--iterations`. |
| `--no-sap` | With `--rname`: strict offline, SAP is never touched (no session; SAP GUI may be closed). Every day and report of the chain (prerequisites too) must have ALL its output TSVs, else an error lists the missing day/report/tables and nothing is merged. With `--loop --merge` re-merges an exported week; with `--loop` alone or a single date only verifies. Conflicts with `--skip-existing`; an error with `--output-format=xlsx`. |
| `--skip-existing` | With `--rname`: resume. A day+report whose output tables are all present is logged "exists, skipped" and not run; the rest runs in SAP. Skipped days count as done and are merged. Works with `--loop` and for a single run. Conflicts with `--no-sap`. |
| `--output-date=<YYYY-MM-DD>` | With `--rname`: the output date folder to read/write instead of today (runs, `--loop`, `--merge`, `--skip-existing`, `--no-sap`; relabel/repack read the k2xx/relabel TSVs of that folder). |
| `--output-format=<tsv\|xlsx>` | Output of a report run. `tsv` (default); `xlsx` additionally writes `<DATE>_<RNAME>.xlsx` (every output table a sheet) next to the TSV files. |

`--loop` / `--merge` are unrelated to `--run-loop`, `--run-sequence` and `--iterations` (the config-driven TCode loop/sequence) and conflict with
`--run-loop` / `--run-sequence`. `--no-sap` / `--skip-existing` / `--output-date` only reuse or redirect the `--rname` TSV outputs; they are
likewise unrelated to those (config-driven) loops and are not in the TUI.

`--rname` and `--run-date` require each other. Allowed with them: `--skip-sap-check`, `--keep-awake`, `--reports-dir`,
`--date-format`, `--timezone`, `--ignore-display-error`, `--date-range-buffer`, `--use-clipboard`, `--output-format`, `--loop`, `--merge`, `--no-sap`, `--skip-existing`, `--output-date`. Defaults for variants, layouts, warehouse and clean-up come from the
optional `[abc.mustgo]` / `[abc.xdock]` / `[abc.toyota]` / `[abc.misc]` / `[abc.k2xx]` / `[abc.relabel]` / `[abc.repack]` section (see [CONFIG.md](CONFIG.md)); there are no per-tcode flags for a report.

```bash
./sap_automation.exe --rname=mustgo --run-date=2026-11-05
./sap_automation.exe --rname=mustgo --run-date=WK45 --keep-awake
./sap_automation.exe --rname=mustgo --run-date=2026-11-01..2026-11-07 --reports-dir=D:\reports
./sap_automation.exe --rname=xdock --run-date=2026-11-05
./sap_automation.exe --rname=toyota --run-date=2026-11-05
./sap_automation.exe --rname=misc --run-date=2026-11-05
./sap_automation.exe --rname=k2xx --run-date=2026-11-05
./sap_automation.exe --rname=relabel --run-date=2026-11-05                     # after k2xx for the same date, same day
./sap_automation.exe --rname=repack --run-date=2026-11-05                      # after k2xx and relabel for the same date, same day
./sap_automation.exe --rname=toyota --run-date=WK45                          # ZMDETPC 2026-10-29..2026-11-10
./sap_automation.exe --rname=toyota --run-date=WK45 --date-range-buffer=0    # ZMDETPC 2026-11-01..2026-11-07
./sap_automation.exe --rname=xdock --run-date=2026-11-05 --output-format=xlsx   # TSV files + one workbook
./sap_automation.exe --rname=xdock --run-date=wk40 --loop                        # 7 separate per-day runs
./sap_automation.exe --rname=xdock --run-date=wk40 --loop --merge                # ... plus WK_40_XDOCK_MERGED.xlsx
./sap_automation.exe --rname=repack --run-date=wk40 --loop --merge               # per day k2xx, relabel, repack; 3 merged files
./sap_automation.exe --rname=xdock --run-date=wk46 --loop --merge --no-sap --output-date=2026-11-12        # re-merge, SAP closed
./sap_automation.exe --rname=xdock --run-date=wk46 --loop --merge --skip-existing --output-date=2026-11-12 # resume after a stop
```

**Per-day iteration (`--loop`, `--merge`).** Each day is a single-date run of the whole chain (`k2xx` -> `relabel` -> `repack` for repack; a
report without prerequisites runs alone), so `relabel`/`repack` find the k2xx output of the same day. A `NO DATA` day is logged and skipped,
every other error stops the iteration (no merge; per-day files stay). The merge covers the days that ran or were skipped (a skipped day
may still have output); if no day finished there is no merge and no error. Merged workbook: `<reports_dir>\<folder>\<today>\<rName>\<DATE>_<RNAME>_MERGED.xlsx`
(`WK_<n>` for a week), sheets = all output tables, columns matched by header name, a `run_date` column first (ISO `yyyy-mm-dd`). Per-day
file names keep `MM-DD-YYYY`. The summary lists days done / skipped / failed.

The run needs a logged-in SAP GUI session (checked unless `--skip-sap-check`), asks nothing, and prints the step row counts
and the output paths. The flow of `mustgo` is VT11 -> VL06O -> ZMDESNR (by deliveries) -> LT23 -> ZMDESNR (by deliveries +
TOs); the flow of `xdock` is VT11 (variant `VT11_XDOCK_VARIANT`, layout `VT11_LAYOUT`) -> VL06O for all shipments (layout `VL06O_XDOCK_LAYOUT`, one
row per delivery) -> ZMDETPC summarised for the deliveries -> merge (`total` = pallet + child, or the number of packages for a
delivery without a ZMDETPC row) -> pivot of `total` by plant. `toyota` is the same flow with VT11 variant `VT11_TOYOTA_VARIANT`
plus a trailers table (`ROUND(total / 14.5)` per pivot row). `misc` is VT11 (variant `VT11_MISC_VARIANT`) -> VL06O (layout `VL06O_INV_LAYOUT`,
one row per delivery) -> a shipping point containing the configured `ship_point_marker` is replaced by the first parenthesised part of the ship-to name -> pivot of
`number_of_packages` by shipping point; it has no ZMDETPC step. `k2xx` is VT11 (variant `VT11_K2XX_VARIANT`, layout `VT11_LAYOUT`;
a Monday run date starts on the Sunday before) -> VL06O for all shipments (layout `VL06O_K2XX_LAYOUT`, one row per delivery) -> ZVT11
Closed for the deliveries (variant `ZVT11_K2XX_VARIANT`, layout `ZVT11_K2XX_LAYOUT`, run date -/+ 100 days) -> ZMDETPC summarised for the deliveries
-> merge (`total` = pallet + child; **blank** for a delivery without a ZMDETPC row, unlike xdock/toyota) -> pivot of `total`
by plant (the macro has no pivot for k2xx; it was added on the owner's request). The
first empty result or error stops the run and **nothing is written**.

`relabel` takes the delivery list from the `VL06O` table that `k2xx` wrote for the **same run date on the same calendar day**
(`<reports_dir>\K2XX\<today>\k2xx\<date>_K2XX_VL06O.tsv`); without that file (or with an empty one) it stops with "Run k2xx first" and
SAP is not touched. It needs a **single date** (`--run-date=YYYY-MM-DD`; a range or `WKnn` is rejected). Flow: ZMDESNR by deliveries (variant
`ZMDESNR_RELABEL_VARIANT`, layout `ZMDESNR_RELABEL_LAYOUT`) -> rows of plant `PL55` for the customers the configured alias customer patterns count as plant `AL55` while
the blocks are built -> blocks of rows (non-PL55 rows with 2 scan copies or a blank verification dated the run date or the next day,
the first pallet row per parent serial, the PL55 pallet rows, PL55 rows without pallet and parent, rows with a kanban and a parent) ->
ZMDESNR by serial / History for the PL55 pallet-1 serials (rows at storage location 11 with status `PRI` are flagged `yellow`) ->
`_of_labels` (4 for PL55 pallet rows 1/2 without kanban, not container `KC*`, not yellow, when at least two exist; 2 otherwise) ->
`export_date` -> pivot of `_of_labels` by customer and plant. Both `k2xx` and `relabel` have run against live SAP and are golden-tested on the owner's real exports.

`repack` takes the delivery list from the `VL06O` table of `k2xx` **and** the exclusion list from the `yellow` rows of the `combined` table of
`relabel`, both of the **same run date on the same calendar day** (`...\K2XX\<today>\k2xx\<date>_K2XX_VL06O.tsv`,
`...\relabel\<date>_RELABEL_COMBINED.tsv`); without one of them (or with an empty one) it stops with "Run k2xx first" / "Run relabel first" and SAP
is not touched. It needs a **single date**. Flow: ZMDESNR by deliveries excluding the yellow serials as parents (variant `ZMDESNR_REPACK_VARIANT`,
layout `ZMDESNR_REPACK_LAYOUT`; with fewer than two yellow rows the export runs without exclusion) -> rows of plant `PL55` for the customers the
configured alias customer patterns count as plant `AL55` while the blocks are built -> blocks of rows in this order: non-PL55 rows without parent and
under 50 (R1), the History rows (ZMDESNR by serial / History for the non-PL55 rows without parent and over 50; only warehouse `WH01` and status `UNP` are kept),
non-PL55 pallet rows with status `PAL` (R3), with every row of container `KC*` or `SC*` deleted from those three, then PL55 rows without pallet and
not container `KC*` (R4) -> `export_date` -> pivot (count of `lm_serial_number`) by customer and plant. Nothing is sorted (the macro does not either).
It has been verified offline against the owner's real exports and the macro output (280 rows); it has not run against live SAP yet.

**Exit codes:** `0` success; non-zero for every failure. The message tells them apart:
`NO DATA at step '<step>': <reason>` (SAP returned nothing, or a stage had nothing left, e.g. no TPDC shipments or no delivery
with two or more TOs) or `ERROR: <details>` (SAP step failed, missing column, unreadable export, ...). Invalid flags exit
non-zero before SAP is touched (clap errors exit with 2).

**Where files go**
* Outputs `mustgo`: `<reports_dir>\K2XX\<today yyyy-mm-dd>\mustgo\<date>_MUSTGO_LT23.tsv`, `..._SNR_COPY.tsv`, `..._PIVOT.tsv`
  (`<date>` is `MM-DD-YYYY`, or `WK_<n>` for a range).
* Outputs `xdock`: `<reports_dir>\INV\<today yyyy-mm-dd>\xdock\<date>_XDOCK_VT11.tsv`, `..._VL06O.tsv` (merged table, last
  columns `delivery_`, `pallet`, `child`, `total`), `..._PIVOT.tsv` (`plant`, `total`, a blank plant is `(blank)` and last, no
  grand-total row).
* Outputs `toyota`: `<reports_dir>\INV\<today yyyy-mm-dd>\toyota\<date>_TOYOTA_VT11.tsv`, `..._VL06O.tsv`, `..._PIVOT.tsv`
  and `..._TRAILERS.tsv` (`plant`, `trailers`; one row per pivot row, so a `(blank)` plant has one too).
* Outputs `misc`: `<reports_dir>\INV\<today yyyy-mm-dd>\misc\<date>_MISC_VT11.tsv`, `..._VL06O.tsv`, `..._PIVOT.tsv`
  (`shipping_pointreceiving_pt`, `number_of_packages`; same shape rules as the xdock pivot).
* Outputs `k2xx`: `<reports_dir>\K2XX\<today yyyy-mm-dd>\k2xx\<date>_K2XX_ZVT11.tsv` (as exported), `..._VT11.tsv`,
  `..._VL06O.tsv` (merged table, sorted by delivery, last columns `date`, `delivery_`, `pallet`, `child`, `total`),
  `..._PIVOT.tsv` (`plant`, `total` = sum of `total` of the `VL06O` table; same shape rules as the xdock pivot; a delivery
  with a blank `total` adds nothing).
* Outputs `relabel`: `<reports_dir>\K2XX\<today yyyy-mm-dd>\relabel\<date>_RELABEL_COMBINED.tsv` (the columns of the ZMDESNR export, then
  `_of_labels`, `export_date`, `yellow` (`1` = history hit)), `..._SN_DEL.tsv` and `..._SNR_ORIG.tsv` (the delivery export as loaded, identical;
  no temporary `AL55` marks), `..._PIVOT.tsv` (`customer`, `plant`, sum of `_of_labels`; `(blank)` last, no grand-total row). The `full_label`
  sheet of the macro is not written.
* Outputs `repack`: `<reports_dir>\K2XX\<today yyyy-mm-dd>\repack\<date>_REPACK_SNR_REPACK.tsv` (the columns of the ZMDESNR export, then
  `export_date`), `..._REPACK_SN.tsv` (the by-deliveries export as loaded; no temporary `AL55` marks and no extra row), `..._PIVOT.tsv` (`customer`,
  one column per plant, `Grand Total`; count of `lm_serial_number`; `(blank)` last).
* Temporary SAP exports (text with tabs, forced): `<reports_dir>\<TCODE>\<yyyymmddhhmmss>_<TCODE>.txt`; deleted after a
  successful load unless `[abc.<report>] delete_exports = false`. A file that failed to load is kept.

**Rejected before SAP connect**

| Condition | Error |
| --- | --- |
| `--rname` without `--run-date` (or the reverse) | clap: required arguments not provided |
| `--rname` unknown or without a runner | `Invalid --rname value: unknown report '...'. Known reports: mustgo, xdock, toyota, misc, k2xx, relabel, repack` |
| `--run-date` not a date / range / week, or end before start, week outside 1-53 | `Invalid --run-date value '...': ...` |
| `--rname` with `--run-loop`, `--run-sequence` or the run-loop / run-sequence subcommands | clap conflict / `--rname can't be combined ...` |
| `--rname` with any per-tcode flag (`--tcode --layout --variant --export-type ... --bin-filter`) | `<flags> can't be used with --rname; ...` |
| `--rname` with `--rname-dir` | `--rname-dir can't be used with --rname ...` |

## Validation Rules (fail-fast, before SAP connect)

| Condition | Error |
| --- | --- |
| `--by-delivery=true` and `--by-shipment=true` together | `--by-delivery=true and --by-shipment=true cannot both be set ...` |
| `--by-shipment=true` with `--tcode!=VL06O` | `--by-shipment is only supported for VL06O ...` |
| `--pre-export-back` with `--tcode!=ZEXSNR` | `--pre-export-back is only supported for ZEXSNR ...` |
| `--tab-number` with `--tcode!=ZEXSNR` | `--tab-number is only supported for ZEXSNR ...` |
| `--date-start` / `--date-end` not ISO | `Invalid --date-start value '...': expected ISO YYYY-MM-DD ...` |
| Loop run, no `--tcode` and no `[loop].tcode` | `Missing flag for tcode, enter with --tcode ...` |
| 149 single-shot with unknown `--tcode-run-type` | `Unknown --tcode-run-type='...'. Use rcv \| mat \| tsp \| none ...` |
| `--run-sequence` with any per-tcode flag | `<flag> can't be used with --run-sequence; sequence uses config.toml ...` |

The full list of per-tcode flags rejected by `--run-sequence`:

```
--tcode --layout --variant --export-type --tcode-run-type --plants
--by-date --by-delivery --by-shipment --limiter
--date-start --date-end
--delivery-file --delivery-col --shipment-file --shipment-col
--pre-export-back --tab-number
```

## Configuration Requirements

CLI flags can fully replace `config.toml` for **loop** and **single-shot** modes. **Sequence** mode still requires a `[sequence].options` list in config.

### Loop section

```toml
[loop]
tcode = "Y_XX_99999999"
iterations = "2"
delay_seconds = "30"
param_tcode_run_type = "mat"
```

CLI equivalent (no config needed):

```bash
./sap_automation.exe --run-loop --tcode=y_xx_99999999 \
  --tcode-run-type=mat --iterations=2 --delay-seconds=30
```

### Sequence section

```toml
[sequence]
options = ["9", "7"]
iterations = "1"
delay_seconds = "60"
interval_seconds = "10"
```

CLI may override the timing only:

```bash
./sap_automation.exe --run-sequence --iterations=3 --delay-seconds=120 --interval-seconds=15
```

If `[sequence].options` is unset and you try to run sequence:

```
No sequence options configured. Sequence uses config.toml — create and set up
[sequence].options (run interactively and pick `Configure Sequence`, or hand-edit config.toml).
```

## End-to-End Examples

### VT11 single-shot with explicit deliveries

```bash
./sap_automation.exe --tcode=vt11 --layout=my_layout --variant=my_variant \
  --export-type=1 --by-delivery=true \
  --delivery-file=C:\out\dn.csv --delivery-col="Delivery Number"
```

### VL06O shipment branch using a slug subdir

`--shipment-file=vt11` resolves to `<reports_dir>\vt11\` (newest file).

```bash
./sap_automation.exe --tcode=vl06o --by-shipment=true --shipment-file=vt11
```

### VT11 loop, ISO date range, 3 iterations

```bash
./sap_automation.exe --run-loop --tcode=vt11 \
  --by-date=true --date-start=2026-02-01 --date-end=2026-02-15 \
  --iterations=3 --delay-seconds=30
```

### ZEXSNR with deliveries from another tcode's output dir

Disables the post-export back-send (which `[tcode.ZEXSNR].pre_export_back = "true"` would otherwise do) and selects the inventory-view tab (2) instead of the configured one:

```bash
./sap_automation.exe --tcode=zexsnr --variant=ZEXSNR_VARIANT --layout=ZMDESNR_MUSTGO_LAYOUT \
  --by-delivery=true --delivery-file=zvt11 \
  --pre-export-back=false --tab-number=2
```

### ZMDESNR bin filter (tab-delimited export)

Sets Storage Bin (`LGPLA-LOW`) and skips delivery paste. `--export-type=1` is text with tabs; omit `--export-type` (and no config default) for Excel `.xlsx`. Quote `*` in PowerShell.

```bash
./sap_automation.exe --tcode=zmdesnr --variant=ZEXSNR_VARIANT --layout=my_layout \
  --bin-filter="64EP*" --export-type=1 --reports-dir=C:\reports
```

### LX03 variant + layout (tab-delimited export)

```bash
./sap_automation.exe --tcode=lx03 --variant=my_lx03_variant --layout=my_layout \
  --export-type=1 --reports-dir=C:\reports
```

### MM60 from the newest LX03 export

MM60 is standalone: it pastes the `Material` column of the newest LX03 export (`.txt` or
`.xlsx`; default dir = the LX03 export dir, honours `--rname-dir`) into the material
selection, clears the plant selection, runs, and exports. `--layout`, `--variant` and
`--export-type` apply to MM60 only (omit export type for an Excel `.xlsx` export). Rows are
de-duplicated by (material, plant) only. `--lx03-file=<slug|path>` pins the source.

```bash
./sap_automation.exe --tcode=lx03 --variant=my_lx03_variant --layout=missing --export-type=1
./sap_automation.exe --tcode=mm60 --layout=value --export-type=1
./sap_automation.exe --tcode=mm60 --layout=value --lx03-file=C:\reports\LX03\x.txt
```

### Sequence (config-driven) with a faster cadence

```bash
./sap_automation.exe --run-sequence --iterations=5 --interval-seconds=10
```

### Regular 149 plant loop (omit run-type or use `none`)

Config plants under `[tcode.y_xx_99999999]`:

```toml
[tcode.y_xx_99999999]
variant = "my_variant"
layout = "ZMDESNR_MUSTGO_LAYOUT"
export_type = 1
plants = ["PLT1", "PLT2"]
```

CLI override (wins over config):

```bash
./sap_automation.exe --tcode=Y_XX_99999999 --plants=PLT1,PLT2
# equivalent:
./sap_automation.exe --tcode=Y_XX_99999999 --tcode-run-type=none --plants=PLT1,PLT2
```

Each plant exports as `{timestamp}_y_149-{plant}.txt` under `<reports_dir>\y_149\`.

### Smoke test without SAP

```bash
./sap_automation.exe --run-loop --skip-sap-check
```

## How It Works

### Interactive Mode (Default)

When you run `./sap_automation.exe` with no arguments:

- Shows the TUI menu system
- Requires user input for navigation
- Provides full access to all features

### Unattended Mode

Triggered by any subcommand (`run-loop`, `run-sequence`) or any of the top-level flags described above:

- No user interaction required
- Uses `config.toml` (when present) plus any CLI overrides
- Prints a "CLI overrides applied" summary line when overrides are in play
- Exits automatically when complete; non-zero exit code on failure

## Wrapper Script Examples

### Windows Batch File

```batch
@echo off
echo Starting SAP Automation Loop
sap_automation.exe --run-loop --tcode=vt11 --iterations=3
if %ERRORLEVEL% EQU 0 (
    echo Loop completed successfully
) else (
    echo Loop failed with error code %ERRORLEVEL%
)
pause
```

### PowerShell Script

```powershell
Write-Host "Starting SAP Automation Sequence"
& ".\sap_automation.exe" --run-sequence --iterations=5
if ($LASTEXITCODE -eq 0) {
    Write-Host "Sequence completed successfully" -ForegroundColor Green
} else {
    Write-Host "Sequence failed with error code $LASTEXITCODE" -ForegroundColor Red
}
```

### Linux/Mac Shell Script

```bash
#!/bin/bash
echo "Starting SAP Automation Loop"
if ./sap_automation.exe --run-loop --tcode=vt11; then
    echo "Loop completed successfully"
else
    echo "Loop failed with error code $?"
    exit 1
fi
```

## Notes

- `--skip-sap-check` is for testing without a live SAP session.
- Validation runs **before** any SAP connection is attempted, so bad flag combos error out instantly.
- Ctrl+C still interrupts execution at any time.
- The "consumed" `_.csv` rename of VT11 ListCheck files only happens when the legacy merge is in use; passing `--delivery-file` skips that step.
