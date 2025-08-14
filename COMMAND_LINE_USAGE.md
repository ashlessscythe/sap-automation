# SAP Automation - Command Line Usage

## Overview

The SAP Automation tool now supports both interactive and command-line modes. When run without command-line arguments, it operates in the traditional interactive mode with the TUI menu system. When run with specific command-line arguments, it operates in unattended mode for automation scenarios.

## Command Line Options

### Main Help

```bash
./sap_automation.exe --help
```

### Run Loop Configuration (Unattended)

```bash
./sap_automation.exe run-loop [OPTIONS]
```

**Options:**

- `--skip-sap-check`: Skip SAP connection check (for testing)
- `-h, --help`: Show help for this command

**Example:**

```bash
./sap_automation.exe run-loop
./sap_automation.exe run-loop --skip-sap-check
```

### Run Sequence Configuration (Unattended)

```bash
./sap_automation.exe run-sequence [OPTIONS]
```

**Options:**

- `--skip-sap-check`: Skip SAP connection check (for testing)
- `-h, --help`: Show help for this command

**Example:**

```bash
./sap_automation.exe run-sequence
./sap_automation.exe run-sequence --skip-sap-check
```

## How It Works

### Interactive Mode (Default)

When you run `./sap_automation.exe` without any arguments, the application starts in interactive mode:

- Shows the TUI menu system
- Requires user input for navigation
- Provides full access to all features
- Requires pressing Enter to continue after operations

### Unattended Mode

When you run `./sap_automation.exe run-loop` or `./sap_automation.exe run-sequence`:

- **No user interaction required** - runs completely automatically
- Uses the configuration from your `config.toml` file
- Executes the specified operation without prompts
- Provides progress information via console output
- Exits automatically when complete

## Configuration Requirements

### For Loop Execution

Your `config.toml` must have a `[loop]` section:

```toml
[loop]
tcode = "Y_DN3_47000149"
iterations = "2"
delay_seconds = "30"
param_tcode_run_type = "mat"
```

### For Sequence Execution

Your `config.toml` must have a `[sequence]` section:

```toml
[sequence]
options = ["9", "7"]
iterations = "1"
delay_seconds = "60"
interval_seconds = "10"
```

## Use Cases

### Automation Scripts

```bash
# Run in a batch script
./sap_automation.exe run-loop > loop_output.log 2>&1

# Run in a scheduled task
./sap_automation.exe run-sequence --skip-sap-check
```

### CI/CD Pipelines

```bash
# Run as part of a build process
./sap_automation.exe run-loop
if [ $? -eq 0 ]; then
    echo "Loop execution successful"
else
    echo "Loop execution failed"
    exit 1
fi
```

### Testing

```bash
# Test without SAP connection
./sap_automation.exe run-loop --skip-sap-check
```

## Error Handling

- If configuration is missing or invalid, the command will exit with an error code
- If SAP connection fails, the command will exit with an error code
- All errors are displayed to the console
- No interactive prompts will appear in unattended mode

## Benefits

1. **Automation**: Can be run from scripts, scheduled tasks, or CI/CD pipelines
2. **No User Interaction**: Completely hands-off operation
3. **Logging**: All output goes to console for easy logging
4. **Error Handling**: Clear error messages and exit codes
5. **Configuration Driven**: Uses existing config.toml files
6. **Backward Compatible**: Interactive mode remains unchanged

## Examples

### Windows Batch File

```batch
@echo off
echo Starting SAP Automation Loop
sap_automation.exe run-loop
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
$result = & ".\sap_automation.exe" run-sequence
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
if ./sap_automation.exe run-loop; then
    echo "Loop completed successfully"
else
    echo "Loop failed with error code $?"
    exit 1
fi
```

## Notes

- The `--skip-sap-check` option is useful for testing or when you want to bypass SAP connection validation
- All timing and iteration settings come from your configuration file
- The application will automatically handle SAP login if required
- Progress information is displayed to help monitor long-running operations
- Ctrl+C can still be used to interrupt execution if needed
