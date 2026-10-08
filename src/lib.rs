//! Local-first GitHub event filtering. No telemetry, shell commands or remote UI assets.
pub mod api;
pub mod engine;
pub mod filter;
pub mod model;
pub mod notify;
pub mod secrets;
pub mod storage;
#[cfg(feature = "desktop")]
pub mod ui;

#[cfg(all(feature = "desktop", target_os = "linux"))]
pub mod linux_desktop;
#[cfg(feature = "desktop")]
pub mod tray;
