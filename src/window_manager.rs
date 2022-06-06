// TODO: Remove this
#![allow(dead_code)]

use anyhow::Context;
use log::{debug, error, warn};
use mlua::{Lua, RegistryKey};
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use xcb::{randr, x, Connection};

const MANAGED_HINT: &str = "(managed by NeoPULT) ";

pub type ManagedWid = usize;

#[derive(Debug)]
pub struct ManagedWindow {
    id: ManagedWid,
    variant: WindowVariant,
    min_geometry: MinGeometry,
}

#[derive(Debug, PartialEq, Eq)]
struct Geometry {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Alignment {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, PartialEq, Eq)]
pub struct AlignedGeometry {
    x_offset: u16,
    y_offset: u16,
    width: u16,
    height: u16,
    alignment: Alignment,
}

impl FromStr for AlignedGeometry {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let x_offset_sign;
        let y_offset_sign;

        let (s, width) = match s.chars().position(|c| c == 'x') {
            Some(pos) => (
                &s[(pos + 1)..],
                u16::from_str(&s[..pos]).context("width is not numeric")?,
            ),
            None => anyhow::bail!("expected 'x' after width"),
        };

        let (s, height) = match s.chars().enumerate().find(|&(_, c)| c == '+' || c == '-') {
            Some((pos, sign)) => {
                x_offset_sign = sign;
                (
                    &s[(pos + 1)..],
                    u16::from_str(&s[..pos]).context("height is not numeric")?,
                )
            }
            None => anyhow::bail!("expected '+' or '-' after height"),
        };

        let (s, x_offset) = match s.chars().enumerate().find(|&(_, c)| c == '+' || c == '-') {
            Some((pos, sign)) => {
                y_offset_sign = sign;
                (
                    &s[(pos + 1)..],
                    u16::from_str(&s[..pos]).context("x offset is not numeric")?,
                )
            }
            None => anyhow::bail!("expected '+' or '-' after x offset"),
        };

        let y_offset = u16::from_str(s).context("y offset is not numeric")?;

        let alignment = match (x_offset_sign, y_offset_sign) {
            ('+', '+') => Alignment::TopLeft,
            ('-', '+') => Alignment::TopRight,
            ('-', '-') => Alignment::BottomRight,
            ('+', '-') => Alignment::BottomLeft,
            _ => unreachable!(),
        };

        Ok(AlignedGeometry {
            x_offset,
            y_offset,
            width,
            height,
            alignment,
        })
    }
}

#[derive(Debug)]
pub enum MinGeometry {
    Fixed(AlignedGeometry),
    Dynamic { callback_key: Arc<RegistryKey> },
}

impl Default for MinGeometry {
    fn default() -> Self {
        MinGeometry::Fixed(AlignedGeometry {
            x_offset: 0,
            y_offset: 0,
            width: 480,
            height: 360,
            alignment: Alignment::BottomRight,
        })
    }
}

impl FromStr for MinGeometry {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let aligned_geometry = AlignedGeometry::from_str(s)?;
        Ok(MinGeometry::Fixed(aligned_geometry))
    }
}

impl MinGeometry {
    fn get_geometry(&self, wm: &WindowManager, _lua: &Lua) -> Geometry {
        match self {
            MinGeometry::Fixed(AlignedGeometry {
                x_offset,
                y_offset,
                width,
                height,
                alignment,
            }) => {
                let (x, y) = match alignment {
                    Alignment::TopLeft => (*x_offset, *y_offset),
                    Alignment::TopRight => (wm.screen_width - *width - *x_offset, *y_offset),
                    Alignment::BottomRight => (
                        wm.screen_width - *width - *x_offset,
                        wm.screen_height - *height - *y_offset,
                    ),
                    Alignment::BottomLeft => (*x_offset, wm.screen_height - *height - *y_offset),
                };
                Geometry {
                    x: x as i16,
                    y: y as i16,
                    width: *width,
                    height: *height,
                }
            }
            // TODO: Call lua function to get geometry
            MinGeometry::Dynamic { callback_key: _ } => Geometry {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
        }
    }
}

#[derive(Debug)]
enum WindowVariant {
    XWindow { window: x::Window },
    VirtualWindow,
}

// NOTE: Remember to adjust Debug implementation when changing something here
pub struct WindowManager {
    conn: Connection,
    screen: x::ScreenBuf,
    screen_height: u16,
    screen_width: u16,
    current_id: ManagedWid,
    managed_windows: HashMap<ManagedWid, ManagedWindow>,
}

// xcb::Connection doesn't implement Debug, so we have to implement Debug ourselves
impl Debug for WindowManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowManager")
            .field("screen", &self.screen)
            .field("current_id", &self.screen)
            .field("managed_windows", &self.managed_windows)
            .finish()
    }
}

impl WindowManager {
    pub fn init() -> anyhow::Result<Self> {
        match std::env::var("DISPLAY") {
            Ok(display) => debug!("DISPLAY environment variable is {}", display),
            Err(std::env::VarError::NotPresent) => debug!("DISPLAY environment varibale isn't set"),
            Err(std::env::VarError::NotUnicode(_)) => {
                warn!("DISPLAY environment varibale isn't valid UTF-8")
            }
        }

        let (conn, screen_num) = xcb::Connection::connect(None).context(
            "couldn't connect to the x server, setting the DISPLAY \
            environment variable may solve the problem",
        )?;

        let setup = conn.get_setup();
        let screen = setup.roots().nth(screen_num as usize).unwrap().to_owned();

        let screen_res_cookie = conn.send_request(&randr::GetScreenResources {
            window: screen.root(),
        });
        let screen_res_reply = conn
            .wait_for_reply(screen_res_cookie)
            .context("error while waiting for GetScreenResources reply")?;
        let output = screen_res_reply.outputs()[0];
        let crtc = screen_res_reply.crtcs()[0];

        let crtc_info_cookie = conn.send_request(&randr::GetCrtcInfo {
            crtc,
            config_timestamp: x::CURRENT_TIME,
        });

        let output_info_cookie = conn.send_request(&randr::GetOutputInfo {
            output,
            config_timestamp: x::CURRENT_TIME,
        });

        let crtc_info_reply = conn
            .wait_for_reply(crtc_info_cookie)
            .context("error while waiting for GetCrtcInfo reply")?;
        let screen_height = crtc_info_reply.height();
        let screen_width = crtc_info_reply.width();

        let output_info_reply = conn
            .wait_for_reply(output_info_cookie)
            .context("error while waiting for GetOutputInfo reply")?;
        let output_name = String::from_utf8_lossy(output_info_reply.name());

        if !output_name.starts_with("VNC") {
            anyhow::bail!(
                "the x server isn't a vnc server, setting the DISPLAY \
                environment variable may solve the problem"
            );
        }

        Ok(WindowManager {
            conn,
            screen: screen.to_owned(),
            screen_height,
            screen_width,
            current_id: 0,
            managed_windows: HashMap::new(),
        })
    }

    pub fn get_window_by_name(&self, to_claim: &str) -> anyhow::Result<Option<x::Window>> {
        let cookie = self.conn.send_request(&x::QueryTree {
            window: self.screen.root(),
        });
        let reply = self
            .conn
            .wait_for_reply(cookie)
            .context("error while waiting for QueryTree reply")?;

        let children: &[x::Window] = reply.children();

        let mut children_name_cookies = Vec::with_capacity(children.len());

        for child in children.iter() {
            let cookie = self.conn.send_request(&x::GetProperty {
                delete: false,
                window: *child,
                property: x::ATOM_WM_NAME,
                r#type: x::ATOM_STRING,
                long_offset: 0,
                // Amount of chars of name to retrieve
                long_length: 128,
            });
            children_name_cookies.push(cookie);
        }

        for (cookie, &window) in children_name_cookies.into_iter().zip(children) {
            match self.conn.wait_for_reply(cookie) {
                Err(e) => error!("error while waiting for WM_NAME reply: {}", e),
                Ok(reply) => {
                    let name = String::from_utf8_lossy(reply.value());
                    if name.contains(to_claim) && !name.starts_with(MANAGED_HINT) {
                        return Ok(Some(window));
                    }
                }
            }
        }

        Ok(None)
    }

    pub fn manage_x_window(&mut self, window: x::Window) -> xcb::Result<ManagedWid> {
        self.conn.send_and_check_request(&x::ChangeProperty {
            mode: x::PropMode::Prepend,
            window,
            property: x::ATOM_WM_NAME,
            r#type: x::ATOM_STRING,
            data: MANAGED_HINT.as_bytes(),
        })?;
        let id = self.current_id;
        let managed_window = ManagedWindow {
            id,
            variant: WindowVariant::XWindow { window },
            min_geometry: MinGeometry::default(),
        };
        self.managed_windows.insert(id, managed_window);
        self.current_id += 1;

        // TODO: Set geometry of window

        Ok(id)
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_aligned_geometry_from_str() {
        let s = "400x300+200-100";
        let min_position = AlignedGeometry::from_str(s).unwrap();
        assert_eq!(
            min_position,
            AlignedGeometry {
                width: 400,
                height: 300,
                x_offset: 200,
                y_offset: 100,
                alignment: Alignment::BottomLeft
            }
        );

        let s = "480x360-0-0";
        let min_position = AlignedGeometry::from_str(s).unwrap();
        assert_eq!(
            min_position,
            AlignedGeometry {
                width: 480,
                height: 360,
                x_offset: 0,
                y_offset: 0,
                alignment: Alignment::BottomRight
            }
        );

        let s = "";
        assert!(AlignedGeometry::from_str(s).is_err());

        let s = "-100x-100-0-0";
        assert!(AlignedGeometry::from_str(s).is_err());

        let s = "480x360";
        assert!(AlignedGeometry::from_str(s).is_err());

        let s = "100x100-0-0 ";
        assert!(AlignedGeometry::from_str(s).is_err());
    }
}
