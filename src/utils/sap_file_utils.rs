use crate::utils::config_ops::get_reports_dir;
use crate::utils::sap_ctrl_utils::exist_ctrl;
use crate::utils::utils::generate_timestamp;
use sap_scripting::*;
use std::ffi::OsString;
use std::fs;
use std::os::windows::ffi::OsStringExt;
use std::path::Path;
use std::thread;
use std::time::Duration;
use windows::core::{Result, HSTRING, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FindWindowW, GetWindowTextW, SendMessageW, WM_CLOSE,
};

/// Directory exports for `tcode` go to. Without `rname_dir` this is the legacy
/// `{reports_dir}\\{tcode}`. A relative `rname_dir` (`./mustgo`, `mustgo`) goes under the
/// reports dir (`{reports_dir}\{rname_dir}\{tcode}`); an absolute one is used as is
/// (`{rname_dir}\{tcode}`).
pub fn resolve_export_dir(reports_dir: &str, rname_dir: Option<&str>, tcode: &str) -> String {
    let r = rname_dir.map(str::trim).filter(|r| !r.is_empty());
    let Some(r) = r else {
        return format!("{reports_dir}\\\\{tcode}");
    };
    let rp = Path::new(r);
    let base = if rp.is_absolute() {
        rp.to_path_buf()
    } else {
        // Reports dir may carry doubled backslashes from config; normalise for joining.
        let mut p = std::path::PathBuf::from(reports_dir.replace("\\\\", "\\"));
        for c in rp.components() {
            if !matches!(c, std::path::Component::CurDir) {
                p.push(c.as_os_str());
            }
        }
        p
    };
    base.join(tcode).to_string_lossy().into_owned()
}

/// Gets a file path for a specific tcode
///
/// This function generates a file path based on the tcode, timestamp, and extension.
/// The path follows the pattern: {reports_dir}\{tcode}\{timestamp}_{tcode}.{ext}
///
/// # Arguments
///
/// * `tcode` - The SAP transaction code
/// * `ext` - The file extension (without the dot)
///
/// # Returns
///
/// * `(String, String)` - A tuple containing (file_path, file_name)
pub fn get_tcode_file_path(tcode: &str, ext: &str) -> (String, String) {
    let reports_dir = get_reports_dir();
    let tcode_dir = resolve_export_dir(
        &reports_dir,
        crate::utils::cli_overrides::cli_overrides()
            .rname_dir
            .as_deref(),
        tcode,
    );

    // Create the directory if it doesn't exist
    if !Path::new(&tcode_dir).exists() {
        let _ = fs::create_dir_all(&tcode_dir);
    }

    let timestamp = generate_timestamp();
    let file_name = format!("{timestamp}_{tcode}.{ext}");
    let file_path = tcode_dir;

    (file_path, file_name)
}

/// Closes Excel windows that match the given file name
///
/// This function uses the Windows API to find Excel windows with titles
/// containing the specified file name, and sends a close message to them.
///
/// # Arguments
///
/// * `file_name` - Optional file name to match in the window title
///
/// # Returns
///
/// * `Result<bool>` - Ok(true) if at least one window was closed, Ok(false) if no matching
///   windows were found or if the specified file was not found
pub fn close_excel_windows(file_name: Option<&str>) -> Result<bool> {
    println!("Attempting to close Excel windows...");

    // If no filename is provided, close all Excel windows
    if file_name.is_none() {
        return close_all_excel_windows();
    }

    // If a filename is provided, first check if it exists
    let file_exists = check_excel_file_exists(file_name.unwrap())?;

    // Only close the file if it was found
    if file_exists {
        close_specific_excel_window(file_name.unwrap())
    } else {
        println!(
            "Excel file '{}' not found, not closing any windows",
            file_name.unwrap()
        );
        Ok(false)
    }
}

/// Checks if an Excel window with the specified file name exists
fn check_excel_file_exists(file_name: &str) -> Result<bool> {
    // Structure to hold data for the callback
    struct EnumWindowsData {
        file_name: String,
        file_exists: bool,
    }

    // Create data for the callback
    let mut data = EnumWindowsData {
        file_name: file_name.to_string(),
        file_exists: false,
    };

    // Define the callback function for EnumWindows
    unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumWindowsData);

        // Check if this is an Excel window
        let class_name = HSTRING::from("XLMAIN");
        let excel_hwnd = FindWindowW(PCWSTR::from_raw(class_name.as_ptr()), PCWSTR::null());

        if excel_hwnd != HWND(0) {
            // Get the window title
            let mut title_buffer = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buffer);

            if title_len > 0 {
                let window_title = OsString::from_wide(&title_buffer[..title_len as usize]);
                let window_title = window_title.to_string_lossy().to_string();

                // Check if the window title contains the file name
                if window_title.contains(&data.file_name) {
                    data.file_exists = true;
                }
            }
        }

        // Continue enumeration
        BOOL(1)
    }

    // Enumerate all top-level windows
    unsafe {
        let lparam = LPARAM(&mut data as *mut _ as isize);
        EnumWindows(Some(enum_windows_callback), lparam)?;
    }

    Ok(data.file_exists)
}

/// Closes a specific Excel window that matches the given file name
fn close_specific_excel_window(file_name: &str) -> Result<bool> {
    // Structure to hold data for the callback
    struct EnumWindowsData {
        file_name: String,
        windows_closed: bool,
    }

    // Create data for the callback
    let mut data = EnumWindowsData {
        file_name: file_name.to_string(),
        windows_closed: false,
    };

    // Define the callback function for EnumWindows
    unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumWindowsData);

        // Check if this is an Excel window
        let class_name = HSTRING::from("XLMAIN");
        let excel_hwnd = FindWindowW(PCWSTR::from_raw(class_name.as_ptr()), PCWSTR::null());

        if excel_hwnd != HWND(0) {
            // Get the window title
            let mut title_buffer = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buffer);

            if title_len > 0 {
                let window_title = OsString::from_wide(&title_buffer[..title_len as usize]);
                let window_title = window_title.to_string_lossy().to_string();

                // Check if the window title contains the file name
                if window_title.contains(&data.file_name) {
                    println!("Closing Excel window: {window_title}");
                    SendMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                    data.windows_closed = true;
                }
            }
        }

        // Continue enumeration
        BOOL(1)
    }

    // Enumerate all top-level windows
    unsafe {
        let lparam = LPARAM(&mut data as *mut _ as isize);
        EnumWindows(Some(enum_windows_callback), lparam)?;
    }

    if data.windows_closed {
        println!("Excel window with file '{file_name}' closed successfully");
        Ok(true)
    } else {
        println!("No Excel window with file '{file_name}' found to close");
        Ok(false)
    }
}

/// Closes all Excel windows
fn close_all_excel_windows() -> Result<bool> {
    // Structure to hold data for the callback
    struct EnumWindowsData {
        windows_closed: bool,
    }

    // Create data for the callback
    let mut data = EnumWindowsData {
        windows_closed: false,
    };

    // Define the callback function for EnumWindows
    unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let data = &mut *(lparam.0 as *mut EnumWindowsData);

        // Check if this is an Excel window
        let class_name = HSTRING::from("XLMAIN");
        let excel_hwnd = FindWindowW(PCWSTR::from_raw(class_name.as_ptr()), PCWSTR::null());

        if excel_hwnd != HWND(0) {
            // Get the window title
            let mut title_buffer = [0u16; 512];
            let title_len = GetWindowTextW(hwnd, &mut title_buffer);

            if title_len > 0 {
                let window_title = OsString::from_wide(&title_buffer[..title_len as usize]);
                let window_title = window_title.to_string_lossy().to_string();

                println!("Closing Excel window: {window_title}");
                SendMessageW(hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                data.windows_closed = true;
            }
        }

        // Continue enumeration
        BOOL(1)
    }

    // Enumerate all top-level windows
    unsafe {
        let lparam = LPARAM(&mut data as *mut _ as isize);
        EnumWindows(Some(enum_windows_callback), lparam)?;
    }

    if data.windows_closed {
        println!("All Excel windows closed successfully");
        Ok(true)
    } else {
        println!("No Excel windows found to close");
        Ok(false)
    }
}

/// Saves a file from SAP GUI to the specified path and filename
///
/// This function handles the SAP GUI dialog for saving files to the local filesystem.
/// It supports both text and Excel file formats.
///
/// # Arguments
///
/// * `session` - Reference to the SAP GUI session
/// * `file_path` - Directory path where the file should be saved
/// * `file_name` - Name of the file to be saved
/// * `close_export_file` - Optional boolean to prevent Excel from opening after save (default: false)
///
/// # Returns
///
/// * `Result<bool>` - Ok(true) if the file was successfully saved, Ok(false) otherwise
pub fn save_sap_file(
    session: &GuiSession,
    file_path: &str,
    file_name: &str,
    close_export_file: Option<bool>,
) -> Result<bool> {
    let close_export = close_export_file.unwrap_or(false);
    println!("Exporting data from SAP....");
    if close_export {
        println!("Will close after export...");
    }

    // Check if window[1] exists (the save dialog)
    let err_wnd = exist_ctrl(session, 1, "", true)?;

    if err_wnd.cband {
        println!(
            "Found window title: ({}). Extracting to filename: ({}\\{})",
            err_wnd.ctext, file_path, file_name
        );

        // Check if it's an error message window
        let msg_err_wnd = exist_ctrl(session, 1, "/usr/txtMESSTXT1", true)?;
        if msg_err_wnd.cband {
            // There's an error message, get the text
            if let Ok(component) = session.find_by_id("wnd[1]/usr/txtMESSTXT1".to_string()) {
                if let Some(text_field) = component.downcast::<GuiTextField>() {
                    let error_msg = text_field.text()?;
                    println!("Error message: {error_msg}");
                    return Ok(false);
                }
            }
        }

        // Set the file path
        if let Ok(component) = session.find_by_id("wnd[1]/usr/ctxtDY_PATH".to_string()) {
            if let Some(text_field) = component.downcast::<GuiCTextField>() {
                text_field.set_text(file_path.to_string())?;
            }
        }

        // Set the file name
        if let Ok(component) = session.find_by_id("wnd[1]/usr/ctxtDY_FILENAME".to_string()) {
            if let Some(text_field) = component.downcast::<GuiCTextField>() {
                text_field.set_text(file_name.to_string())?;
            }
        }

        // Press the save button
        if let Ok(component) = session.find_by_id("wnd[1]/tbar[0]/btn[0]".to_string()) {
            if let Some(button) = component.downcast::<GuiButton>() {
                button.press()?;
            }
        }

        // Wait a moment for the save operation to complete
        thread::sleep(Duration::from_millis(3000));

        println!("File saved successfully");

        // If Excel is allowed to open and we want to close it after saving
        if close_export {
            // Wait a moment for Excel to open
            thread::sleep(Duration::from_millis(5000));

            // Close Excel windows with the specified file name
            match close_excel_windows(Some(file_name)) {
                Ok(true) => println!("Excel closed successfully"),
                Ok(false) => println!("No Excel windows found to close"),
                Err(e) => println!("Error closing Excel: {e:?}"),
            }
        }

        Ok(true)
    } else {
        println!("Error: Save dialog window not found");
        Ok(false)
    }
}

#[cfg(test)]
mod export_dir_tests {
    use super::resolve_export_dir;
    use std::path::Path;

    fn joined(base: &str, parts: &[&str]) -> String {
        let mut p = Path::new(base).to_path_buf();
        for x in parts {
            p.push(x);
        }
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn legacy_without_rname_dir() {
        let legacy = resolve_export_dir(r"C:\Reports", None, "VT11");
        assert!(legacy.ends_with("VT11") && legacy.starts_with(r"C:\Reports"));
        assert_eq!(
            resolve_export_dir(r"C:\Reports", Some("  "), "VT11"),
            legacy
        );
    }

    #[test]
    fn relative_goes_under_reports_dir() {
        let want = joined(r"C:\Reports", &["mustgo", "VT11"]);
        assert_eq!(
            resolve_export_dir(r"C:\Reports", Some("./mustgo"), "VT11"),
            want
        );
        assert_eq!(
            resolve_export_dir(r"C:\Reports", Some(r".\mustgo"), "VT11"),
            want
        );
        assert_eq!(
            resolve_export_dir(r"C:\Reports", Some("mustgo"), "VT11"),
            want
        );
        // doubled backslashes from config are normalised
        assert_eq!(
            resolve_export_dir(r"C:\\Reports", Some("mustgo"), "VT11"),
            want
        );
    }

    #[test]
    fn absolute_used_as_is() {
        let want = joined(r"D:\out\k2xx", &["VT11"]);
        assert_eq!(
            resolve_export_dir(r"C:\Reports", Some(r"D:\out\k2xx"), "VT11"),
            want
        );
    }
}
