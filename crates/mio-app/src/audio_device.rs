//! Output devices of the settings page (`audio_device/macos.rs` of the original and the
//! `audioOutput` store's fallbacks): the list with sample rate, channels and volume, switching
//! the system's default output, the device volume, a change notification and the test sound.
//!
//! On macOS the devices come from CoreAudio and a choice changes the system default output, as
//! in the original. Elsewhere the original had only the browser's device list (which did not
//! reach its Rust player); the list comes from the audio host and a choice is remembered and
//! used for the test sound.
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    /// CoreAudio's device ID on macOS; the list position elsewhere.
    pub id: u32,
    pub name: String,
    pub is_default: bool,
    pub sample_rate: f64,
    pub channels: u32,
    pub volume: f32,
    pub volume_supported: bool,
}

/// Set by the system's device-change callback; read and cleared by the page.
static CHANGED: AtomicBool = AtomicBool::new(false);

/// Whether the device list changed since the last call.
pub fn take_changed() -> bool {
    CHANGED.swap(false, Ordering::SeqCst)
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{CHANGED, Device};
    use core_foundation::base::TCFType;
    use core_foundation::string::{CFString, CFStringRef};
    use std::ffi::c_void;
    use std::sync::Once;
    use std::sync::atomic::Ordering;

    #[repr(C)]
    struct Address {
        selector: u32,
        scope: u32,
        element: u32,
    }
    #[repr(C)]
    struct Buffer {
        channels: u32,
        size: u32,
        data: *mut c_void,
    }
    #[repr(C)]
    struct BufferList {
        count: u32,
        buffers: [Buffer; 1],
    }

    #[link(name = "CoreAudio", kind = "framework")]
    unsafe extern "C" {
        fn AudioObjectGetPropertyDataSize(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
        ) -> i32;
        fn AudioObjectGetPropertyData(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: *mut u32,
            data: *mut c_void,
        ) -> i32;
        fn AudioObjectSetPropertyData(
            id: u32,
            address: *const Address,
            qualifier_size: u32,
            qualifier: *const c_void,
            size: u32,
            data: *const c_void,
        ) -> i32;
        fn AudioObjectAddPropertyListener(
            id: u32,
            address: *const Address,
            listener: extern "C" fn(u32, u32, *const Address, *mut c_void) -> i32,
            data: *mut c_void,
        ) -> i32;
    }

    const SYSTEM_OBJECT: u32 = 1;
    const SCOPE_GLOBAL: u32 = u32::from_be_bytes(*b"glob");
    const SCOPE_OUTPUT: u32 = u32::from_be_bytes(*b"outp");
    const DEVICES: u32 = u32::from_be_bytes(*b"dev#");
    const DEFAULT_OUTPUT: u32 = u32::from_be_bytes(*b"dOut");
    const NAME: u32 = u32::from_be_bytes(*b"lnam");
    const SAMPLE_RATE: u32 = u32::from_be_bytes(*b"nsrt");
    const STREAM_CONFIG: u32 = u32::from_be_bytes(*b"slay");
    const VOLUME: u32 = u32::from_be_bytes(*b"volm");

    fn address(selector: u32, scope: u32) -> Address {
        Address {
            selector,
            scope,
            element: 0,
        }
    }

    unsafe fn read<T: Default>(id: u32, selector: u32, scope: u32) -> Option<T> {
        let a = address(selector, scope);
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                id,
                &a,
                0,
                std::ptr::null(),
                &mut size,
                (&mut value as *mut T).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    unsafe fn ids() -> Result<Vec<u32>, String> {
        let a = address(DEVICES, SCOPE_GLOBAL);
        let mut size = 0u32;
        let status = unsafe {
            AudioObjectGetPropertyDataSize(SYSTEM_OBJECT, &a, 0, std::ptr::null(), &mut size)
        };
        if status != 0 {
            return Err(format!("AudioObjectGetPropertyDataSize failed: {status}"));
        }
        let mut list = vec![0u32; size as usize / 4];
        if list.is_empty() {
            return Ok(list);
        }
        let status = unsafe {
            AudioObjectGetPropertyData(
                SYSTEM_OBJECT,
                &a,
                0,
                std::ptr::null(),
                &mut size,
                list.as_mut_ptr().cast(),
            )
        };
        if status != 0 {
            return Err(format!("AudioObjectGetPropertyData failed: {status}"));
        }
        Ok(list)
    }

    /// Output channels of a device (the sum over its output buffers).
    unsafe fn output_channels(id: u32) -> u32 {
        let a = address(STREAM_CONFIG, SCOPE_OUTPUT);
        let mut size = 0u32;
        if unsafe { AudioObjectGetPropertyDataSize(id, &a, 0, std::ptr::null(), &mut size) } != 0
            || size < 4
        {
            return 0;
        }
        // 8-byte aligned storage for the variable-length list.
        let mut storage = vec![0u64; (size as usize).div_ceil(8)];
        if unsafe {
            AudioObjectGetPropertyData(
                id,
                &a,
                0,
                std::ptr::null(),
                &mut size,
                storage.as_mut_ptr().cast(),
            )
        } != 0
        {
            return 0;
        }
        let list = storage.as_ptr().cast::<BufferList>();
        let count = unsafe { (*list).count } as usize;
        let first = unsafe { std::ptr::addr_of!((*list).buffers).cast::<Buffer>() };
        let offset = first as usize - list as usize;
        let room = (size as usize).saturating_sub(offset) / std::mem::size_of::<Buffer>();
        (0..count.min(room))
            .map(|i| unsafe { (*first.add(i)).channels })
            .sum()
    }

    pub fn enumerate() -> Result<Vec<Device>, String> {
        unsafe {
            let default = read::<u32>(SYSTEM_OBJECT, DEFAULT_OUTPUT, SCOPE_GLOBAL).unwrap_or(0);
            let mut devices = Vec::new();
            for id in ids()? {
                let channels = output_channels(id);
                if channels == 0 {
                    continue;
                }
                let name = read::<CFStringRef>(id, NAME, SCOPE_GLOBAL)
                    .filter(|s| !s.is_null())
                    .map(|s| CFString::wrap_under_create_rule(s).to_string())
                    .unwrap_or_else(|| format!("Device {id}"));
                let volume = read::<f32>(id, VOLUME, SCOPE_OUTPUT);
                devices.push(Device {
                    id,
                    name,
                    is_default: id == default,
                    sample_rate: read::<f64>(id, SAMPLE_RATE, SCOPE_OUTPUT).unwrap_or(0.),
                    channels,
                    volume: volume.unwrap_or(0.),
                    volume_supported: volume.is_some(),
                });
            }
            Ok(devices)
        }
    }

    pub fn set_default(id: u32) -> Result<(), String> {
        let a = address(DEFAULT_OUTPUT, SCOPE_GLOBAL);
        let status = unsafe {
            AudioObjectSetPropertyData(
                SYSTEM_OBJECT,
                &a,
                0,
                std::ptr::null(),
                4,
                (&id as *const u32).cast(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(format!("Failed to set default output device: {status}"))
        }
    }

    pub fn set_volume(id: u32, volume: f32) -> Result<(), String> {
        let a = address(VOLUME, SCOPE_OUTPUT);
        let value = volume.clamp(0., 1.);
        let status = unsafe {
            AudioObjectSetPropertyData(
                id,
                &a,
                0,
                std::ptr::null(),
                4,
                (&value as *const f32).cast(),
            )
        };
        if status == 0 {
            Ok(())
        } else {
            Err(format!("Failed to set device volume: {status}"))
        }
    }

    extern "C" fn changed(_: u32, _: u32, _: *const Address, _: *mut c_void) -> i32 {
        CHANGED.store(true, Ordering::SeqCst);
        0
    }

    /// Starts watching the device list and the default output (once).
    pub fn listen() {
        static START: Once = Once::new();
        START.call_once(|| {
            for selector in [DEVICES, DEFAULT_OUTPUT] {
                let a = address(selector, SCOPE_GLOBAL);
                unsafe {
                    AudioObjectAddPropertyListener(
                        SYSTEM_OBJECT,
                        &a,
                        changed,
                        std::ptr::null_mut(),
                    );
                }
            }
        });
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::Device;
    use rodio::cpal::{self, traits::DeviceTrait, traits::HostTrait};

    pub fn enumerate() -> Result<Vec<Device>, String> {
        let host = cpal::default_host();
        let default = host.default_output_device().and_then(|d| d.name().ok());
        let devices = host.output_devices().map_err(|e| e.to_string())?;
        Ok(devices
            .enumerate()
            .filter_map(|(index, device)| {
                let name = device.name().ok()?;
                let config = device.default_output_config().ok();
                Some(Device {
                    id: index as u32,
                    is_default: default.as_deref() == Some(name.as_str()),
                    name,
                    sample_rate: config.as_ref().map_or(0., |c| f64::from(c.sample_rate().0)),
                    channels: config.as_ref().map_or(0, |c| u32::from(c.channels())),
                    volume: 0.,
                    volume_supported: false,
                })
            })
            .collect())
    }
    pub fn set_default(_: u32) -> Result<(), String> {
        // Remembered by the page; the system default is not touched.
        Ok(())
    }
    pub fn set_volume(_: u32, _: f32) -> Result<(), String> {
        Err("Not supported on this platform".into())
    }
    pub fn listen() {}
}

pub fn enumerate() -> Result<Vec<Device>, String> {
    imp::enumerate()
}
pub fn set_default(id: u32) -> Result<(), String> {
    imp::set_default(id)
}
pub fn set_volume(id: u32, volume: f32) -> Result<(), String> {
    imp::set_volume(id, volume)
}
/// Starts the change notification; safe to call repeatedly.
pub fn listen() {
    imp::listen()
}

/// The test sound: a sine gliding from 440 to 880 Hz in 0.1 s at gain 0.1 fading out over
/// 0.5 s (the original's WebAudio envelope), on the system's default output.
pub fn play_test_sound() -> Result<(), String> {
    use rodio::{OutputStreamBuilder, Sink, Source};
    struct Beep {
        sample: u64,
        phase: f64,
    }
    const RATE: u32 = 48_000;
    impl Iterator for Beep {
        type Item = f32;
        fn next(&mut self) -> Option<f32> {
            let t = self.sample as f64 / f64::from(RATE);
            if t >= 0.5 {
                return None;
            }
            // Exponential ramps: frequency 440 → 880 over 0.1 s, gain 0.1 → 0.001 over 0.5 s.
            let frequency = if t < 0.1 {
                440. * (880f64 / 440.).powf(t / 0.1)
            } else {
                880.
            };
            let gain = 0.1 * (0.001f64 / 0.1).powf(t / 0.5);
            self.phase += 2. * std::f64::consts::PI * frequency / f64::from(RATE);
            self.sample += 1;
            Some((self.phase.sin() * gain) as f32)
        }
    }
    impl Source for Beep {
        fn current_span_len(&self) -> Option<usize> {
            None
        }
        fn channels(&self) -> u16 {
            1
        }
        fn sample_rate(&self) -> u32 {
            RATE
        }
        fn total_duration(&self) -> Option<std::time::Duration> {
            Some(std::time::Duration::from_millis(500))
        }
    }
    std::thread::spawn(|| {
        if let Ok(mut stream) = OutputStreamBuilder::open_default_stream() {
            stream.log_on_drop(false);
            let sink = Sink::connect_new(stream.mixer());
            sink.append(Beep {
                sample: 0,
                phase: 0.,
            });
            sink.sleep_until_end();
        }
    });
    Ok(())
}
