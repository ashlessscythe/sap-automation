# Breakpoint Examples for SAP Automation Debugging

This document provides specific examples of where to place breakpoints to debug common issues in the SAP automation program.

## Debugging VL06O Export Functionality

### Issue: Export not generating files

Place breakpoints at:

```rust
// In src/vl06o.rs - At the beginning of the export function
pub fn run_export(session: &GuiSession, params: &VL06OParams) -> Result<bool> {
    println!("Running VL06O export..."); // <-- BREAKPOINT HERE

    // ...
}

// In src/utils/sap_file_utils.rs - When generating the file path
pub fn get_tcode_file_path(tcode: &str, ext: &str) -> (String, String) {
    let reports_dir = get_reports_dir(); // <-- BREAKPOINT HERE

    // ...
}

// In src/utils/sap_file_utils.rs - When saving the file
pub fn save_sap_file(session: &GuiSession, file_path: &str, file_name: &str, close_export_file: Option<bool>) -> Result<bool> {
    let close_export = close_export_file.unwrap_or(false);
    println!("Exporting data from SAP...."); // <-- BREAKPOINT HERE

    // ...
}
```

### Issue: SAP GUI interaction problems

Place breakpoints at:

```rust
// In src/vl06o.rs - When selecting a variant
if let Some(variant_name) = &params.sap_variant_name {
    if !variant_name.is_empty() && !variant_select(session, &params.t_code, variant_name)? {
        println!(
            "Failed to select variant '{}' for tCode '{}'",
            variant_name, params.t_code
        ); // <-- BREAKPOINT HERE

        // ...
    }
}

// In src/vl06o.rs - When pasting shipment numbers
let paste_result = paste_values_with_scroll(
    session,
    1, // Window index
    table_id,
    &params.shipment_numbers,
    batch_size
)?; // <-- BREAKPOINT HERE

if !paste_result {
    println!("Failed to paste shipment numbers"); // <-- BREAKPOINT HERE
    return Ok(false);
}
```

## Debugging SAP Connection Issues

### Issue: SAP connection failing

Place breakpoints in src/main.rs:

```rust
// When initializing COM environment
match SAPComInstance::new() {
    Ok(instance) => {
        com_instance = Some(instance); // <-- BREAKPOINT HERE

        // ...
    }
    Err(e) => {
        eprintln!("Warning: Couldn't initialize COM environment: {}", e); // <-- BREAKPOINT HERE

        // ...
    }
}

// When getting connection
match get_or_create_connection(engine.as_ref().unwrap()) {
    Ok(conn) => {
        connection = Some(conn); // <-- BREAKPOINT HERE

        // ...
    }
    Err(e) => {
        eprintln!("Warning: Error getting SAP connection: {}", e); // <-- BREAKPOINT HERE

        // ...
    }
}
```

## Debugging Date Update Functionality

### Issue: Date not updating correctly

Place breakpoints in src/vl06o.rs:

```rust
// In run_date_update function
// When changing the date
if let Ok(txt) = session.find_by_id(r"wnd[0]/usr/tabsTAXI_TABSTRIP_OVERVIEW/tabpT\01/ssubSUBSCREEN_BODY:SAPMV50A:1102/ctxtLIKP-WADAT".to_string()) {
    if let Some(text_field) = txt.downcast::<GuiCTextField>() {
        text_field.set_text(target_date_str.clone())?; // <-- BREAKPOINT HERE
    }
}

// When saving changes
if let Ok(wnd) = session.find_by_id("wnd[0]".to_string()) {
    if let Some(main_window) = wnd.downcast::<GuiMainWindow>() {
        main_window.send_v_key(11)?; // Ctrl+S to save
        println!("Saved changes for delivery {}", delivery_number); // <-- BREAKPOINT HERE
    }
}
```

## Using Conditional Breakpoints

For more targeted debugging, you can set conditional breakpoints:

1. **Break only for specific delivery numbers**:

   ```
   delivery_number == "1234567890"
   ```

2. **Break when a specific error occurs**:

   ```
   status_msg.contains("Error")
   ```

3. **Break after processing a certain number of items**:
   ```
   counter > 5
   ```

## Using Logpoints

Instead of stopping execution, you can use logpoints to print information:

1. **Log delivery numbers being processed**:

   ```
   Processing delivery: {delivery_number}
   ```

2. **Log date changes**:

   ```
   Changed date from {original_date} to {target_date_str} for delivery {delivery_number}
   ```

3. **Log SAP connection status**:
   ```
   SAP connection status: {sap_connected}
   ```

## How to Use These Examples

1. Open the relevant file in VS Code
2. Navigate to the line where you want to add a breakpoint
3. Click in the gutter (left margin) next to the line number
4. For conditional breakpoints or logpoints, right-click on the breakpoint and select "Edit Breakpoint"
5. Start debugging with F5
