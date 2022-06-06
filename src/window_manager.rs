// TODO: Remove this
#![allow(dead_code)]

use anyhow::Context;
use log::{debug, error, warn};
use mlua::{Lua, RegistryKey};
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::FromStr;
use std::sync::Arc;
use xcb::{randr, x, Connection, Xid};

const MANAGED_HINT: &str = "(managed by NeoPULT) ";

pub type ManagedWid = usize;

#[derive(Debug)]
pub struct ManagedWindow {
    id: ManagedWid,
    variant: WindowVariant,
    min_geometry: MinGeometry,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
struct Geometry {
    x: i16,
    y: i16,
    width: u16,
    height: u16,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Alignment {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
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

impl AlignedGeometry {
    fn into_geometry(&self, wm: &WindowManager) -> Geometry {
        let (x, y) = match self.alignment {
            Alignment::TopLeft => (self.x_offset, self.y_offset),
            Alignment::TopRight => (wm.screen_width - self.width - self.x_offset, self.y_offset),
            Alignment::BottomRight => (
                wm.screen_width - self.width - self.x_offset,
                wm.screen_height - self.height - self.y_offset,
            ),
            Alignment::BottomLeft => (
                self.x_offset,
                wm.screen_height - self.height - self.y_offset,
            ),
        };
        Geometry {
            x: x as i16,
            y: y as i16,
            width: self.width,
            height: self.height,
        }
    }
}

#[derive(Debug, Clone)]
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
            MinGeometry::Fixed(aligned_geometry) => aligned_geometry.into_geometry(wm),
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

    pub fn manage_x_window(
        &mut self,
        lua: &Lua,
        window: x::Window,
        min_geometry: MinGeometry,
    ) -> xcb::Result<ManagedWid> {
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
            min_geometry,
        };
        let geometry = managed_window.min_geometry.get_geometry(self, lua);
        self.managed_windows.insert(id, managed_window);
        self.current_id += 1;

        self.change_window_geometry(id, geometry)?;

        Ok(id)
    }

    // The caller must ensure that `id` is the id of a managed window
    fn change_window_geometry(&self, id: ManagedWid, geometry: Geometry) -> xcb::Result<()> {
        let window = self.managed_windows.get(&id).unwrap();
        match window.variant {
            WindowVariant::XWindow { window } => {
                self.conn.send_request(&x::ConfigureWindow {
                    window,
                    value_list: &[
                        x::ConfigWindow::X(geometry.x as i32),
                        x::ConfigWindow::Y(geometry.y as i32),
                        x::ConfigWindow::Width(geometry.width as u32),
                        x::ConfigWindow::Height(geometry.height as u32),
                    ],
                });
            }
            WindowVariant::VirtualWindow => todo!(),
        }

        Ok(())
    }

    // This is copied from a proof of concept implementation
    // TODO: Adjust to current implementation
    fn set_resolution(
        conn: &xcb::Connection,
        screen: &x::Screen,
        (target_width, target_height): (u16, u16),
    ) -> xcb::Result<()> {
        let cookie = conn.send_request(&randr::GetScreenSizeRange {
            window: screen.root(),
        });
        let screen_size_range = conn.wait_for_reply(cookie)?;

        if target_width < screen_size_range.min_width()
            || target_width > screen_size_range.max_width()
            || target_height < screen_size_range.min_width()
            || target_height > screen_size_range.max_height()
        {
            // TODO: Change with returning some Error. This will require the introduction of a new
            // error type which can be used for xcb errors and this custom result.
            panic!(
                "Tried to set invalid resolution {}x{}. Minimum resolution is {}x{}. \
                Maximum resolution is {}x{}",
                target_width,
                target_height,
                screen_size_range.min_width(),
                screen_size_range.min_height(),
                screen_size_range.max_width(),
                screen_size_range.max_height()
            );
        }

        let cookie = conn.send_request(&randr::GetScreenResources {
            window: screen.root(),
        });
        let screen_resources = conn.wait_for_reply(cookie)?;

        let crtc = *screen_resources
            .crtcs()
            .first()
            .expect("no crtc in screen resources");
        let output = *screen_resources
            .outputs()
            .first()
            .expect("no output in screen resources");

        let cookie = conn.send_request(&randr::GetCrtcInfo {
            crtc,
            config_timestamp: x::CURRENT_TIME,
        });
        let crtc_info = conn.wait_for_reply(cookie)?;
        let current_width = crtc_info.width();
        let current_height = crtc_info.height();

        if current_width == target_width && current_height == target_height {
            return Ok(());
        }

        // If one dimension grows and the other shrinks, we do it in two steps:
        //  1. Grow the one dimension of the screen to the bigger target dimension
        //  2. Shrink the other dimension of the screen to the smaller target dimension
        if target_width < current_width && target_height > current_height {
            // randr_set_screen_size(conn, screen, (current_width, target_height))?;
        }

        if target_width > current_width && target_height < current_height {
            // randr_set_screen_size(conn, screen, (target_width, current_height))?;
        }

        // SetCrtcConfig only works if the target output size fits into the current screen size. We
        // can't use it directly when going from 200x300 to 300x200 because the size grows in width.
        if target_width < current_width || target_height < current_height {
            let target_mode_opt = screen_resources
                .modes()
                .iter()
                .find(|m| m.width == target_width && m.height == target_height);

            let mode = match target_mode_opt {
                Some(target_mode) => {
                    let target_mode_id = target_mode.id;
                    let cookie = conn.send_request(&randr::GetOutputInfo {
                        output,
                        config_timestamp: x::CURRENT_TIME,
                    });
                    let output_info = conn.wait_for_reply(cookie)?;

                    let mode = *output_info
                        .modes()
                        .iter()
                        .find(|m| m.resource_id() == target_mode_id)
                        .expect("no matching mode on output");
                    mode
                }
                None => {
                    let id = conn.generate_id::<randr::Mode>().resource_id();
                    let name_len =
                        target_width.to_string().len() + target_height.to_string().len() + 1;
                    let name = format!("{}x{}", target_width, target_height);
                    println!(
                        "new mode, id: {:?}, name_len: {}, name: {}",
                        id, name_len, name
                    );
                    // Values reverse engineered from GetScreenResources output and existing modes
                    let cookie = conn.send_request(&randr::CreateMode {
                        window: screen.root(),
                        mode_info: randr::ModeInfo {
                            id,
                            width: target_width,
                            height: target_height,
                            name_len: name_len as u16,
                            dot_clock: 60 * target_width as u32 * target_height as u32, // 60 fps
                            hsync_start: 0,
                            hsync_end: 0,
                            htotal: target_width,
                            hskew: 0,
                            vsync_start: 0,
                            vsync_end: 0,
                            vtotal: target_height,
                            mode_flags: randr::ModeFlag::empty(),
                        },
                        name: name.as_bytes(),
                    });
                    let create_mode_resp = conn.wait_for_reply(cookie)?;
                    let mode = create_mode_resp.mode();

                    conn.send_and_check_request(&randr::AddOutputMode { output, mode })?;

                    mode
                }
            };

            let cookie = conn.send_request(&randr::SetCrtcConfig {
                crtc,
                timestamp: x::CURRENT_TIME,
                config_timestamp: x::CURRENT_TIME,
                x: crtc_info.x(),
                y: crtc_info.y(),
                mode,
                rotation: crtc_info.rotation(),
                outputs: &[output],
            });
            let _ = conn.wait_for_reply(cookie)?;
        }

        // SetScreenSize only works if the output size (mode from SetCrtcConfig) fits into the target
        // screen size. To achieve that, we shrink the output first.
        //
        // Increasing the screen size automatically increases the output size too.
        // randr_set_screen_size(conn, screen, (target_width, target_height))?;

        Ok(())
    }
}

mod tests {
    #[allow(unused_imports)]
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
