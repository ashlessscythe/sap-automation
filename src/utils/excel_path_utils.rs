use std::path::{Path, PathBuf};
use std::fs::{self, DirEntry};
use std::io::{self, Error, ErrorKind};
use std::time::SystemTime;
use dialoguer::{Select, Input};
use anyhow::{Result, Context};
use windows::core;

use crate::utils::config_ops::get_reports_dir;

/// Private helper function to list files with specified extensions in a directory, sorted by modification time
fn list_files_with_extensions(dir_path: &str, extensions: &[&str]) -> io::Result<Vec<DirEntry>> {
    let path = Path::new(dir_path);
    
    // Check if path exists and is a directory
    if !path.exists() {
        return Err(Error::new(ErrorKind::NotFound, 
            format!("Directory not found: {}", dir_path)));
    }
    
    if !path.is_dir() {
        return Err(Error::new(ErrorKind::InvalidInput, 
            format!("Not a directory: {}", dir_path)));
    }
    
    // Read directory entries
    let entries = fs::read_dir(path)?
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            
            // Check if it's a file with one of the specified extensions
            if path.is_file() {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    if extensions.iter().any(|&e| ext_str == e.to_lowercase()) {
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

/// Lists Excel files in a directory, sorted by modification time (newest first)
pub fn list_excel_files(dir_path: &str) -> Result<Vec<DirEntry>> {
    // Use the helper function with Excel extensions
    list_files_with_extensions(dir_path, &["xlsx", "xls"])
        .with_context(|| format!("Failed to list Excel files in directory: {}", dir_path))
}

/// Gets the newest file with the specified extension in a directory
pub fn get_newest_file(dir_path: &str, extension: &str) -> core::Result<String> {
    // Use the helper function with the specified extension
    match list_files_with_extensions(dir_path, &[extension]) {
        Ok(files) => {
            if files.is_empty() {
                println!("No files with extension .{} found in directory: {}", extension, dir_path);
                Ok(String::new())
            } else {
                // Return the path of the newest file
                let newest_file = files[0].path();
                Ok(newest_file.to_string_lossy().to_string())
            }
        },
        Err(e) => {
            println!("Error listing files in {}: {}", dir_path, e);
            Ok(String::new())
        }
    }
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
        
        // Handle the custom path (could be a slug, relative path, or full path)
        let resolved_path = resolve_path(&custom_path);
        
        // Validate the resolved path
        let path_buf = PathBuf::from(&resolved_path);
        if !path_buf.exists() {
            return Err(Error::new(ErrorKind::NotFound, 
                format!("File not found: {}", resolved_path)).into());
        }
        
        if !path_buf.is_file() {
            return Err(Error::new(ErrorKind::InvalidInput, 
                format!("Not a file: {}", resolved_path)).into());
        }
        
        // Check extension
        if let Some(ext) = path_buf.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if ext_str != "xlsx" && ext_str != "xls" {
                return Err(Error::new(ErrorKind::InvalidInput, 
                    format!("Not an Excel file: {}", resolved_path)).into());
            }
        } else {
            return Err(Error::new(ErrorKind::InvalidInput, 
                format!("Not an Excel file (no extension): {}", resolved_path)).into());
        }
        
        Ok(resolved_path)
    }
}

/// Helper function to resolve a path that might be a slug or relative path
pub fn resolve_path(path_str: &str) -> String {
    let path = Path::new(path_str);
    
    // If the path exists as-is, return it
    if path.exists() {
        return path_str.to_string();
    }
    
    // Handle "../" at the beginning of the path (up one directory from reports dir)
    if path_str.starts_with("../") || path_str.starts_with("..\\") {
        let reports_dir = get_reports_dir();
        let reports_path = PathBuf::from(&reports_dir);
        
        // Get the parent directory of the reports directory
        if let Some(parent_dir) = reports_path.parent() {
            // Remove the "../" prefix and append the rest to the parent directory
            let rest_of_path = if path_str.starts_with("../") {
                &path_str[3..]
            } else { // starts_with("..\\")
                &path_str[3..]
            };
            
            let resolved_path = format!("{}\\{}", parent_dir.to_string_lossy(), rest_of_path);
            println!("Attempting to use parent directory path: {}", resolved_path);
            return resolved_path;
        }
    }
    
    // Check if it's a slug (no path separators)
    let needles = vec!["\\", "/", "\\\\"];
    if !needles.iter().any(|n| path_str.contains(n)) {
        // It's a slug, try to resolve it relative to the reports directory
        let reports_dir = get_reports_dir();
        let resolved_path = format!("{}\\{}", reports_dir, path_str);
        println!("Attempting to use relative path: {}", resolved_path);
        return resolved_path;
    }
    
    // It has path separators but doesn't exist, return as-is
    path_str.to_string()
}

/// Gets an Excel file path, either from a provided path or by selecting from a directory
pub fn get_excel_file_path(path_or_dir: &str) -> Result<String> {
    // First, try to resolve the path if it's a slug or relative path
    let resolved_path = resolve_path(path_or_dir);
    let path = Path::new(&resolved_path);
    
    if path.exists() {
        if path.is_dir() {
            // It's a directory, let the user select a file
            select_excel_file(&resolved_path)
        } else if path.is_file() {
            // It's a file, check if it's an Excel file
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_string_lossy().to_lowercase();
                if ext_str == "xlsx" || ext_str == "xls" {
                    Ok(resolved_path)
                } else {
                    Err(Error::new(ErrorKind::InvalidInput, 
                        format!("Not an Excel file: {}", resolved_path)).into())
                }
            } else {
                Err(Error::new(ErrorKind::InvalidInput, 
                    format!("Not an Excel file (no extension): {}", resolved_path)).into())
            }
        } else {
            Err(Error::new(ErrorKind::InvalidInput, 
                format!("Not a file or directory: {}", resolved_path)).into())
        }
    } else {
        Err(Error::new(ErrorKind::NotFound, 
            format!("Path not found: {}", resolved_path)).into())
    }
}
