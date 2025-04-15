use anyhow::Result;
use crossterm::{
    execute,
    terminal::{Clear, ClearType},
};
use dialoguer::{Input, Select};
use std::io::{self, Write};
use std::thread;
use std::time::Duration;
use windows::core;

use crate::utils::config_ops::get_reports_dir;
use crate::utils::excel_fileread_utils::{read_excel_file, ExcelValue};
use crate::utils::excel_path_utils::{get_excel_file_path, resolve_path};

/// Read a specific column from an Excel file and return the values as strings
///
/// # Arguments
///
/// * `file_path` - Path to the Excel file
/// * `sheet_name` - Name of the sheet to read
/// * `column_header` - Header of the column to read
///
/// # Returns
///
/// * `Result<Vec<String>>` - Vector of string values from the column
pub fn read_excel_column(
    file_path: &str,
    sheet_name: &str,
    column_header: &str,
) -> core::Result<Vec<String>> {
    // Resolve the file path (handle slugs and non-full paths)
    let resolved_path = resolve_path(file_path);

    // Read the Excel file
    let df = match read_excel_file(&resolved_path, sheet_name) {
        Ok(df) => df,
        Err(e) => {
            println!("Error reading Excel file: {}", e);
            return Ok(Vec::new());
        }
    };

    // Check if the column header exists
    if !df.headers.contains(&column_header.to_string()) {
        println!(
            "Column header '{}' not found in sheet '{}'",
            column_header, sheet_name
        );
        println!("Available headers: {:?}", df.headers);
        return Ok(Vec::new());
    }

    // Get the column index
    let column_index = df.headers.iter().position(|h| h == column_header).unwrap();

    // Extract the column values
    let mut column_values = Vec::new();
    for row in &df.data {
        if column_index < row.len() {
            let value = match &row[column_index] {
                ExcelValue::String(s) => s.clone(),
                ExcelValue::Float(f) => f.to_string(),
                ExcelValue::Int(i) => i.to_string(),
                ExcelValue::Bool(b) => b.to_string(),
                ExcelValue::Empty => String::new(),
            };

            // Only add non-empty values
            if !value.is_empty() {
                column_values.push(value);
            }
        }
    }

    Ok(column_values)
}

pub fn handle_read_excel_file() -> Result<()> {
    clear_screen();
    println!("Read Excel File");
    println!("==============");

    // Get reports directory as default location
    let reports_dir = get_reports_dir();

    // Ask for Excel file path or directory
    let path_input: String = Input::new()
        .with_prompt("Enter Excel file path or directory (leave empty to use reports directory)")
        .allow_empty(true)
        .interact()
        .unwrap();

    // Use reports directory if input is empty
    let path_or_dir = if path_input.is_empty() {
        reports_dir
    } else {
        path_input
    };

    // Get Excel file path (either directly or by selection from directory)
    let file_path = match get_excel_file_path(&path_or_dir) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: {}", e);
            thread::sleep(Duration::from_secs(2));
            return Ok(());
        }
    };

    println!("\nSelected file: {}", file_path);

    // Ask for sheet name
    let sheet_name: String = Input::new()
        .with_prompt("Enter sheet name")
        .default("Sheet1".to_string())
        .interact()
        .unwrap();

    // Read the Excel file first to get headers
    let df = match read_excel_file(&file_path, &sheet_name) {
        Ok(df) => {
            // Display headers
            println!("\nHeaders found in file:");
            for (i, header) in df.headers.iter().enumerate() {
                println!("  {}: {}", i + 1, header);
            }

            // Display row count
            println!("\nTotal rows: {}", df.data.len());

            df
        }
        Err(e) => {
            eprintln!("Error reading Excel file: {}", e);
            thread::sleep(Duration::from_secs(2));
            return Ok(());
        }
    };

    // Keep track of selected columns
    let mut selected_columns: Vec<String> = Vec::new();

    // Loop for column selection
    loop {
        // Display currently selected columns
        if !selected_columns.is_empty() {
            println!("\nCurrently selected columns:");
            for (i, col) in selected_columns.iter().enumerate() {
                println!("  {}: {}", i + 1, col);
            }

            // Display preview of selected columns data (first 5 rows)
            println!("\nPreview of selected data (first 5 rows):");

            // Convert column names to &str for get_columns
            let col_refs: Vec<&str> = selected_columns.iter().map(|s| s.as_str()).collect();

            if let Some(columns) = df.get_columns(&col_refs) {
                // Calculate column widths based on content
                let mut col_widths = Vec::new();
                for (i, col_name) in selected_columns.iter().enumerate() {
                    // Start with header width
                    let mut max_width = col_name.len();

                    // Check data widths (up to 5 rows)
                    if i < columns.len() {
                        let row_count = std::cmp::min(5, columns[i].len());
                        for row_idx in 0..row_count {
                            if row_idx < columns[i].len() {
                                let value_width = columns[i][row_idx].to_string().len();
                                max_width = std::cmp::max(max_width, value_width);
                            }
                        }
                    }

                    // Add padding
                    col_widths.push(max_width + 2); // +2 for padding
                }

                // Print header row with color
                print!("\x1b[1;36m| "); // Bright cyan, bold
                for (i, col) in selected_columns.iter().enumerate() {
                    let width = col_widths[i];
                    print!("{:<width$} | ", col, width = width);
                }
                println!("\x1b[0m"); // Reset color

                // Print separator with double line for better visibility
                print!("\x1b[1;36m|"); // Bright cyan, bold
                for width in &col_widths {
                    for _ in 0..(width + 3) {
                        // +3 for the " | " separator
                        print!("=");
                    }
                    print!("|");
                }
                println!("\x1b[0m"); // Reset color

                // Print data rows (up to 5)
                let row_count = if columns.is_empty() {
                    0
                } else {
                    columns[0].len()
                };
                let display_rows = std::cmp::min(5, row_count);

                for row_idx in 0..display_rows {
                    print!("| ");
                    for (col_idx, col) in columns.iter().enumerate() {
                        let width = col_widths[col_idx];
                        if row_idx < col.len() {
                            let value = col[row_idx].to_string();
                            print!("{:<width$} | ", value, width = width);
                        } else {
                            print!("{:<width$} | ", "", width = width);
                        }
                    }
                    println!();
                }

                // Print bottom separator
                print!("|");
                for width in &col_widths {
                    for _ in 0..(width + 3) {
                        // +3 for the " | " separator
                        print!("-");
                    }
                    print!("|");
                }
                println!();
            }
        }

        // Create a list of options including headers, done selecting, and exit options
        let mut options = Vec::new();
        for header in &df.headers {
            options.push(format!("Select column: {}", header));
        }
        options.push("Done selecting - Format for SAP".to_string());
        options.push("Exit back to main menu".to_string());

        // Show selection dialog
        let selection = Select::new()
            .with_prompt("Choose a column or action")
            .items(&options)
            .default(0)
            .interact()
            .unwrap();

        // Check if user selected exit option
        if selection == options.len() - 1 {
            println!("Exiting to main menu...");
            thread::sleep(Duration::from_secs(1));
            return Ok(());
        }

        // Check if user selected "Done selecting"
        if selection == options.len() - 2 {
            // Check if any columns were selected
            if selected_columns.is_empty() {
                println!("\nNo columns selected. Please select at least one column.");
                thread::sleep(Duration::from_secs(2));
                continue;
            }

            // Convert column names to &str for format_columns_for_sap
            let col_refs: Vec<&str> = selected_columns.iter().map(|s| s.as_str()).collect();

            // Try to format columns for SAP
            match df.format_columns_for_sap(&col_refs) {
                Some(formatted) => {
                    println!("\nFormatted data for SAP multi-value field:");
                    println!("{}", formatted);

                    println!("\nThis data can be pasted directly into SAP multi-value fields.");
                    break; // Success, exit the loop
                }
                None => {
                    println!(
                        "\nError: One or more selected columns not found. This shouldn't happen."
                    );
                    thread::sleep(Duration::from_secs(2));
                }
            }
        } else {
            // User selected a column, add it to selected columns if not already there
            let selected_header = &df.headers[selection];
            if !selected_columns.contains(selected_header) {
                selected_columns.push(selected_header.clone());
                println!("\nAdded column: {}", selected_header);
            } else {
                println!("\nColumn already selected: {}", selected_header);
            }
            thread::sleep(Duration::from_secs(1));
        }
    }

    println!("\nPress Enter to continue...");
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();

    Ok(())
}

// Helper function to clear the screen
fn clear_screen() {
    execute!(std::io::stdout(), Clear(ClearType::All)).unwrap();
}
