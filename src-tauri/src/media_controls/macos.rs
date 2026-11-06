//! macOS counterpart of the Linux MPRIS server (`mpris.rs`): registers
//! MPRemoteCommandCenter handlers so media keys, Control Center, and AirPods
//! controls drive playback, and publishes MPNowPlayingInfoCenter metadata.
//! Command handlers emit the same `tray:`/`mpris:` events the frontend
//! already listens to.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::NSImage;
use objc2_core_foundation::CGSize;
use objc2_foundation::{NSData, NSMutableDictionary, NSNumber, NSString};
use objc2_media_player::{
    MPChangePlaybackPositionCommandEvent, MPMediaItemArtwork, MPMediaItemPropertyAlbumArtist,
    MPMediaItemPropertyAlbumTitle, MPMediaItemPropertyAlbumTrackNumber, MPMediaItemPropertyArtist,
    MPMediaItemPropertyArtwork, MPMediaItemPropertyDiscNumber, MPMediaItemPropertyPlaybackDuration,
    MPMediaItemPropertyTitle, MPNowPlayingInfoCenter, MPNowPlayingInfoPropertyElapsedPlaybackTime,
    MPNowPlayingInfoPropertyPlaybackRate, MPNowPlayingPlaybackState, MPRemoteCommand,
    MPRemoteCommandEvent, MPRemoteCommandHandlerStatus,
};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

use super::MediaCommand;

/// Retained MPMediaItemArtwork shared through `NowPlaying`. Created and read
/// on the main thread only; the Send/Sync impls are sound because the other
/// threads merely move or drop the handle, and Objective-C release is
/// thread-safe.
struct Artwork(Retained<MPMediaItemArtwork>);
unsafe impl Send for Artwork {}
unsafe impl Sync for Artwork {}

#[derive(Default, Clone)]
struct NowPlaying {
    title: String,
    artist: String,
    album: String,
    album_artist: Option<String>,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    duration_secs: f64,
    position_secs: f64,
    is_playing: bool,
    /// False until the first track's metadata arrives and after Stop; while
    /// false the system Now Playing entry is cleared.
    active: bool,
    /// URL the current `artwork` was (or is being) fetched from.
    art_url: String,
    artwork: Option<Arc<Artwork>>,
}

pub struct MediaControls {
    app_handle: tauri::AppHandle,
    now_playing: Arc<Mutex<NowPlaying>>,
}

impl MediaControls {
    pub fn new(app_handle: tauri::AppHandle) -> Self {
        let app = app_handle.clone();
        let scheduled = app_handle.run_on_main_thread(move || {
            if MainThreadMarker::new().is_none() {
                log::warn!(
                    "media remote setup did not run on the main thread; media keys disabled"
                );
                return;
            }
            register_remote_commands(app);
        });
        if let Err(e) = scheduled {
            log::warn!("failed to schedule media remote setup: {e}");
        }

        Self {
            app_handle,
            now_playing: Arc::new(Mutex::new(NowPlaying::default())),
        }
    }

    pub fn send(&self, cmd: MediaCommand) {
        // Positions provided by the caller (seek, new track) are authoritative;
        // for bare play/pause transitions the anchor is re-read from the audio
        // player on the main thread instead.
        let mut refresh_position = false;
        let mut fetch_art_url: Option<String> = None;
        {
            let mut np = match self.now_playing.lock() {
                Ok(np) => np,
                Err(_) => return,
            };
            match cmd {
                MediaCommand::SetMetadata {
                    title,
                    artist,
                    album,
                    art_url,
                    duration_secs,
                    album_artist,
                    track_number,
                    disc_number,
                    ..
                } => {
                    np.title = title;
                    np.artist = artist;
                    np.album = album;
                    np.album_artist = album_artist;
                    np.track_number = track_number;
                    np.disc_number = disc_number;
                    np.duration_secs = duration_secs;
                    np.position_secs = 0.0;
                    np.active = true;
                    // Same album art as the previous track: keep the cached
                    // artwork instead of refetching.
                    if np.art_url != art_url || np.artwork.is_none() {
                        np.artwork = None;
                        np.art_url = art_url.clone();
                        if !art_url.is_empty() {
                            fetch_art_url = Some(art_url);
                        }
                    }
                }
                MediaCommand::SetPlaybackStatus { is_playing } => {
                    np.is_playing = is_playing;
                    refresh_position = true;
                }
                MediaCommand::Seeked { position_secs } => {
                    np.position_secs = position_secs;
                }
                MediaCommand::Stop => {
                    *np = NowPlaying::default();
                }
                // Not represented in the macOS Now Playing surface.
                MediaCommand::SetVolume { .. }
                | MediaCommand::SetShuffle { .. }
                | MediaCommand::SetLoopStatus { .. }
                | MediaCommand::SetFullscreen { .. } => return,
            }
        }

        let app = self.app_handle.clone();
        let now_playing = Arc::clone(&self.now_playing);
        let result = self.app_handle.run_on_main_thread(move || {
            publish(&app, &now_playing, refresh_position);
        });
        if let Err(e) = result {
            log::warn!("failed to schedule now-playing update: {e}");
        }

        if let Some(url) = fetch_art_url {
            fetch_artwork(self.app_handle.clone(), Arc::clone(&self.now_playing), url);
        }
    }
}

/// Downloads the cover image and, if the track hasn't changed in the
/// meantime, attaches it to the Now Playing entry.
fn fetch_artwork(app: tauri::AppHandle, now_playing: Arc<Mutex<NowPlaying>>, url: String) {
    tauri::async_runtime::spawn(async move {
        // Same egress rules as every other request: a blocked proxy plan
        // means no artwork, never a direct fetch.
        let client = match app.state::<crate::AppState>().proxied_http.client() {
            Ok(client) => client,
            Err(e) => {
                log::debug!("[media_remote]: artwork fetch blocked by proxy plan: {e}");
                return;
            }
        };
        let response = client
            .get(&url)
            .send()
            .await
            .and_then(|r| r.error_for_status());
        let bytes = match response {
            Ok(response) => match response.bytes().await {
                Ok(bytes) => bytes,
                Err(e) => {
                    log::debug!("[media_remote]: artwork body read failed: {e}");
                    return;
                }
            },
            Err(e) => {
                log::debug!("[media_remote]: artwork fetch failed: {e}");
                return;
            }
        };

        let app_for_publish = app.clone();
        let result = app.run_on_main_thread(move || {
            let Some(artwork) = make_artwork(&bytes) else {
                log::debug!("[media_remote]: artwork bytes did not decode to an image");
                return;
            };
            {
                let mut np = match now_playing.lock() {
                    Ok(np) => np,
                    Err(_) => return,
                };
                if np.art_url != url || !np.active {
                    return;
                }
                np.artwork = Some(Arc::new(Artwork(artwork)));
            }
            publish(&app_for_publish, &now_playing, false);
        });
        if let Err(e) = result {
            log::debug!("[media_remote]: failed to schedule artwork update: {e}");
        }
    });
}

/// Wraps decoded image bytes in an MPMediaItemArtwork. Main thread only.
fn make_artwork(bytes: &[u8]) -> Option<Retained<MPMediaItemArtwork>> {
    let data = NSData::with_bytes(bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    let bounds: CGSize = image.size();
    if bounds.width <= 0.0 || bounds.height <= 0.0 {
        return None;
    }
    let handler = RcBlock::new(move |_: CGSize| -> NonNull<NSImage> { NonNull::from(&*image) });
    Some(unsafe {
        MPMediaItemArtwork::initWithBoundsSize_requestHandler(
            MPMediaItemArtwork::alloc(),
            bounds,
            &handler,
        )
    })
}

/// Builds the MPNowPlayingInfoCenter dictionary from the current snapshot and
/// pushes it, along with the playback state. Main thread only.
fn publish(app: &tauri::AppHandle, now_playing: &Arc<Mutex<NowPlaying>>, refresh_position: bool) {
    let snapshot = {
        let mut np = match now_playing.lock() {
            Ok(np) => np,
            Err(_) => return,
        };
        if refresh_position && np.active {
            if let Some(state) = app.try_state::<crate::AppState>() {
                if let Ok(secs) = state.audio_player.get_position() {
                    np.position_secs = secs as f64;
                }
            }
        }
        np.clone()
    };

    unsafe {
        let center = MPNowPlayingInfoCenter::defaultCenter();
        if !snapshot.active {
            center.setNowPlayingInfo(None);
            center.setPlaybackState(MPNowPlayingPlaybackState::Stopped);
            return;
        }

        let info = NSMutableDictionary::<NSString, AnyObject>::new();
        let set_str = |key: &NSString, value: &str| {
            info.insert(key, NSString::from_str(value).as_ref());
        };
        set_str(MPMediaItemPropertyTitle, &snapshot.title);
        set_str(MPMediaItemPropertyArtist, &snapshot.artist);
        set_str(MPMediaItemPropertyAlbumTitle, &snapshot.album);
        if let Some(ref album_artist) = snapshot.album_artist {
            set_str(MPMediaItemPropertyAlbumArtist, album_artist);
        }
        if let Some(n) = snapshot.track_number {
            info.insert(
                MPMediaItemPropertyAlbumTrackNumber,
                NSNumber::numberWithUnsignedInt(n).as_ref(),
            );
        }
        if let Some(n) = snapshot.disc_number {
            info.insert(
                MPMediaItemPropertyDiscNumber,
                NSNumber::numberWithUnsignedInt(n).as_ref(),
            );
        }
        info.insert(
            MPMediaItemPropertyPlaybackDuration,
            NSNumber::numberWithDouble(snapshot.duration_secs).as_ref(),
        );
        info.insert(
            MPNowPlayingInfoPropertyElapsedPlaybackTime,
            NSNumber::numberWithDouble(snapshot.position_secs).as_ref(),
        );
        info.insert(
            MPNowPlayingInfoPropertyPlaybackRate,
            NSNumber::numberWithDouble(if snapshot.is_playing { 1.0 } else { 0.0 }).as_ref(),
        );
        if let Some(ref artwork) = snapshot.artwork {
            info.insert(MPMediaItemPropertyArtwork, artwork.0.as_ref());
        }

        center.setNowPlayingInfo(Some(&info));
        center.setPlaybackState(if snapshot.is_playing {
            MPNowPlayingPlaybackState::Playing
        } else {
            MPNowPlayingPlaybackState::Paused
        });
    }
}

/// Registers handlers with the shared MPRemoteCommandCenter. Each handler
/// emits the same event the Linux MPRIS server emits for that action.
/// Main thread only.
fn register_remote_commands(app: tauri::AppHandle) {
    use objc2_media_player::MPRemoteCommandCenter;

    let center = unsafe { MPRemoteCommandCenter::sharedCommandCenter() };

    let simple = |command: Retained<MPRemoteCommand>, event: &'static str| {
        let app = app.clone();
        let handler = RcBlock::new(move |_: NonNull<MPRemoteCommandEvent>| {
            app.emit(event, ()).ok();
            MPRemoteCommandHandlerStatus::Success
        });
        // The target token must stay registered for the app's lifetime.
        std::mem::forget(unsafe { command.addTargetWithHandler(&handler) });
        std::mem::forget(handler);
    };

    unsafe {
        simple(center.playCommand(), "mpris:play");
        simple(center.pauseCommand(), "mpris:pause");
        simple(center.togglePlayPauseCommand(), "tray:toggle-play");
        simple(center.stopCommand(), "mpris:stop");
        simple(center.nextTrackCommand(), "tray:next-track");
        simple(center.previousTrackCommand(), "tray:prev-track");

        let position_command = center.changePlaybackPositionCommand();
        let handler = RcBlock::new(move |event: NonNull<MPRemoteCommandEvent>| {
            let Some(event) = event
                .as_ref()
                .downcast_ref::<MPChangePlaybackPositionCommandEvent>()
            else {
                return MPRemoteCommandHandlerStatus::CommandFailed;
            };
            app.emit("mpris:set-position", event.positionTime()).ok();
            MPRemoteCommandHandlerStatus::Success
        });
        std::mem::forget(position_command.addTargetWithHandler(&handler));
        std::mem::forget(handler);
    }

    log::info!("macOS media remote command handlers registered");
}
