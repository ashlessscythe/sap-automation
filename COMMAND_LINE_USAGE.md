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
| `--tcode=<NAME>` | Required for loop / single-shot if `[loop].tcode` is unset | Case-insensitive (`vt11` → `VT11`). Supported in single-shot: `VT11`, `ZVT11`, `VL06O`, `ZMDESNR`, `Y_DN3_47000149`. |
| `--tcode-run-type=<rcv\|mat\|tsp>` | Required for 149 reports | Selects the 149 sub-flow. |

If `--tcode-run-type` is missing for a 149 run you'll see:

```
Missing flag for tcode-run-type, enter with --tcode-run-type=rcv|mat|tsp ...
```

If `--tcode` (and `[loop].tcode`) are both missing for a loop run:

```
Missing flag for tcode, enter with --tcode (or set [loop].tcode in config.toml)
```

### Per-TCode overrides

| Flag | Targets | Notes |
| --- | --- | --- |
| `--layout=<NAME>` | The TCode under `--tcode` | Overrides `[tcode.X].layout`. |
| `--variant=<NAME>` | The TCode under `--tcode` | Overrides `[tcode.X].variant`. |
| `--export-type=<0..4>` | The TCode under `--tcode` | 0=unconverted, 1=text-tabs, 2=rich-text, 3=HTML, 4=clipboard. |

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
| `--by-date=<bool>` | VT11, ZVT11, VL06O | Filter the report by date range. |
| `--by-delivery=<bool>` | VT11, ZVT11, VL06O | Filter the report by delivery numbers. |
| `--by-shipment=<bool>` | VL06O **only** | Filter the report by shipment numbers. |
| `--limiter=<TYPE>` | VT11, ZVT11 | Currently only `date_range` is wired in code; other values are accepted but no-op. |

Two hard rules enforced before any SAP connection is attempted:

- `--by-delivery=true` and `--by-shipment=true` cannot both be set.
- `--by-shipment=true` requires `--tcode=VL06O`.

`--by-date` stacks with `--by-delivery` (or `--by-shipment` for VL06O); the SAP flow applies both filters.

### TCode-specific behavior toggles

| Flag | TCode scope | Notes |
| --- | --- | --- |
| `--pre-export-back=<bool>` | ZMDESNR **only** | Send vkey 3 (back) after export but before layout selection. Overrides `[tcode.ZMDESNR].pre_export_back`. Pass `=false` to disable a config-on value from the CLI. Rejected with `--tcode!=ZMDESNR`. |
| `--tab-number=<N>` | ZMDESNR **only** | Which results tab to select. Resolution order: **CLI override → `[tcode.ZMDESNR].tab_number` → in-code default `2`** (so a brand-new install with no config still works). Rejected with `--tcode!=ZMDESNR`. **Note:** SAP GUI requires plant (`WERKS`) to be valid before tab switching, so the report flow always selects the variant first (which fills `WERKS`) and only then switches to this tab. |

### Date range

| Flag | Format | Notes |
| --- | --- | --- |
| `--date-start=<YYYY-MM-DD>` | ISO date | Overrides `[tcode.X].date_range_start`. Validated up-front; bad input fails fast. |
| `--date-end=<YYYY-MM-DD>` | ISO date | Overrides `[tcode.X].date_range_end`. |

### Delivery / shipment source overrides

By default `--by-delivery=true` reads numbers from a hardcoded merge of:

- the newest ZMDESNR export under `<reports_dir>\zmdesnr\`, plus
- the newest unused VT11 ListCheck CSV under `<reports_dir>\vt11_listcheck\` (skipping files marked with the `_.csv` "consumed" suffix).

The CLI lets you override that source per run:

| Flag | Default | Resolution |
| --- | --- | --- |
| `--delivery-file=<value>` | unset → legacy merge | See "Source resolution rules" below. |
| `--delivery-col=<HEADER>` | `Delivery` | Header column to read; only used when the source has headers. |
| `--shipment-file=<value>` | unset → `<reports_dir>\vt11\` newest `.xlsx` | VL06O only. |
| `--shipment-col=<HEADER>` | `Shipment Number` | VL06O only. |

When `--delivery-file` is set, the legacy ZMDESNR + ListCheck merge is **replaced** (and the `_.csv` rename of consumed ListCheck files is skipped — we didn't consume them). When unset, behavior is unchanged.

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

## Validation Rules (fail-fast, before SAP connect)

| Condition | Error |
| --- | --- |
| `--by-delivery=true` and `--by-shipment=true` together | `--by-delivery=true and --by-shipment=true cannot both be set ...` |
| `--by-shipment=true` with `--tcode!=VL06O` | `--by-shipment is only supported for VL06O ...` |
| `--pre-export-back` with `--tcode!=ZMDESNR` | `--pre-export-back is only supported for ZMDESNR ...` |
| `--tab-number` with `--tcode!=ZMDESNR` | `--tab-number is only supported for ZMDESNR ...` |
| `--date-start` / `--date-end` not ISO | `Invalid --date-start value '...': expected ISO YYYY-MM-DD ...` |
| Loop run, no `--tcode` and no `[loop].tcode` | `Missing flag for tcode, enter with --tcode ...` |
| 149 loop run, no `--tcode-run-type` | `Missing flag for tcode-run-type, enter with --tcode-run-type=rcv\|mat\|tsp ...` |
| `--run-sequence` with any per-tcode flag | `<flag> can't be used with --run-sequence; sequence uses config.toml ...` |

The full list of per-tcode flags rejected by `--run-sequence`:

```
--tcode --layout --variant --export-type --tcode-run-type
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
tcode = "Y_DN3_47000149"
iterations = "2"
delay_seconds = "30"
param_tcode_run_type = "mat"
```

CLI equivalent (no config needed):

```bash
./sap_automation.exe --run-loop --tcode=y_dn3_47000149 \
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

### ZMDESNR with deliveries from another tcode's output dir

Disables the post-export back-send (which `[tcode.ZMDESNR].pre_export_back = "true"` would otherwise do) and selects the inventory-view tab (2) instead of the configured one:

```bash
./sap_automation.exe --tcode=zmdesnr --variant=INV_VIEW_EPDC --layout=mg_view \
  --by-delivery=true --delivery-file=zvt11 \
  --pre-export-back=false --tab-number=2
```

### Sequence (config-driven) with a faster cadence

```bash
./sap_automation.exe --run-sequence --iterations=5 --interval-seconds=10
```

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
