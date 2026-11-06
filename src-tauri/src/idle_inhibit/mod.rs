//! Keeps the screen awake while the maximized player is visible.
//!
//! Each OS module exports the same `IdleInhibitor` surface:
//! `new()`, `async inhibit(&WebviewWindow)` and `async uninhibit()`.

#[cfg(target_os = "linux")]
mod dbus;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
mod wayland;
#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
pub use linux::IdleInhibitor;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::IdleInhibitor;
