//! Status bar information provider (battery percentage and current time)
//!
//! This module provides functionality to display battery percentage and current time
//! in the terminal's tab bar.

use std::time::{Duration, Instant};

/// Cached status information to avoid expensive system calls on every frame
pub struct StatusInfo {
    battery_percentage: Option<u8>,
    time_string: String,
    last_update: Instant,
    update_interval: Duration,
}

impl Default for StatusInfo {
    fn default() -> Self {
        Self::new()
    }
}

impl StatusInfo {
    pub fn new() -> Self {
        let mut status = Self {
            battery_percentage: None,
            time_string: String::new(),
            last_update: Instant::now() - Duration::from_secs(60), // Force initial update
            update_interval: Duration::from_secs(30), // Update every 30 seconds
        };
        status.update();
        status
    }

    /// Update status info if enough time has passed
    pub fn update_if_needed(&mut self) {
        if self.last_update.elapsed() >= self.update_interval {
            self.update();
        }
    }

    /// Force update status info
    fn update(&mut self) {
        self.battery_percentage = get_battery_percentage();
        self.time_string = get_current_time();
        self.last_update = Instant::now();
    }

    /// Get the formatted status string for display
    pub fn get_display_string(&mut self) -> String {
        self.update_if_needed();

        match self.battery_percentage {
            Some(pct) => format!("{}%  {}", pct, self.time_string),
            None => self.time_string.clone(),
        }
    }
}

/// Get the current time formatted as HH:MM
fn get_current_time() -> String {
    use std::time::SystemTime;

    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();

    // Calculate local time (this is a simplified approach)
    // For proper timezone handling, chrono would be ideal, but we'll use a shell command fallback
    #[cfg(target_os = "macos")]
    {
        if let Ok(output) = std::process::Command::new("date")
            .arg("+%H:%M")
            .output()
        {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(output) = std::process::Command::new("date")
            .arg("+%H:%M")
            .output()
        {
            if output.status.success() {
                return String::from_utf8_lossy(&output.stdout).trim().to_string();
            }
        }
    }

    // Fallback: UTC time from duration
    let secs = duration.as_secs();
    let hours = (secs / 3600) % 24;
    let minutes = (secs / 60) % 60;
    format!("{:02}:{:02}", hours, minutes)
}

/// Get battery percentage on macOS using pmset
#[cfg(target_os = "macos")]
fn get_battery_percentage() -> Option<u8> {
    let output = std::process::Command::new("pmset")
        .arg("-g")
        .arg("batt")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Parse output like: "Now drawing from 'Battery Power'\n -InternalBattery-0 (id=...) 85%; ..."
    for line in stdout.lines() {
        if line.contains("InternalBattery") || line.contains('%') {
            // Find the percentage value
            for part in line.split_whitespace() {
                if part.ends_with("%;") || part.ends_with('%') {
                    let num_str = part.trim_end_matches("%;").trim_end_matches('%');
                    if let Ok(pct) = num_str.parse::<u8>() {
                        return Some(pct);
                    }
                }
            }
        }
    }

    None
}

/// Get battery percentage on non-macOS platforms
#[cfg(not(target_os = "macos"))]
fn get_battery_percentage() -> Option<u8> {
    // Try reading from /sys/class/power_supply/BAT0/capacity (Linux)
    #[cfg(target_os = "linux")]
    {
        for bat in ["BAT0", "BAT1", "BAT2"] {
            let path = format!("/sys/class/power_supply/{}/capacity", bat);
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(pct) = content.trim().parse::<u8>() {
                    return Some(pct);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_info_creation() {
        let status = StatusInfo::new();
        assert!(!status.time_string.is_empty());
    }

    #[test]
    fn test_get_current_time_format() {
        let time = get_current_time();
        // Should be in HH:MM format
        assert_eq!(time.len(), 5);
        assert!(time.contains(':'));
    }
}
