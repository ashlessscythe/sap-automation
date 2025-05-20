# SAP Automation Troubleshooting Guide

This guide provides solutions for common issues you might encounter when running and debugging the SAP automation program.

## Common Issues and Solutions

### 1. SAP Connection Failures

**Symptoms:**

- Program reports "SAP connection not available"
- Features are disabled
- Error messages about COM environment initialization

**Solutions:**

- Ensure SAP GUI is running before starting the program
- Verify SAP GUI Scripting is enabled in SAP (Transaction RZ11, parameter `sapgui/user_scripting`)
- Check Windows COM settings
- Try running as administrator
- Use the "Debug with SAP Mock" configuration to test program logic without SAP

**Debugging Approach:**

```rust
// Set breakpoints in main.rs at the SAP connection initialization:
match SAPComInstance::new() {
    Ok(instance) => {
        // Breakpoint here to confirm instance creation succeeded
    }
    Err(e) => {
        // Breakpoint here to examine the error
        eprintln!("Warning: Couldn't initialize COM environment: {}", e);
    }
}
```

### 2. File Export Issues

**Symptoms:**

- No files are generated when exporting data
- Program reports success but files are missing
- Permission errors when writing files

**Solutions:**

- Check if the reports directory exists and is writable
- Verify the path in the configuration
- Ensure no other process has locked the target file
- Check for Excel instances that might be holding file locks

**Debugging Approach:**

```rust
// In sap_file_utils.rs, set breakpoints to trace the file path generation:
pub fn get_tcode_file_path(tcode: &str, ext: &str) -> (String, String) {
    let reports_dir = get_reports_dir(); // Breakpoint here
    let tcode_dir = format!("{}\\\\{}", reports_dir, tcode);

    // Breakpoint here to check the constructed path
    if !Path::new(&tcode_dir).exists() {
        let _ = fs::create_dir_all(&tcode_dir); // Breakpoint here if directories need creation
    }
}
```

### 3. SAP GUI Interaction Problems

**Symptoms:**

- "Element not found" errors
- Unexpected behavior in SAP transactions
- Timeouts during operations

**Solutions:**

- Verify SAP screen IDs match what the code expects
- Increase wait times between operations
- Check if SAP layout or variant names match configuration
- Ensure SAP GUI version is compatible

**Debugging Approach:**

```rust
// In vl06o.rs, add breakpoints when interacting with SAP elements:
if let Ok(btn) = session.find_by_id("wnd[0]/usr/btnBUTTON6".to_string()) {
    if let Some(button) = btn.downcast::<GuiButton>() {
        // Breakpoint here before pressing the button
        button.press()?;
        // Breakpoint here after pressing to check for errors
    }
} else {
    // Breakpoint here if the button wasn't found
}
```

### 4. Configuration Issues

**Symptoms:**

- Default values being used instead of configured ones
- "Failed to load config" messages
- Unexpected behavior in transactions

**Solutions:**

- Verify config.toml exists and has correct format
- Check for typos in configuration keys
- Ensure the configuration file is in the correct location
- Use the configuration utilities in the program to set up proper configuration

**Debugging Approach:**

```rust
// Set breakpoints when loading configuration:
let config = crate::utils::config_types::SapConfig::load();

// Debug the config loading result
match &config {
    Ok(cfg) => println!("Config loaded successfully"), // Breakpoint here
    Err(e) => println!("Failed to load config: {}", e), // Breakpoint here
}
```

### 5. Date Format Issues

**Symptoms:**

- Date updates fail
- Error messages about invalid date formats
- Unexpected date conversions

**Solutions:**

- Check the date format in the configuration
- Ensure dates are formatted according to SAP's expectations
- Verify locale settings

**Debugging Approach:**

```rust
// In vl06o.rs, set breakpoints when formatting dates:
let format_str = if date_format.to_lowercase() == "yyyy-mm-dd" { "%Y-%m-%d" } else { "%m/%d/%Y" };

// Format target date for SAP
let target_date_str = params.target_date.format(format_str).to_string(); // Breakpoint here
```

### 6. Excel Integration Issues

**Symptoms:**

- Excel files don't open
- Excel crashes when processing files
- File format errors

**Solutions:**

- Check if Excel is installed and properly configured
- Verify file permissions
- Close any open Excel instances before operations
- Check for file locks

**Debugging Approach:**

```rust
// In sap_file_utils.rs, set breakpoints in the Excel handling code:
match close_excel_windows(Some(file_name)) {
    Ok(true) => println!("Excel closed successfully"), // Breakpoint here
    Ok(false) => println!("No Excel windows found to close"), // Breakpoint here
    Err(e) => println!("Error closing Excel: {:?}", e), // Breakpoint here
}
```

## Advanced Troubleshooting

### Memory Issues

If the program is using excessive memory or crashing:

1. Use the Memory view in VS Code debugger to monitor allocations
2. Look for large collections that might be growing unbounded
3. Check for resource leaks, especially with SAP COM objects

### Performance Problems

If operations are slow:

1. Add timing code around suspect operations:

   ```rust
   use std::time::Instant;
   let start = Instant::now();
   // operation here
   let duration = start.elapsed();
   println!("Operation took: {:?}", duration);
   ```

2. Use the VS Code profiler extension to identify bottlenecks

### Crash Debugging

If the program crashes:

1. Run with the debugger attached
2. Set the debugger to break on exceptions
3. Examine the call stack when the crash occurs
4. Check for null pointers or invalid memory access

## Getting More Information

When standard debugging isn't enough:

1. **Enable Logging**: Uncomment the logging initialization in main.rs:

   ```rust
   // Initialize logging
   pretty_env_logger::init();
   ```

2. **Set Environment Variables**: Add `RUST_LOG=debug` or `RUST_LOG=trace` to see more detailed logs

3. **Dump SAP Objects**: Add temporary code to print SAP object properties:

   ```rust
   if let Ok(properties) = session.properties() {
       println!("Session properties: {:?}", properties);
   }
   ```

4. **Check SAP Logs**: Review SAP system logs for scripting errors
