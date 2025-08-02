use std::str::FromStr;
use std::{env, str};

use super::*;
use crate::dpi::validate_scale_factor;
use crate::platform_impl::platform::x11::{monitor, VideoModeHandle};

use tracing::warn;
use x11rb::protocol::randr::{self, ConnectionExt as _};

/// Represents values of `WINIT_HIDPI_FACTOR`.
pub enum EnvVarDPI {
    Randr,
    Scale(f64),
    NotSet,
}

pub fn calc_dpi_factor(
    (width_px, height_px): (u32, u32),
    (width_mm, height_mm): (u64, u64),
) -> f64 {
    // See http://xpra.org/trac/ticket/728 for more information.
    if width_mm == 0 || height_mm == 0 {
        warn!(
            "XRandR reported that the display's 0mm in size, which is certifiably insane"
        );
        return 1.0;
    }

    let ppmm = ((width_px as f64 * height_px as f64)
        / (width_mm as f64 * height_mm as f64))
        .sqrt();
    // Quantize 1/12 step size
    let dpi_factor = ((ppmm * (12.0 * 25.4 / 96.0)).round() / 12.0).max(1.0);
    assert!(validate_scale_factor(dpi_factor));
    if dpi_factor <= 20. {
        dpi_factor
    } else {
        1.
    }
}

impl XConnection {
    // Retrieve DPI from Xft.dpi property
    pub fn get_xft_dpi(&self) -> Option<f64> {
        // Try to get it from XSETTINGS first.
        if let Some(xsettings_screen) = self.xsettings_screen() {
            match self.xsettings_dpi(xsettings_screen) {
                Ok(Some(dpi)) => return Some(dpi),
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!("failed to fetch XSettings: {err}");
                }
            }
        }

        self.database()
            .get_string("Xft.dpi", "")
            .and_then(|s| f64::from_str(s).ok())
    }

    pub fn get_output_info(
        &self,
        resources: &monitor::ScreenResources,
        crtc: &randr::GetCrtcInfoReply,
    ) -> Option<(String, f64, Vec<VideoModeHandle>)> {
        let output_info = match self
            .xcb_connection()
            .randr_get_output_info(crtc.outputs[0], x11rb::CURRENT_TIME)
            .map_err(X11Error::from)
            .and_then(|r| r.reply().map_err(X11Error::from))
        {
            Ok(output_info) => output_info,
            Err(err) => {
                warn!("Failed to get output info: {:?}", err);
                return None;
            }
        };

        let bit_depth = self.default_root().root_depth;
        let output_modes = &output_info.modes;
        let resource_modes = resources.modes();

        let modes = resource_modes
            .iter()
            // XRROutputInfo contains an array of mode ids that correspond to
            // modes in the array in XRRScreenResources
            .filter(|x| output_modes.contains(&x.id))
            .map(|mode| {
                VideoModeHandle {
                    size: (mode.width.into(), mode.height.into()),
                    refresh_rate_millihertz: monitor::mode_refresh_rate_millihertz(mode)
                        .unwrap_or(0),
                    bit_depth: bit_depth as u16,
                    native_mode: mode.id,
                    // This is populated in `MonitorHandle::video_modes` as the
                    // video mode is returned to the user
                    monitor: None,
                }
            })
            .collect();

        let name = match str::from_utf8(&output_info.name) {
            Ok(name) => name.to_owned(),
            Err(err) => {
                warn!("Failed to get output name: {:?}", err);
                return None;
            }
        };
        // Override DPI if `WINIT_X11_SCALE_FACTOR` variable is set
        let deprecated_dpi_override = env::var("WINIT_HIDPI_FACTOR").ok();
        if deprecated_dpi_override.is_some() {
            warn!(
                "The WINIT_HIDPI_FACTOR environment variable is deprecated; use \
                 WINIT_X11_SCALE_FACTOR"
            )
        }
        let dpi_env = env::var("WINIT_X11_SCALE_FACTOR").ok().map_or_else(
            || EnvVarDPI::NotSet,
            |var| {
                if var.to_lowercase() == "randr" {
                    EnvVarDPI::Randr
                } else if let Ok(dpi) = f64::from_str(&var) {
                    EnvVarDPI::Scale(dpi)
                } else if var.is_empty() {
                    EnvVarDPI::NotSet
                } else {
                    panic!(
                        "`WINIT_X11_SCALE_FACTOR` invalid; DPI factors must be either normal \
                         floats greater than 0, or `randr`. Got `{var}`"
                    );
                }
            },
        );

        let scale_factor = match dpi_env {
            EnvVarDPI::Randr => calc_dpi_factor(
                (crtc.width.into(), crtc.height.into()),
                (output_info.mm_width as _, output_info.mm_height as _),
            ),
            EnvVarDPI::Scale(dpi_override) => {
                if !validate_scale_factor(dpi_override) {
                    panic!(
                        "`WINIT_X11_SCALE_FACTOR` invalid; DPI factors must be either normal \
                         floats greater than 0, or `randr`. Got `{dpi_override}`",
                    );
                }
                dpi_override
            }
            EnvVarDPI::NotSet => {
                if let Some(dpi) = self.get_xft_dpi() {
                    dpi / 96.
                } else {
                    calc_dpi_factor(
                        (crtc.width.into(), crtc.height.into()),
                        (output_info.mm_width as _, output_info.mm_height as _),
                    )
                }
            }
        };

        Some((name, scale_factor, modes))
    }

    pub fn set_crtc_config(
        &self,
        crtc_id: randr::Crtc,
        mode_id: randr::Mode,
    ) -> Result<(), X11Error> {
        let crtc = self
            .xcb_connection()
            .randr_get_crtc_info(crtc_id, x11rb::CURRENT_TIME)?
            .reply()?;

        self.xcb_connection()
            .randr_set_crtc_config(
                crtc_id,
                crtc.timestamp,
                x11rb::CURRENT_TIME,
                crtc.x,
                crtc.y,
                mode_id,
                crtc.rotation,
                &crtc.outputs,
            )?
            .reply()
            .map(|_| ())
            .map_err(Into::into)
    }

    pub fn get_crtc_mode(&self, crtc_id: randr::Crtc) -> Result<randr::Mode, X11Error> {
        Ok(self
            .xcb_connection()
            .randr_get_crtc_info(crtc_id, x11rb::CURRENT_TIME)?
            .reply()?
            .mode)
    }
}
