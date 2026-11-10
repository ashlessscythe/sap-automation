# SAP Automation Configuration Guide

This document describes the configuration system for the SAP Automation tool.

## Configuration File Structure

The configuration file (`config.toml`) uses the TOML format and is organized into several sections:

### Build Section

Contains build-related configuration:

```toml
[build]
target = "i686-pc-windows-msvc"
```

### Global Section

Contains global configuration settings:

```toml
[global]
instance_id = "AB2"
reports_dir = "C:\\temp\\reports"
default_tcode = "VT11"
```

- `instance_id`: The SAP instance ID to connect to
- `reports_dir`: Directory where reports will be saved
- `default_tcode`: Default transaction code to use

### TCode Sections

Contains configuration for specific transaction codes:

```toml
[tcode.VT11]
variant = "testing_7"
layout = "my_layout"
date_range_start = "01/01/2023"
date_range_end = "12/31/2023"
by_date = "true"

[tcode.VL06O]
variant = "delivery_layout"
layout = "delivery_view"
column_name = "Shipment Number"

[tcode.ZEXSNR]
variant = "serial_variant"
layout = "serial_layout"
tab_number = "2"
serial_number = "SN12345"
```

Each TCode section can have the following parameters:

- `variant`: SAP variant to use
- `layout`: Layout to apply
- `column_name`: Column name for data extraction
- `date_range_start`: Start date for date range
- `date_range_end`: End date for date range
- `by_date`: Whether to filter by date
- `serial_number`: Serial number for ZEXSNR
- `tab_number`: Tab number for ZEXSNR
- Additional custom parameters as needed

### Loop Section

Contains configuration for loop operations:

```toml
[loop]
tcode = "VT11"
iterations = "4"
delay_seconds = "15"
param_list_header = "Shipment Number"
param_set_field = "date"
param_set_value = ""
```

- `tcode`: Transaction code to use in the loop
- `iterations`: Number of iterations to run
- `delay_seconds`: Delay between iterations
- Additional parameters with `param_` prefix

### ABC Headless Section (`[abc.mustgo]`)

Optional. Used only by `--rname=mustgo` (and the ABC Headless > mustgo menu entry). Every key is optional; a missing,
empty or wrongly typed key falls back to the default. Precedence: CLI flag > `[abc.mustgo]` > `[global]` > default
(`--reports-dir` / `--date-format` / `--ignore-display-error` are the CLI side).

```toml
[abc.mustgo]
vt11_variant = "XD_ALL_EPDC_"   # VT11 variant (carries the shipment selection)
zmdesnr_variant = "mg_k2xx"     # ZMDESNR variant (carries the date)
zmdesnr_layout = "mg_view"      # ZMDESNR layout (19 columns)
lt23_layout = "to_mg"           # LT23 layout
lt23_create_layout = false      # true: create the LT23 layout from the built-in column list when it is missing
warehouse = "E01"               # LT23 warehouse
delete_exports = true           # delete each temporary export after it was loaded
ignore_display_error = true     # treat "no display authorization" as harmless (VT11)
```

`[abc.toyota]` (used by `--rname=toyota`) takes the same keys with the same defaults except `vt11_variant`, which defaults to
`FG_TOYO_XD`:

```toml
[abc.toyota]
vt11_variant = "FG_TOYO_XD"
```

`[abc.misc]` (used by `--rname=misc`) takes `vt11_variant` (default `MISC_EPDC_`), `vt11_layout` (`scorecard`), `vl06o_layout`
(default `inv`), `delete_exports` and `ignore_display_error`; `misc` does not call ZMDETPC, so `zmdetpc_variant` is ignored.

```toml
[abc.misc]
vt11_variant = "MISC_EPDC_"
vl06o_layout = "inv"
```

| Key | Default |
| --- | --- |
| `vt11_variant` | `XD_ALL_EPDC_` |
| `zmdesnr_variant` | `mg_k2xx` |
| `zmdesnr_layout` | `mg_view` |
| `lt23_layout` | `to_mg` |
| `lt23_create_layout` | `false` (a missing layout fails the run; the creation path has not been run live) |
| `warehouse` | `E01` |
| `delete_exports` | `true` |
| `ignore_display_error` | `true` |

`reports_dir` and `date_format` are not part of the section: they come from `--reports-dir` / `--date-format`, else
`[global]`. **`[global].date_format` must match the date format of the SAP user**: the same text is typed into VT11
(`K_DATEN`), and it is also the format used to read dates from the exports (ISO dates, as seen in the real exports, are
accepted whichever name is set). The built-in default is `mm/dd/yyyy`.

### ABC Headless Section (`[abc.xdock]`, `[abc.toyota]`, `[abc.misc]`)

Optional, same rules as `[abc.mustgo]` (CLI flag > `[abc.xdock]` > `[global]` > default; wrong or empty values fall back).
Used only by `--rname=xdock` (and the ABC Headless > xdock menu entry). `reports_dir` and `date_format` come from the CLI /
`[global]` as above.

```toml
[abc.xdock]
vt11_variant = "XD_ALL_EPDC_"   # VT11 variant
vt11_layout = "scorecard"       # VT11 layout (13 columns incl. shipment_number)
vl06o_layout = "xdock"          # VL06O layout (delivery, ship-to, name, external id, GM, plant, no. of packages)
zmdetpc_variant = "EPDC_TONY"   # ZMDETPC variant (taken from the toyota row of the spec; check it exists)
delete_exports = true           # delete each temporary export after it was loaded
ignore_display_error = true     # treat "no display authorization" as harmless (VT11)
# date_range_buffer = 3         # optional, any [abc.<report>]: days added each side of the run date range
                                # of every tcode that sets one (VT11, ZMDETPC). Unset: ZMDETPC gets 3 on a
                                # week run (WKnn), everything else 0. `--date-range-buffer` wins.
```

## Migration from Legacy Format

If you're upgrading from a previous version, you can use the migration tool to convert your configuration file to the new format:

```
cargo run --bin migrate_config
```

This will:

1. Create a backup of your existing config.toml as config.toml.bak
2. Convert your configuration to the new format
3. Save the new configuration to config.toml

## Configuration Management

The SAP Automation tool provides several ways to manage your configuration:

1. **Edit the config.toml file directly** - For advanced users who are comfortable with the TOML format
2. **Use the configuration menu in the application** - Navigate to "Configure SAP Parameters" in the main menu
3. **Use the configuration handlers in your code** - For programmatic configuration management

## Examples

### Basic Configuration

```toml
[build]
target = "i686-pc-windows-msvc"

[global]
instance_id = "JL2"
reports_dir = "C:\\temp\\reports"
default_tcode = "VT11"

[tcode.VT11]
variant = "testing_7"
layout = "my_layout"
```

### Multiple TCode Configuration

```toml
[global]
instance_id = "KM9"
reports_dir = "C:\\temp\\reports"

[tcode.VT11]
variant = "testing_7"
layout = "my_layout"

[tcode.VL06O]
variant = "delivery_layout"
layout = "delivery_view"

[tcode.ZEXSNR]
variant = "serial_variant"
layout = "serial_layout"
```

### Inbond Configuration

Used by menu **Inbond - Shipment to 149** (VL06O by shipment → 149 by delivery → paste-ready txt + Notepad):

```toml
[inbond]
vl06o_variant = "blank_"
variant_149 = "inb_ship"
layout_149 = "inb_ship"
layout_columns = [
  "Plant",
  "Delivery Number",
  "Material",
  "Quantity",
  "UOM",
  "Logistics Reference Number"
]
export_type = 1
open_notepad = true
```

Defaults match the values above when `[inbond]` is omitted. If `layout_149` is empty or not found in SAP, columns from `layout_columns` are set up via Change Layout and you are prompted to save under that name. This flow does not use `exec_rest`.

### Loop Configuration

```toml
[global]
instance_id = "SX4"
reports_dir = "C:\\temp\\reports"

[tcode.VT11]
variant = "testing_7"
layout = "my_layout"

[loop]
tcode = "VT11"
iterations = "4"
delay_seconds = "15"
param_list_header = "Shipment Number"
```
