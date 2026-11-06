//! macOS idle inhibition.
//!
//! System sleep needs no handling: coreaudiod holds a
//! `PreventUserIdleSystemSleep` power assertion whenever an audio output
//! device is active, so the Mac never suspends mid-playback. What macOS does
//! NOT prevent is display sleep — and the maximized player view wants the
//! screen to stay on, mirroring the Linux screensaver inhibitors. That is
//! covered here with an IOKit `PreventUserIdleDisplaySleep` assertion.

use objc2_core_foundation::CFString;

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

#[derive(Default)]
pub struct IdleInhibitor {
    assertion_id: Option<u32>,
}

impl IdleInhibitor {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn inhibit(&mut self, _window: &tauri::WebviewWindow) {
        if self.assertion_id.is_some() {
            return;
        }
        let assertion_type = CFString::from_static_str("PreventUserIdleDisplaySleep");
        let name = CFString::from_static_str("SONE maximized player is visible");
        let mut id: u32 = 0;
        let status = unsafe {
            IOPMAssertionCreateWithName(&*assertion_type, ASSERTION_LEVEL_ON, &*name, &mut id)
        };
        if status == 0 {
            self.assertion_id = Some(id);
            log::debug!("[idle_inhibit]: display-sleep assertion created (id={id})");
        } else {
            log::warn!("[idle_inhibit]: IOPMAssertionCreateWithName failed: {status:#x}");
        }
    }

    pub async fn uninhibit(&mut self) {
        if let Some(id) = self.assertion_id.take() {
            unsafe { IOPMAssertionRelease(id) };
            log::debug!("[idle_inhibit]: display-sleep assertion released (id={id})");
        }
    }
}

impl Drop for IdleInhibitor {
    fn drop(&mut self) {
        if let Some(id) = self.assertion_id.take() {
            unsafe { IOPMAssertionRelease(id) };
        }
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
