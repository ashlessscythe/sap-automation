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
| `--rname=<name>` | Report to run. `mustgo`, `xdock`, `toyota`, `misc`, `k2xx` and `relabel` have a runner today (case-insensitive; `k2xx` and `relabel` have not run against live SAP yet). Unknown names fail with the list of known reports. |
| `--run-date=<DATE\|A..B\|WKnn>` | `YYYY-MM-DD`, `YYYY-MM-DD..YYYY-MM-DD`, or a week `WK45` / `WK45/2026` (Sunday..Saturday, Excel WEEKNUM; the year defaults to the current year). A range or week is **one run**: VT11 gets the whole range and `export_date` becomes `WK_<n>` (a single date gives `yyyy-mm-dd`). |

| `--date-range-buffer=<DAYS>` | Days added before the start and after the end of the run date range of every tcode that sets one (VT11, ZMDETPC). **Omitted**: ZMDETPC gets **3** days each side when the run is a week (`WKnn`, or the exact Sunday..Saturday range), everything else gets **0**, so a single date or other range is not widened. An explicit value (also `0`) applies to every such tcode and to single dates too. Overrides `[abc.<report>] date_range_buffer`. |

`--rname` and `--run-date` require each other. Allowed with them: `--skip-sap-check`, `--keep-awake`, `--reports-dir`,
`--date-format`, `--timezone`, `--ignore-display-error`, `--date-range-buffer`. Defaults for variants, layouts, warehouse and clean-up come from the
optional `[abc.mustgo]` / `[abc.xdock]` / `[abc.toyota]` / `[abc.misc]` / `[abc.k2xx]` / `[abc.relabel]` section (see [CONFIG.md](CONFIG.md)); there are no per-tcode flags for a report.

```bash
./sap_automation.exe --rname=mustgo --run-date=2026-11-05
./sap_automation.exe --rname=mustgo --run-date=WK45 --keep-awake
./sap_automation.exe --rname=mustgo --run-date=2026-11-01..2026-11-07 --reports-dir=D:\reports
./sap_automation.exe --rname=xdock --run-date=2026-11-05
./sap_automation.exe --rname=toyota --run-date=2026-11-05
./sap_automation.exe --rname=misc --run-date=2026-11-05
./sap_automation.exe --rname=k2xx --run-date=2026-11-05
./sap_automation.exe --rname=relabel --run-date=2026-11-05                     # after k2xx for the same date, same day
./sap_automation.exe --rname=toyota --run-date=WK45                          # ZMDETPC 2026-10-29..2026-11-10
./sap_automation.exe --rname=toyota --run-date=WK45 --date-range-buffer=0    # ZMDETPC 2026-11-01..2026-11-07
```

The run needs a logged-in SAP GUI session (checked unless `--skip-sap-check`), asks nothing, and prints the step row counts
and the output paths. The flow of `mustgo` is VT11 -> VL06O -> ZMDESNR (by deliveries) -> LT23 -> ZMDESNR (by deliveries +
TOs); the flow of `xdock` is VT11 (variant `XD_ALL_EPDC_`, layout `scorecard`) -> VL06O for all shipments (layout `xdock`, one
row per delivery) -> ZMDETPC summarised for the deliveries -> merge (`total` = pallet + child, or the number of packages for a
delivery without a ZMDETPC row) -> pivot of `total` by plant. `toyota` is the same flow with VT11 variant `FG_TOYO_XD`
plus a trailers table (`ROUND(total / 14.5)` per pivot row). `misc` is VT11 (variant `MISC_EPDC_`) -> VL06O (layout `inv`,
one row per delivery) -> a shipping point `EPDC` is replaced by the first parenthesised part of the ship-to name -> pivot of
`number_of_packages` by shipping point; it has no ZMDETPC step. `k2xx` is VT11 (variant `TIER_VT11_K2XX`, layout `scorecard`;
a Monday run date starts on the Sunday before) -> VL06O for all shipments (layout `pallets`, one row per delivery) -> ZVT11
Closed for the deliveries (variant `k2xx_`, layout `add_del`, run date -/+ 100 days) -> ZMDETPC summarised for the deliveries
-> merge (`total` = pallet + child; **blank** for a delivery without a ZMDETPC row, unlike xdock/toyota) -> pivot of `total`
by plant (the macro has no pivot for k2xx; it was added on the owner's request). `k2xx` has
not run against live SAP yet and is tested on synthetic stand-in data only (checklist in `docs/abc_headless_plan.md`). The
first empty result or error stops the run and **nothing is written**.

`relabel` takes the delivery list from the `VL06O` table that `k2xx` wrote for the **same run date on the same calendar day**
(`<reports_dir>\K2XX\<today>\k2xx\<date>_K2XX_VL06O.tsv`); without that file (or with an empty one) it stops with "Run k2xx first" and
SAP is not touched. It needs a **single date** (`--run-date=YYYY-MM-DD`; a range or `WKnn` is rejected). Flow: ZMDESNR by deliveries (variant
`relabel_k2xx`, layout `epdc_relabel`) -> rows of plant `FV55` for the customers `LSI*`, `GM*`, `GENERAL*` count as plant `GM55` while
the blocks are built -> blocks of rows (non-FV55 rows with 2 scan copies or a blank verification dated the run date or the next day,
the first pallet row per parent serial, the FV55 pallet rows, FV55 rows without pallet and parent, rows with a kanban and a parent) ->
ZMDESNR by serial / History for the FV55 pallet-1 serials (rows at storage location 11 with status `PRI` are flagged `yellow`) ->
`_of_labels` (4 for FV55 pallet rows 1/2 without kanban, not container `KC*`, not yellow, when at least two exist; 2 otherwise) ->
`export_date` -> pivot of `_of_labels` by customer and plant. `relabel` has not run against live SAP yet and is tested on
synthetic stand-in data only (checklist in `docs/abc_headless_plan.md`).

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
  no temporary `GM55` marks), `..._PIVOT.tsv` (`customer`, `plant`, sum of `_of_labels`; `(blank)` last, no grand-total row). The `full_label`
  sheet of the macro is not written.
* Temporary SAP exports (text with tabs, forced): `<reports_dir>\<TCODE>\<yyyymmddhhmmss>_<TCODE>.txt`; deleted after a
  successful load unless `[abc.<report>] delete_exports = false`. A file that failed to load is kept.

**Rejected before SAP connect**

| Condition | Error |
| --- | --- |
| `--rname` without `--run-date` (or the reverse) | clap: required arguments not provided |
| `--rname` unknown or without a runner | `Invalid --rname value: unknown report '...'. Known reports: mustgo, xdock, toyota, misc, k2xx, relabel` |
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
./sap_automation.exe --tcode=vt11 --layout=ob_6 --variant=ob_win \
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
./sap_automation.exe --tcode=zexsnr --variant=INV_VIEW_EPDC --layout=mg_view \
  --by-delivery=true --delivery-file=zvt11 \
  --pre-export-back=false --tab-number=2
```

### ZMDESNR bin filter (tab-delimited export)

Sets Storage Bin (`LGPLA-LOW`) and skips delivery paste. `--export-type=1` is text with tabs; omit `--export-type` (and no config default) for Excel `.xlsx`. Quote `*` in PowerShell.

```bash
./sap_automation.exe --tcode=zmdesnr --variant=INV_VIEW_EPDC --layout=audit25 \
  --bin-filter="64EP*" --export-type=1 --reports-dir=C:\reports
```

### LX03 variant + layout (tab-delimited export)

```bash
./sap_automation.exe --tcode=lx03 --variant=FG_BIN_AUD --layout=audit25 \
  --export-type=1 --reports-dir=C:\reports
```

### Sequence (config-driven) with a faster cadence

```bash
./sap_automation.exe --run-sequence --iterations=5 --interval-seconds=10
```

### Regular 149 plant loop (omit run-type or use `none`)

Config plants under `[tcode.y_xx_99999999]`:

```toml
[tcode.y_xx_99999999]
variant = "mg_int_var"
layout = "mg_view"
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
