# SAP Automation

A Rust-based utility for automating SAP GUI interactions.

## Description

This project provides tools for automating SAP GUI operations using Rust. It leverages the Windows COM interface to interact with the SAP GUI Scripting API, allowing for programmatic control of SAP sessions.

Key features include:

- Automated SAP login with credential management
- Secure storage of encrypted credentials
- Handling of multiple logon scenarios
- Interaction with SAP GUI controls (buttons, text fields, checkboxes, etc.)
- Management of popups and error messages
- Support for transaction code execution and verification
- Data export capabilities

## Dependencies

This project relies on the following key dependencies:

- `sap-scripting` - A Rust library for interacting with SAP GUI Scripting API, created by Lily Hopkins (https://github.com/lilopkins/sap-scripting-rs)
- `windows` - For Windows COM interface integration
- `aes-gcm` and `base64` - For secure credential encryption
- `dialoguer` and `crossterm` - For terminal UI components

## Configuration

### Setting Up Your Configuration

The application uses a `config.toml` file for configuration. To get started:

1. Copy the example configuration file:

   ```
   copy config.toml.example config.toml
   ```

2. Edit the `config.toml` file to match your environment and requirements

### Configuration Parameters

The configuration file is divided into sections:

#### [build] Section

- `target` - Specifies the build target architecture (e.g., `i686-pc-windows-msvc` for 32-bit Windows)

#### [sap_config] Section

##### Core Parameters

- `instance_id` - SAP instance identifier (default: "rs")
- `reports_dir` - Directory where reports will be saved (default: User's Documents\Reports folder)
- `tcode` - Default transaction code to execute (e.g., "VL06O")
- `variant` - SAP variant name to use with the transaction
- `layout` - Layout name for SAP screens
- `column_name` - Column name for specific operations
- `date_range_start` and `date_range_end` - Date range for reports (format: MM/DD/YYYY)

##### Loop Operation Parameters

- `loop_tcode` - Transaction code to use for loop operations (defaults to `tcode` if not specified)
- `loop_iterations` - Number of times to repeat the operation
- `loop_delay_seconds` - Delay between iterations in seconds
- `loop_vt11_randomarg` - Random argument for VT11 transaction in loop mode
- `loop_param_argname` - Parameter name for loop operations

##### Transaction-Specific Parameters

You can add transaction-specific parameters by prefixing them with the transaction code:

- `VL06O_parameter_name` - Parameter specific to VL06O transaction
- `VT11_parameter_name` - Parameter specific to VT11 transaction

## Usage

The main binary provides a simple interface for logging into SAP:

```
cargo run --bin sap_login
```

This will present a menu with options to log in to SAP, with support for saving encrypted credentials for future use.

The application looks for credentials in the user's Documents folder under `SAP/cryptauth_*.txt`. The instance ID can be configured via the `SAP_INSTANCE_ID` environment variable.

## Line Endings

This project uses Git's line ending normalization to ensure consistent behavior across different operating systems. The `.gitattributes` file configures:

- Automatic line ending normalization for most text files
- LF (Unix-style) line endings for shell scripts (\*.sh)
- CRLF (Windows-style) line endings for batch files (\*.bat)

If you encounter line ending issues when committing changes, make sure you have the `.gitattributes` file in your repository.
