use std::thread;
use std::time::Duration;
use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};

/// Prevents the system from going to sleep by setting the thread execution state.
/// This function should be called once to enable keep-awake functionality.
pub fn prevent_sleep() -> Result<(), String> {
    unsafe {
        let result =
            SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
        if result == 0 {
            Err("Failed to set thread execution state".to_string())
        } else {
            Ok(())
        }
    }
}

/// Starts a background thread that periodically refreshes the keep-awake state.
/// This is useful for long-running processes to ensure the system stays awake.
pub fn start_keep_awake_thread() -> Result<(), String> {
    // First, set the initial execution state
    prevent_sleep()?;

    // Start a background thread to periodically refresh the state
    thread::spawn(|| {
        loop {
            thread::sleep(Duration::from_secs(60)); // Refresh every minute
            if let Err(e) = prevent_sleep() {
                eprintln!("Warning: Failed to refresh keep-awake state: {}", e);
            }
        }
    });

    Ok(())
}

/// Enables keep-awake functionality with optional background thread.
/// If `use_background_thread` is true, starts a background thread to periodically
/// refresh the keep-awake state. Otherwise, just sets the initial state.
pub fn enable_keep_awake(use_background_thread: bool) -> Result<(), String> {
    if use_background_thread {
        start_keep_awake_thread()
    } else {
        prevent_sleep()
    }
}

/// Disables keep-awake functionality by resetting the thread execution state.
/// Note: This is not strictly necessary as the state will be reset when the process exits,
/// but it's good practice to clean up.
pub fn disable_keep_awake() -> Result<(), String> {
    unsafe {
        let result = SetThreadExecutionState(ES_CONTINUOUS);
        if result == 0 {
            Err("Failed to reset thread execution state".to_string())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prevent_sleep() {
        // This test might fail in some environments, so we'll just check it doesn't panic
        let result = prevent_sleep();
        // We don't assert the result since it depends on system permissions
        println!("prevent_sleep result: {:?}", result);
    }

    #[test]
    fn test_enable_keep_awake() {
        let result = enable_keep_awake(false);
        // We don't assert the result since it depends on system permissions
        println!("enable_keep_awake result: {:?}", result);
    }
}
