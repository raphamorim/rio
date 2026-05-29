//! Terminal bell side effects: the audible beep and the desktop environment's
//! event sound theme.

#[cfg(all(unix, not(target_os = "macos")))]
mod canberra;

/// Play the legacy/native beep: the self-synthesized tone on Linux/BSD (behind
/// the `audio` feature), or the OS system beep on macOS/Windows.
pub fn beep() {
    #[cfg(target_os = "macos")]
    {
        // Use the system bell sound on macOS.
        unsafe {
            #[link(name = "AppKit", kind = "framework")]
            extern "C" {
                fn NSBeep();
            }
            NSBeep();
        }
    }

    #[cfg(target_os = "windows")]
    {
        // MB_OK (0x00000000) plays the default system beep.
        unsafe {
            windows_sys::Win32::System::Diagnostics::Debug::MessageBeep(0x00000000);
        }
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        #[cfg(feature = "audio")]
        {
            std::thread::spawn(|| {
                if let Err(e) = tone::play() {
                    tracing::warn!("Failed to play bell sound: {}", e);
                }
            });
        }
        #[cfg(not(feature = "audio"))]
        {
            tracing::debug!(
                "Audio bell requested but the `audio` feature is not enabled"
            );
        }
    }
}

/// Play the desktop environment's configured event sound. On Linux/BSD this is
/// the freedesktop sound theme (via libcanberra), which respects the user's
/// theme, output routing, volume, mute and Do-Not-Disturb. On macOS/Windows the
/// native system beep already is the system sound.
pub fn system_sound() {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        beep();
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        canberra::play("bell");
    }
}

#[cfg(all(
    feature = "audio",
    not(target_os = "macos"),
    not(target_os = "windows")
))]
mod tone {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    use std::error::Error;

    pub fn play() -> Result<(), Box<dyn Error>> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("No output device available")?;

        let config = device.default_output_config()?;

        match config.sample_format() {
            cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
            cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
            cpal::SampleFormat::U16 => run::<u16>(&device, &config.into()),
            _ => Err("Unsupported sample format".into()),
        }
    }

    fn run<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
    ) -> Result<(), Box<dyn Error>>
    where
        T: cpal::Sample + cpal::SizedSample + cpal::FromSample<f32>,
    {
        let sample_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;
        let duration_secs = crate::constants::BELL_DURATION.as_secs_f32();
        let total_samples = (sample_rate * duration_secs) as usize;

        let mut sample_clock = 0f32;
        let mut samples_played = 0usize;

        let stream = device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                for frame in data.chunks_mut(channels) {
                    if samples_played >= total_samples {
                        for sample in frame.iter_mut() {
                            *sample = T::from_sample(0.0);
                        }
                    } else {
                        let value = (sample_clock * 440.0 * 2.0 * std::f32::consts::PI
                            / sample_rate)
                            .sin()
                            * 0.2;
                        for sample in frame.iter_mut() {
                            *sample = T::from_sample(value);
                        }
                        sample_clock += 1.0;
                        samples_played += 1;
                    }
                }
            },
            |err| tracing::error!("Audio stream error: {}", err),
            None,
        )?;

        stream.play()?;
        std::thread::sleep(crate::constants::BELL_DURATION);

        Ok(())
    }
}
