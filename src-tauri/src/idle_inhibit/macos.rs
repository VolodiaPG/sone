//! macOS sleep and screen locks, each an IOKit power assertion.
//!
//! Sleep: `PreventUserIdleSystemSleep`. coreaudiod already holds one while an
//! audio output device is active, but an explicit assertion also covers
//! video and does not depend on the output device.
//! Screen: `PreventUserIdleDisplaySleep`, which keeps the display on.

use objc2_core_foundation::CFString;

use super::LatestOnly;

/// kIOPMAssertionLevelOn
const ASSERTION_LEVEL_ON: u32 = 255;

#[link(name = "IOKit", kind = "framework")]
extern "C" {
    fn IOPMAssertionCreateWithName(
        assertion_type: *const CFString,
        assertion_level: u32,
        assertion_name: *const CFString,
        assertion_id: *mut u32,
    ) -> i32;
    fn IOPMAssertionRelease(assertion_id: u32) -> i32;
}

/// One held IOKit power assertion, released on drop.
struct Assertion(u32);

impl Assertion {
    fn create(kind: &'static str, name: &'static str) -> Option<Self> {
        let assertion_type = CFString::from_static_str(kind);
        let name = CFString::from_static_str(name);
        let mut id: u32 = 0;
        let status = unsafe {
            IOPMAssertionCreateWithName(&*assertion_type, ASSERTION_LEVEL_ON, &*name, &mut id)
        };
        if status == 0 {
            log::debug!("[idle_inhibit]: {kind} assertion created (id={id})");
            Some(Self(id))
        } else {
            log::warn!("[idle_inhibit]: IOPMAssertionCreateWithName({kind}) failed: {status:#x}");
            None
        }
    }
}

impl Drop for Assertion {
    fn drop(&mut self) {
        unsafe { IOPMAssertionRelease(self.0) };
        log::debug!("[idle_inhibit]: assertion released (id={})", self.0);
    }
}

/// Creates the assertion when `on` and none is held, drops it when `!on`.
fn hold(slot: &mut Option<Assertion>, on: bool, kind: &'static str, name: &'static str) {
    if !on {
        *slot = None;
    } else if slot.is_none() {
        *slot = Assertion::create(kind, name);
    }
}

#[derive(Default)]
pub struct IdleInhibitor {
    latest: LatestOnly,
    sleep: Option<Assertion>,
    screen: Option<Assertion>,
}

impl IdleInhibitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn set(
        &mut self,
        seq: u64,
        sleep: bool,
        screen: bool,
        _window: &tauri::WebviewWindow,
    ) {
        if !self.latest.accept(seq) {
            return;
        }
        hold(
            &mut self.sleep,
            sleep,
            "PreventUserIdleSystemSleep",
            "SONE is playing",
        );
        hold(
            &mut self.screen,
            screen,
            "PreventUserIdleDisplaySleep",
            "SONE player is on screen",
        );
    }

    /// Releases both (logout, exit).
    pub async fn uninhibit(&mut self) {
        self.sleep = None;
        self.screen = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_and_releases_display_assertion() {
        let assertion_type = CFString::from_static_str("PreventUserIdleDisplaySleep");
        let name = CFString::from_static_str("sone idle_inhibit unit test");
        let mut id: u32 = 0;
        let status = unsafe {
            IOPMAssertionCreateWithName(&*assertion_type, ASSERTION_LEVEL_ON, &*name, &mut id)
        };
        assert_eq!(status, 0, "assertion creation failed: {status:#x}");
        assert_ne!(id, 0);
        assert_eq!(unsafe { IOPMAssertionRelease(id) }, 0);
    }
}
