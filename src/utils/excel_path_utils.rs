use std::path::{Path, PathBuf};
use std::fs::{self, DirEntry};
use std::io::{self, Error, ErrorKind};
use std::time::SystemTime;
use dialoguer::{Select, Input};
use anyhow::{Result, Context};

/// Lists Excel files in a directory, sorted by modification time (newest first)
pub fn list_excel_files(dir_path: &str) -> Result<Vec<DirEntry>> {
    let path = Path::new(dir_path);
    
    // Check if path exists and is a directory
    if !path.exists() {
        return Err(Error::new(ErrorKind::NotFound, 
            format!("Directory not found: {}", dir_path)).into());
    }
    
    if !path.is_dir() {
        return Err(Error::new(ErrorKind::InvalidInput, 
            format!("Not a directory: {}", dir_path)).into());
    }
    
    // Read directory entries
    let entries = fs::read_dir(path)
        .with_context(|| format!("Failed to read directory: {}", dir_path))?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            
            // Check if it's a file with .xlsx or .xls extension
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if ext_str == "xlsx" || ext_str == "xls" {
                        return Some(entry);
                    }
                }
            }
            None
        })
        .collect::<Vec<_>>();
    
    // Sort by modification time (newest first)
    let mut sorted_entries = entries;
    sorted_entries.sort_by(|a, b| {
        let time_a = a.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        let time_b = b.metadata().and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH);
        time_b.cmp(&time_a) // Reverse order for newest first
    });
    
    Ok(sorted_entries)
}

/// Selects an Excel file from a directory using dialoguer
/// If the directory contains more than 10 files, only the 10 newest are shown
/// with an option to enter a custom path
pub fn select_excel_file(dir_path: &str) -> Result<String> {
    // List Excel files in the directory
    let entries = list_excel_files(dir_path)?;
    
    if entries.is_empty() {
        return Err(Error::new(ErrorKind::NotFound, 
            format!("No Excel files found in directory: {}", dir_path)).into());
    }
    
    // Determine how many files to show
    let show_limited = entries.len() > 10;
    let display_entries = if show_limited {
        &entries[0..10] // Take only the 10 newest
    } else {
        &entries
    };
    
    // Create selection items
    let mut items = display_entries
        .iter()
        .map(|entry| {
            let filename = entry.file_name().to_string_lossy().to_string();
            let modified = entry.metadata()
                .and_then(|m| m.modified())
                .map(|time| {
                    time.duration_since(SystemTime::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0)
                })
                .unwrap_or(0);
            
            // Format the modified time as a date string
            let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(modified as i64, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| "Unknown date".to_string());
            
            format!("{} ({})", filename, datetime)
        })
        .collect::<Vec<_>>();
    
    // Add option for custom path
    items.push("Enter a custom path...".to_string());
    
    // Add message about limited display if needed
    let prompt = if show_limited {
        format!("Select an Excel file [showing 10 newest files from {}]", dir_path)
    } else {
        format!("Select an Excel file from {}", dir_path)
    };
    
    // Show selection dialog
    let selection = Select::new()
        .with_prompt(&prompt)
        .items(&items)
        .default(0)
        .interact()
        .with_context(|| "Failed to display selection dialog")?;
    
    // Handle selection
    if selection < display_entries.len() {
        // User selected a file from the list
        let selected_entry = &display_entries[selection];
        let path = selected_entry.path();
        Ok(path.to_string_lossy().to_string())
    } else {
        // User wants to enter a custom path
        let custom_path: String = Input::new()
            .with_prompt("Enter the full path to an Excel file")
            .interact()
            .with_context(|| "Failed to get custom path input")?;
        
        // Validate the custom path
        let custom_path_buf = PathBuf::from(&custom_path);
        if !custom_path_buf.exists() {
            return Err(Error::new(ErrorKind::NotFound, 
                format!("File not found: {}", custom_path)).into());
        }
        
        if !custom_path_buf.is_file() {
            return Err(Error::new(ErrorKind::InvalidInput, 
                format!("Not a file: {}", custom_path)).into());
        }
        
        // Check extension
        if let Some(ext) = custom_path_buf.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if ext_str != "xlsx" && ext_str != "xls" {
                return Err(Error::new(ErrorKind::InvalidInput, 
                    format!("Not an Excel file: {}", custom_path)).into());
            }
        } else {
            return Err(Error::new(ErrorKind::InvalidInput, 
                format!("Not an Excel file (no extension): {}", custom_path)).into());
        }
        
        Ok(custom_path)
    }
}

/// Gets an Excel file path, either from a provided path or by selecting from a directory
pub fn get_excel_file_path(path_or_dir: &str) -> Result<String> {
    let path = Path::new(path_or_dir);
    
    if path.exists() {
        if path.is_dir() {
            // It's a directory, let the user select a file
            select_excel_file(path_or_dir)
        } else if path.is_file() {
            // It's a file, check if it's an Excel file
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if ext_str == "xlsx" || ext_str == "xls" {
                    Ok(path_or_dir.to_string())
                } else {
                    Err(Error::new(ErrorKind::InvalidInput, 
                        format!("Not an Excel file: {}", path_or_dir)).into())
                }
            } else {
                Err(Error::new(ErrorKind::InvalidInput, 
                    format!("Not an Excel file (no extension): {}", path_or_dir)).into())
            }
        } else {
            Err(Error::new(ErrorKind::InvalidInput, 
                format!("Not a file or directory: {}", path_or_dir)).into())
        }
    } else {
        Err(Error::new(ErrorKind::NotFound, 
            format!("Path not found: {}", path_or_dir)).into())
    }
}
