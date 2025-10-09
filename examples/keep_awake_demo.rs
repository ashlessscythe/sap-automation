use std::thread;
use std::time::Duration;
use sap_automation::utils::keep_awake;

fn main() {
    println!("Keep-Awake Demo");
    println!("===============");
    println!("This demo shows how the keep-awake functionality works.");
    println!("The system will stay awake for 30 seconds, then exit.");
    println!("Press Ctrl+C to exit early.\n");

    // Enable keep-awake with background thread
    match keep_awake::enable_keep_awake(true) {
        Ok(_) => println!("✓ Keep-awake enabled - system will stay awake"),
        Err(e) => {
            eprintln!("✗ Failed to enable keep-awake: {}", e);
            return;
        }
    }

    // Run for 30 seconds
    for i in 1..=30 {
        println!("Keep-awake active... {} seconds remaining", 30 - i);
        thread::sleep(Duration::from_secs(1));
    }

    println!("\n✓ Demo completed. Keep-awake will be disabled when the program exits.");
}
