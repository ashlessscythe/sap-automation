# Keep-Awake Feature Usage

The SAP Automation tool now includes a keep-awake feature that prevents the Windows system from going to sleep during unattended operations.

## Overview

The keep-awake functionality uses Windows API calls to prevent the system from entering sleep mode while the automation is running. This is particularly useful for long-running unattended operations that might otherwise be interrupted by system sleep.

## Usage

### Command Line Interface

The keep-awake feature is available as a command-line flag for unattended operations:

```bash
# Run loop with keep-awake enabled
sap_automation.exe run-loop --keep-awake

# Run sequence with keep-awake enabled
sap_automation.exe run-sequence --keep-awake

# Skip SAP connection check and keep system awake
sap_automation.exe run-loop --skip-sap-check --keep-awake
```

### How It Works

1. **Initial Setup**: When `--keep-awake` is specified, the system calls `SetThreadExecutionState` with flags to prevent sleep
2. **Background Thread**: A background thread is started that periodically refreshes the keep-awake state every 60 seconds
3. **Automatic Cleanup**: When the program exits, the keep-awake state is automatically reset

### Technical Details

The implementation uses the Windows `SetThreadExecutionState` API with the following flags:

- `ES_CONTINUOUS`: The state remains in effect until explicitly changed
- `ES_SYSTEM_REQUIRED`: Prevents the system from entering sleep mode
- `ES_DISPLAY_REQUIRED`: Prevents the display from turning off

### Error Handling

If the keep-awake functionality fails to initialize:

- A warning message is displayed
- The automation continues to run normally
- The system may still go to sleep if configured to do so

### Best Practices

1. **Use for Long Operations**: Enable keep-awake for operations that run for extended periods
2. **Monitor System Resources**: Keep-awake prevents sleep but doesn't affect other power management
3. **Test in Your Environment**: Verify that keep-awake works correctly in your specific Windows configuration

### Example Scenarios

- **Overnight Processing**: Run SAP automation overnight without worrying about system sleep
- **Batch Operations**: Process large datasets without interruption
- **Scheduled Tasks**: Ensure scheduled operations complete even if they take longer than expected

### Troubleshooting

If keep-awake doesn't work as expected:

1. **Check Permissions**: Ensure the application has sufficient privileges
2. **Windows Power Settings**: Verify that Windows power settings allow the application to prevent sleep
3. **Group Policy**: Check if corporate group policies restrict power management APIs
4. **Antivirus Software**: Some antivirus software may block power management API calls

### Demo

To see the keep-awake functionality in action, you can run the demo:

```bash
cargo run --example keep_awake_demo
```

This will demonstrate the keep-awake functionality for 30 seconds.
