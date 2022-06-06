use anyhow::Context;
use log::{debug, error, warn};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;
use xcb::{randr, x, Connection};

const MANAGED_HINT: &str = "(managed by NeoPULT) ";

pub type ManagedWid = usize;

#[derive(Debug)]
pub struct ManagedWindow {
    id: ManagedWid,
    geometry: Geometry,
    variant: WindowVariant,
}

#[derive(Debug)]
enum WindowVariant {
    XWindow { window: x::Window },
    VirtualWindow,
}

#[derive(Debug)]
pub struct Geometry {
    x: i16,
    y: i16,
    w: u16,
    h: u16,
}

impl Geometry {
    fn new(x: i16, y: i16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }
}

// NOTE: Remember to adjust Debug implementation when changing something here
pub struct WindowManager {
    conn: Connection,
    screen: x::ScreenBuf,
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

        // Check that the X server is a VNC server
        let cookie = conn.send_request(&randr::GetScreenResources {
            window: screen.root(),
        });
        let screen_res_resp = conn
            .wait_for_reply(cookie)
            .context("error while waiting for GetScreenResources reply")?;
        let output = screen_res_resp.outputs()[0];

        let cookie = conn.send_request(&randr::GetOutputInfo {
            output,
            config_timestamp: x::CURRENT_TIME,
        });
        let output_info_resp = conn
            .wait_for_reply(cookie)
            .context("error while waiting for GetOutputInfo reply")?;
        let output_name = String::from_utf8_lossy(output_info_resp.name());

        if !output_name.starts_with("VNC") {
            anyhow::bail!(
                "the x server isn't a vnc server, setting the DISPLAY \
                environment variable may solve the problem"
            );
        }

        Ok(WindowManager {
            conn,
            screen: screen.to_owned(),
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
        // TODO: Somehow mark the x window as managed, so it can't be claimed again
        // TODO: Query window geometry and set it accordingly
        let geometry_cookie = self.conn.send_request(&x::GetGeometry {
            drawable: x::Drawable::Window(window),
        });

        self.conn.send_and_check_request(&x::ChangeProperty {
            mode: x::PropMode::Prepend,
            window,
            property: x::ATOM_WM_NAME,
            r#type: x::ATOM_STRING,
            data: MANAGED_HINT.as_bytes(),
        })?;

        let geometry_reply = self.conn.wait_for_reply(geometry_cookie)?;
        let geometry = Geometry::new(
            geometry_reply.x(),
            geometry_reply.y(),
            geometry_reply.width(),
            geometry_reply.height(),
        );

        let id = self.current_id;
        let managed_window = ManagedWindow {
            id,
            geometry,
            variant: WindowVariant::XWindow { window },
        };
        self.managed_windows.insert(id, managed_window);
        self.current_id += 1;

        Ok(id)
    }
}
