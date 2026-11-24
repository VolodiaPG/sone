//! Keeps the computer awake (sleep lock) and the screen on (screen lock), each
//! held separately.
//!
//! Each OS module exports the same `IdleInhibitor` surface:
//! `new()`, `async set(seq, sleep, screen, &WebviewWindow)` and
//! `async uninhibit()`.

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

/// Drops calls that finish after a newer one (the frontend sends one per state
/// change, and they can land out of order).
#[derive(Default)]
struct LatestOnly(u64);

impl LatestOnly {
    fn accept(&mut self, seq: u64) -> bool {
        if seq <= self.0 {
            return false;
        }
        self.0 = seq;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::LatestOnly;

    #[test]
    fn only_newer_calls_are_applied() {
        let mut l = LatestOnly::default();
        assert!(l.accept(5));
        assert!(!l.accept(5));
        assert!(!l.accept(3));
        assert!(l.accept(6));
    }
}
