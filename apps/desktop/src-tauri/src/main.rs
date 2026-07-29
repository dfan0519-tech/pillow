// Prevents an extra console window on Windows in release builds.
// Adapted from Grok App src-tauri/src/main.rs at
// cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    keeper_ide_desktop::run();
}
