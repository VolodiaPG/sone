//! macOS exclusive output through the Core Audio HAL.
//!
//! Bit-perfect playback on macOS needs three things from the output device,
//! and `osxaudiosink` does none of them on its own:
//!
//! 1. Hog mode, so no other process mixes into the device.
//! 2. A physical stream format at the track's own sample rate, so the HAL
//!    does not resample.
//! 3. An integer physical format at least as deep as the track, so no bits
//!    are lost between the HAL's float32 stage and the DAC. float32 holds
//!    every 16-bit and 24-bit sample exactly.
//!
//! [`ExclusiveDevice`] takes hog mode and changes the format. On drop it
//! puts back the format the device had before and releases hog mode, so the
//! user's Audio MIDI Setup state survives a session.

use objc2_core_audio::{
    kAudioDevicePropertyDeviceUID, kAudioDevicePropertyHogMode,
    kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertyStreams,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioHardwarePropertyDevices,
    kAudioObjectPropertyElementMain, kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal,
    kAudioObjectPropertyScopeOutput, kAudioObjectSystemObject,
    kAudioStreamPropertyAvailablePhysicalFormats, kAudioStreamPropertyPhysicalFormat,
    AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize, AudioObjectID,
    AudioObjectPropertyAddress, AudioObjectPropertyScope, AudioObjectPropertySelector,
    AudioObjectSetPropertyData, AudioStreamRangedDescription,
};
use objc2_core_audio_types::{
    kAudioFormatFlagIsFloat, kAudioFormatFlagIsSignedInteger, kAudioFormatLinearPCM,
    AudioStreamBasicDescription,
};
use objc2_core_foundation::CFString;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::{Duration, Instant};

/// How long the HAL gets to report a format change before playback fails.
/// USB DACs usually switch in well under a second.
const FORMAT_SWITCH_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExclusiveError {
    /// No output device has this UID (unplugged, or renamed by the OS).
    NotFound(String),
    /// Another process holds hog mode on the device.
    Busy { owner_pid: i32 },
    /// The device has no physical format at this rate that keeps every bit.
    UnsupportedFormat { sample_rate: u32, bits: u32 },
    CoreAudio(String),
}

impl std::fmt::Display for ExclusiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(uid) => write!(f, "output device not found: {uid}"),
            Self::Busy { owner_pid } => {
                write!(f, "output device is held in exclusive mode by process {owner_pid}")
            }
            Self::UnsupportedFormat { sample_rate, bits } => write!(
                f,
                "output device has no bit-perfect format for {bits}-bit/{sample_rate} Hz"
            ),
            Self::CoreAudio(msg) => write!(f, "Core Audio: {msg}"),
        }
    }
}

/// One output device as the device picker shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputDevice {
    /// Persistent across reboots and replugs, unlike the `AudioObjectID`.
    pub uid: String,
    pub name: String,
}

/// A linear PCM physical format the device offers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicalFormat {
    pub min_rate: f64,
    pub max_rate: f64,
    pub bits: u32,
    pub float: bool,
}

impl PhysicalFormat {
    /// GStreamer-style name for the signal path, e.g. `S24LE` or `F32LE`.
    pub fn label(&self) -> String {
        if self.float {
            format!("F{}LE", self.bits)
        } else {
            format!("S{}LE", self.bits)
        }
    }

    fn covers(&self, rate: u32) -> bool {
        let rate = f64::from(rate);
        self.min_rate <= rate && rate <= self.max_rate
    }

    /// Whether this format carries `bits`-bit integer samples unchanged.
    fn keeps_bits(&self, bits: u32) -> bool {
        if self.float {
            // The float32 mantissa holds 24 bits exactly.
            self.bits >= 32 && bits <= 24
        } else {
            self.bits >= bits
        }
    }
}

/// Pick the physical format for a track, or `None` when no format plays it
/// bit-perfect.
///
/// The rate must match exactly: anything else means the HAL resamples. Among
/// the formats that keep every bit, integer beats float (the DAC gets the
/// samples without a conversion it might dither), then the deepest wins,
/// which is the mode DACs default to.
pub fn choose_format(available: &[PhysicalFormat], rate: u32, bits: u32) -> Option<PhysicalFormat> {
    available
        .iter()
        .filter(|f| f.covers(rate) && f.keeps_bits(bits))
        .max_by_key(|f| (!f.float, f.bits))
        .copied()
}

fn address(
    selector: AudioObjectPropertySelector,
    scope: AudioObjectPropertyScope,
) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress {
        mSelector: selector,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    }
}

fn check(status: i32, what: &str) -> Result<(), ExclusiveError> {
    if status == 0 {
        Ok(())
    } else {
        let code = status.to_be_bytes();
        let fourcc = if code.iter().all(|b| b.is_ascii_graphic()) {
            format!(" ('{}')", String::from_utf8_lossy(&code))
        } else {
            String::new()
        };
        Err(ExclusiveError::CoreAudio(format!("{what} failed: {status}{fourcc}")))
    }
}

/// Read a fixed-size property.
fn get<T: Copy>(
    object: AudioObjectID,
    addr: AudioObjectPropertyAddress,
    what: &str,
) -> Result<T, ExclusiveError> {
    let mut value = std::mem::MaybeUninit::<T>::uninit();
    let mut size = std::mem::size_of::<T>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(value.as_mut_ptr().cast::<c_void>()).unwrap(),
        )
    };
    check(status, what)?;
    if size as usize != std::mem::size_of::<T>() {
        return Err(ExclusiveError::CoreAudio(format!("{what}: unexpected size {size}")));
    }
    Ok(unsafe { value.assume_init() })
}

/// Read a property that is an array of `T`.
fn get_array<T: Copy>(
    object: AudioObjectID,
    addr: AudioObjectPropertyAddress,
    what: &str,
) -> Result<Vec<T>, ExclusiveError> {
    let mut size = 0u32;
    let status = unsafe {
        AudioObjectGetPropertyDataSize(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
        )
    };
    check(status, what)?;
    let count = size as usize / std::mem::size_of::<T>();
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut items: Vec<T> = Vec::with_capacity(count);
    let status = unsafe {
        AudioObjectGetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::new(items.as_mut_ptr().cast::<c_void>()).unwrap(),
        )
    };
    check(status, what)?;
    unsafe { items.set_len(size as usize / std::mem::size_of::<T>()) };
    Ok(items)
}

fn set<T>(
    object: AudioObjectID,
    addr: AudioObjectPropertyAddress,
    value: &T,
    what: &str,
) -> Result<(), ExclusiveError> {
    let status = unsafe {
        AudioObjectSetPropertyData(
            object,
            NonNull::from(&addr),
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            NonNull::from(value).cast::<c_void>(),
        )
    };
    check(status, what)
}

/// Read a CFString property. The HAL returns a +1 reference.
fn get_string(
    object: AudioObjectID,
    addr: AudioObjectPropertyAddress,
    what: &str,
) -> Result<String, ExclusiveError> {
    let raw: *const CFString = get(object, addr, what)?;
    let Some(raw) = NonNull::new(raw as *mut CFString) else {
        return Err(ExclusiveError::CoreAudio(format!("{what}: null string")));
    };
    let string = unsafe { objc2_core_foundation::CFRetained::from_raw(raw) };
    Ok(string.to_string())
}

const SYSTEM: AudioObjectID = kAudioObjectSystemObject as AudioObjectID;

fn output_streams(device: AudioObjectID) -> Result<Vec<AudioObjectID>, ExclusiveError> {
    get_array(
        device,
        address(kAudioDevicePropertyStreams, kAudioObjectPropertyScopeOutput),
        "read output streams",
    )
}

fn device_uid(device: AudioObjectID) -> Result<String, ExclusiveError> {
    get_string(
        device,
        address(kAudioDevicePropertyDeviceUID, kAudioObjectPropertyScopeGlobal),
        "read device UID",
    )
}

/// Every device that has at least one output stream.
pub fn output_devices() -> Result<Vec<OutputDevice>, ExclusiveError> {
    let ids: Vec<AudioObjectID> = get_array(
        SYSTEM,
        address(kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal),
        "list devices",
    )?;
    let mut devices = Vec::new();
    for id in ids {
        if output_streams(id).map(|s| s.is_empty()).unwrap_or(true) {
            continue;
        }
        let (Ok(uid), Ok(name)) = (
            device_uid(id),
            get_string(
                id,
                address(kAudioObjectPropertyName, kAudioObjectPropertyScopeGlobal),
                "read device name",
            ),
        ) else {
            continue;
        };
        devices.push(OutputDevice { uid, name });
    }
    Ok(devices)
}

/// UID of the current system output device.
pub fn default_output_uid() -> Result<String, ExclusiveError> {
    let id: AudioObjectID = get(
        SYSTEM,
        address(kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyScopeGlobal),
        "read default output device",
    )?;
    device_uid(id)
}

fn resolve(uid: &str) -> Result<AudioObjectID, ExclusiveError> {
    let ids: Vec<AudioObjectID> = get_array(
        SYSTEM,
        address(kAudioHardwarePropertyDevices, kAudioObjectPropertyScopeGlobal),
        "list devices",
    )?;
    ids.into_iter()
        .find(|&id| device_uid(id).is_ok_and(|u| u == uid))
        .ok_or_else(|| ExclusiveError::NotFound(uid.to_string()))
}

fn to_physical(desc: &AudioStreamRangedDescription) -> Option<PhysicalFormat> {
    let f = &desc.mFormat;
    if f.mFormatID != kAudioFormatLinearPCM {
        return None;
    }
    let float = f.mFormatFlags & kAudioFormatFlagIsFloat != 0;
    if !float && f.mFormatFlags & kAudioFormatFlagIsSignedInteger == 0 {
        return None;
    }
    Some(PhysicalFormat {
        min_rate: desc.mSampleRateRange.mMinimum,
        max_rate: desc.mSampleRateRange.mMaximum,
        bits: f.mBitsPerChannel,
        float,
    })
}

/// An output device held in hog mode. Dropping it restores the device.
pub struct ExclusiveDevice {
    device: AudioObjectID,
    stream: AudioObjectID,
    original_format: AudioStreamBasicDescription,
    format_changed: bool,
}

// The HAL is thread-safe; the IDs are plain integers.
unsafe impl Send for ExclusiveDevice {}

fn hog_owner(device: AudioObjectID) -> Result<i32, ExclusiveError> {
    get(
        device,
        address(kAudioDevicePropertyHogMode, kAudioObjectPropertyScopeGlobal),
        "read hog mode",
    )
}

impl ExclusiveDevice {
    /// Take hog mode on the device with this UID.
    pub fn acquire(uid: &str) -> Result<Self, ExclusiveError> {
        let device = resolve(uid)?;
        let stream = *output_streams(device)?
            .first()
            .ok_or_else(|| ExclusiveError::CoreAudio("device has no output stream".into()))?;
        let original_format: AudioStreamBasicDescription = get(
            stream,
            address(kAudioStreamPropertyPhysicalFormat, kAudioObjectPropertyScopeGlobal),
            "read physical format",
        )?;

        let me = std::process::id() as i32;
        let owner = hog_owner(device)?;
        if owner != -1 && owner != me {
            return Err(ExclusiveError::Busy { owner_pid: owner });
        }
        if owner != me {
            set(
                device,
                address(kAudioDevicePropertyHogMode, kAudioObjectPropertyScopeGlobal),
                &me,
                "take hog mode",
            )?;
            let owner = hog_owner(device)?;
            if owner != me {
                return Err(ExclusiveError::Busy { owner_pid: owner });
            }
        }
        log::info!("[coreaudio]: hog mode taken on {uid} (device {device})");
        Ok(Self {
            device,
            stream,
            original_format,
            format_changed: false,
        })
    }

    /// The `AudioObjectID` for `osxaudiosink`'s `device` property.
    pub fn device_id(&self) -> u32 {
        self.device
    }

    /// Each usable physical format with the HAL description that selects it.
    fn available_formats(
        &self,
    ) -> Result<Vec<(PhysicalFormat, AudioStreamBasicDescription)>, ExclusiveError> {
        let ranged: Vec<AudioStreamRangedDescription> = get_array(
            self.stream,
            address(kAudioStreamPropertyAvailablePhysicalFormats, kAudioObjectPropertyScopeGlobal),
            "read available physical formats",
        )?;
        Ok(ranged
            .iter()
            .filter_map(|d| to_physical(d).map(|f| (f, d.mFormat)))
            .collect())
    }

    /// Switch the device to a physical format that plays `bits`-bit samples
    /// at `sample_rate` unchanged, and wait until the HAL reports it.
    pub fn apply_format(
        &mut self,
        sample_rate: u32,
        bits: u32,
    ) -> Result<PhysicalFormat, ExclusiveError> {
        let available = self.available_formats()?;
        let formats: Vec<PhysicalFormat> = available.iter().map(|(f, _)| *f).collect();
        let chosen = choose_format(&formats, sample_rate, bits)
            .ok_or(ExclusiveError::UnsupportedFormat { sample_rate, bits })?;
        let (_, mut target) = *available
            .iter()
            .find(|(f, _)| *f == chosen)
            .expect("choose_format returns one of its inputs");
        // A ranged description carries rate 0; pin the track's rate.
        target.mSampleRate = f64::from(sample_rate);

        let addr = address(kAudioStreamPropertyPhysicalFormat, kAudioObjectPropertyScopeGlobal);
        let current: AudioStreamBasicDescription = get(self.stream, addr, "read physical format")?;
        if current != target {
            set(self.stream, addr, &target, "set physical format")?;
            self.format_changed = true;
            self.wait_for_rate(f64::from(sample_rate))?;
        }
        log::info!(
            "[coreaudio]: physical format {}-bit {} at {} Hz",
            chosen.bits,
            if chosen.float { "float" } else { "integer" },
            sample_rate
        );
        Ok(chosen)
    }

    /// The device's current sample rate.
    pub fn nominal_rate(&self) -> Result<f64, ExclusiveError> {
        get(
            self.device,
            address(kAudioDevicePropertyNominalSampleRate, kAudioObjectPropertyScopeGlobal),
            "read nominal sample rate",
        )
    }

    /// The physical format change is asynchronous; poll the nominal rate.
    fn wait_for_rate(&self, rate: f64) -> Result<(), ExclusiveError> {
        let deadline = Instant::now() + FORMAT_SWITCH_TIMEOUT;
        loop {
            let now = self.nominal_rate()?;
            if (now - rate).abs() < 0.5 {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(ExclusiveError::CoreAudio(format!(
                    "device stayed at {now} Hz instead of {rate} Hz"
                )));
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for ExclusiveDevice {
    fn drop(&mut self) {
        if self.format_changed {
            // Wait for the switch: once hog mode is gone, the HAL may drop a
            // format change that is still in flight.
            let addr = address(kAudioStreamPropertyPhysicalFormat, kAudioObjectPropertyScopeGlobal);
            let restored = set(self.stream, addr, &self.original_format, "restore physical format")
                .and_then(|()| self.wait_for_rate(self.original_format.mSampleRate));
            if let Err(e) = restored {
                log::warn!("[coreaudio]: {e}");
            }
        }
        let release: i32 = -1;
        if let Err(e) = set(
            self.device,
            address(kAudioDevicePropertyHogMode, kAudioObjectPropertyScopeGlobal),
            &release,
            "release hog mode",
        ) {
            log::warn!("[coreaudio]: {e}");
        } else {
            log::info!("[coreaudio]: hog mode released (device {})", self.device);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u32, min: f64, max: f64) -> PhysicalFormat {
        PhysicalFormat { min_rate: min, max_rate: max, bits, float: false }
    }

    fn float32(min: f64, max: f64) -> PhysicalFormat {
        PhysicalFormat { min_rate: min, max_rate: max, bits: 32, float: true }
    }

    #[test]
    fn the_rate_must_match_exactly() {
        let formats = [int(24, 48_000.0, 48_000.0), int(24, 96_000.0, 96_000.0)];
        assert_eq!(choose_format(&formats, 44_100, 16), None);
        assert_eq!(choose_format(&formats, 96_000, 24), Some(formats[1]));
    }

    #[test]
    fn a_ranged_format_covers_rates_inside_its_range() {
        let formats = [int(24, 44_100.0, 192_000.0)];
        assert_eq!(choose_format(&formats, 88_200, 24), Some(formats[0]));
        assert_eq!(choose_format(&formats, 384_000, 24), None);
    }

    #[test]
    fn a_shallower_integer_format_never_plays_a_deeper_track() {
        let formats = [int(16, 44_100.0, 44_100.0)];
        assert_eq!(choose_format(&formats, 44_100, 24), None);
        assert_eq!(choose_format(&formats, 44_100, 16), Some(formats[0]));
    }

    #[test]
    fn integer_beats_float_and_deeper_beats_shallower() {
        let formats = [
            float32(44_100.0, 44_100.0),
            int(16, 44_100.0, 44_100.0),
            int(24, 44_100.0, 44_100.0),
        ];
        assert_eq!(choose_format(&formats, 44_100, 16), Some(formats[2]));
    }

    /// Touches the real default output device: run by hand with
    /// `cargo test --lib coreaudio -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn hardware_round_trip_on_the_default_output() {
        let devices = output_devices().unwrap();
        println!("output devices: {devices:?}");
        let uid = default_output_uid().unwrap();
        let before = {
            let dev = ExclusiveDevice::acquire(&uid).unwrap();
            let before = dev.nominal_rate().unwrap();
            println!("formats: {:?}", dev.available_formats().unwrap().iter().map(|(f, _)| f).collect::<Vec<_>>());
            before
        };
        {
            let mut dev = ExclusiveDevice::acquire(&uid).unwrap();
            for (rate, bits) in [(44_100, 16), (48_000, 24), (96_000, 24)] {
                match dev.apply_format(rate, bits) {
                    Ok(f) => {
                        println!("{bits}/{rate}: {f:?}");
                        assert_eq!(dev.nominal_rate().unwrap(), f64::from(rate));
                    }
                    Err(e) => println!("{bits}/{rate}: {e}"),
                }
            }
        }
        let dev = ExclusiveDevice::acquire(&uid).unwrap();
        assert_eq!(dev.nominal_rate().unwrap(), before, "drop restores the rate");
    }

    #[test]
    fn float32_carries_up_to_24_bits() {
        let formats = [float32(44_100.0, 44_100.0)];
        assert_eq!(choose_format(&formats, 44_100, 24), Some(formats[0]));
        assert_eq!(choose_format(&formats, 44_100, 32), None);
    }
}
