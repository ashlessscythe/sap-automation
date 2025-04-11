use std::io::{self, Write};
use std::time::Duration;
use std::thread;
use dialoguer::{Select, Input};
use crossterm::{execute, terminal::{Clear, ClearType}};
use anyhow::Result;

use crate::utils::sap_file_utils::get_reports_dir;
use crate::utils::excel_fileread_utils::{read_excel_file, ExcelValue};
use crate::utils::excel_path_utils::get_excel_file_path;

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
        },
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
                // Print header row
                print!("| ");
                for col in &selected_columns {
                    print!("{} | ", col);
                }
                println!();
                
                // Print separator
                print!("|");
                for col in &selected_columns {
                    for _ in 0..col.len() + 2 {
                        print!("-");
                    }
                    print!("|");
                }
                println!();
                
                // Print data rows (up to 5)
                let row_count = if columns.is_empty() { 0 } else { columns[0].len() };
                let display_rows = std::cmp::min(5, row_count);
                
                for row_idx in 0..display_rows {
                    print!("| ");
                    for col in &columns {
                        if row_idx < col.len() {
                            print!("{} | ", col[row_idx].to_string());
                        } else {
                            print!("  | ");
                        }
                    }
                    println!();
                }
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
                },
                None => {
                    println!("\nError: One or more selected columns not found. This shouldn't happen.");
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
