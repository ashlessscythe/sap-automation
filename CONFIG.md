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

[tcode.ZMDESNR]
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
- `serial_number`: Serial number for ZMDESNR
- `tab_number`: Tab number for ZMDESNR
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

[tcode.ZMDESNR]
variant = "serial_variant"
layout = "serial_layout"
```

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
