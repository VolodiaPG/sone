//! System media controls: now-playing metadata and remote commands.
//!
//! Linux runs an MPRIS server. macOS uses MPNowPlayingInfoCenter and
//! MPRemoteCommandCenter. Both backends export `MediaControls` with
//! `new(AppHandle)` and `send(MediaCommand)`, and both emit the same
//! `tray:`/`mpris:` events to the frontend.

#[cfg(target_os = "linux")]
mod mpris;
#[cfg(target_os = "linux")]
pub use mpris::MediaControls;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::MediaControls;

#[allow(clippy::large_enum_variant)]
pub enum MediaCommand {
    SetMetadata {
        track_id: u64,
        title: String,
        artist: String,
        album: String,
        art_url: String,
        duration_secs: f64,
        url: Option<String>,
        album_artist: Option<String>,
        track_number: Option<u32>,
        disc_number: Option<u32>,
        content_created: Option<String>,
        user_rating: Option<f64>,
    },
    SetPlaybackStatus {
        is_playing: bool,
    },
    SetVolume {
        volume: f64,
    },
    Seeked {
        position_secs: f64,
    },
    SetShuffle {
        enabled: bool,
    },
    SetLoopStatus {
        mode: u8,
    },
    SetFullscreen {
        fullscreen: bool,
    },
    Stop,
}
