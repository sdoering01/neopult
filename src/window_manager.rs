use anyhow::Context;
use log::{debug, error, warn};
use mlua::{Function, Lua, RegistryKey, Value};
use std::collections::HashMap;
use std::fmt::Debug;
use std::str::{self, FromStr};
use std::sync::Arc;
use xcb::{randr, x, Connection, Xid};

const MANAGED_HINT: &str = "MANAGED";

const MIN_Z: u16 = 1;
const MAX_Z: u16 = 0;

pub type ManagedWid = usize;

#[derive(Debug)]
pub struct ManagedWindow {
    id: ManagedWid,
    variant: WindowVariant,
    min_geometry: MinGeometry,
    mode: Mode,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum Mode {
    Max { width: u16, height: u16 },
    Min,
    Hidden,
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

impl ToString for Alignment {
    fn to_string(&self) -> String {
        match self {
            Alignment::TopLeft => "lt",
            Alignment::TopRight => "rt",
            Alignment::BottomRight => "rb",
            Alignment::BottomLeft => "lb",
        }
        .to_string()
    }
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
    fn from_width_height(width: u16, height: u16) -> Self {
        AlignedGeometry {
            x_offset: 0,
            y_offset: 0,
            width,
            height,
            alignment: Alignment::TopLeft,
        }
    }

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
    fn get_geometry(&self, _lua: &Lua) -> AlignedGeometry {
        match self {
            MinGeometry::Fixed(aligned_geometry) => *aligned_geometry,
            // TODO: Call lua function to get geometry
            MinGeometry::Dynamic { callback_key: _ } => todo!(),
        }
    }
}

#[derive(Debug)]
pub struct VirtualWindowCallbacks {
    pub set_geometry_key: RegistryKey,
    pub map_key: RegistryKey,
    pub unmap_key: RegistryKey,
}

#[derive(Debug)]
enum WindowVariant {
    XWindow {
        window: x::Window,
    },
    VirtualWindow {
        name: String,
        callbacks: VirtualWindowCallbacks,
    },
}

// NOTE: Remember to adjust Debug implementation when changing something here
pub struct WindowManager {
    conn: Connection,
    screen: x::ScreenBuf,
    screen_height: u16,
    screen_width: u16,
    current_id: ManagedWid,
    managed_windows: HashMap<ManagedWid, ManagedWindow>,
    primary_window: Option<ManagedWid>,
    managed_atom: x::Atom,
}

// xcb::Connection doesn't implement Debug, so we have to implement Debug ourselves
impl Debug for WindowManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowManager")
            .field("screen", &self.screen)
            .field("screen_height", &self.screen_height)
            .field("screen_width", &self.screen_width)
            .field("current_id", &self.screen)
            .field("managed_windows", &self.managed_windows)
            .field("primary_window", &self.primary_window)
            .field("managed_atom", &self.managed_atom)
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
        let screen_width = crtc_info_reply.width();
        let screen_height = crtc_info_reply.height();
        debug!("screen has size {}x{}", screen_width, screen_height);

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

        let managed_atom_name = "_NEOPULT_MANAGED";
        debug!(
            "creating intern atom for managed state with name {}",
            managed_atom_name
        );

        let cookie = conn.send_request(&x::InternAtom {
            only_if_exists: false,
            name: managed_atom_name.as_bytes(),
        });
        let reply = conn
            .wait_for_reply(cookie)
            .context("error while waiting for intern atom reply")?;
        let managed_atom = reply.atom();

        Ok(WindowManager {
            conn,
            screen: screen.to_owned(),
            screen_height,
            screen_width,
            current_id: 0,
            managed_windows: HashMap::new(),
            primary_window: None,
            managed_atom,
        })
    }

    pub fn get_window_by_class(&self, to_claim: &str) -> anyhow::Result<Option<x::Window>> {
        let cookie = self.conn.send_request(&x::QueryTree {
            window: self.screen.root(),
        });
        let reply = self
            .conn
            .wait_for_reply(cookie)
            .context("error while waiting for QueryTree reply")?;

        let children: &[x::Window] = reply.children();

        let mut children_cookies = Vec::with_capacity(children.len());

        for child in children.iter() {
            let class_cookie = self.conn.send_request(&x::GetProperty {
                delete: false,
                window: *child,
                property: x::ATOM_WM_CLASS,
                r#type: x::ATOM_STRING,
                long_offset: 0,
                // Amount of chars of name to retrieve
                long_length: 128,
            });
            let managed_cookie = self.conn.send_request(&x::GetProperty {
                delete: false,
                window: *child,
                property: self.managed_atom,
                r#type: x::ATOM_STRING,
                long_offset: 0,
                long_length: MANAGED_HINT.len() as u32,
            });
            children_cookies.push((class_cookie, managed_cookie));
        }

        for ((name_cookie, managed_cookie), &window) in children_cookies.into_iter().zip(children) {
            match self.conn.wait_for_reply(name_cookie) {
                Err(e) => error!("error while waiting for WM_CLASS reply: {}", e),
                Ok(class_reply) => {
                    let class = String::from_utf8_lossy(class_reply.value());
                    if class.contains(to_claim) {
                        let managed_reply = self.conn.wait_for_reply(managed_cookie)?;
                        let managed_hint = str::from_utf8(managed_reply.value()).unwrap();
                        if managed_hint != MANAGED_HINT {
                            return Ok(Some(window));
                        }
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
            mode: x::PropMode::Replace,
            window,
            property: self.managed_atom,
            r#type: x::ATOM_STRING,
            data: MANAGED_HINT.as_bytes(),
        })?;
        let id = self.current_id;
        let managed_window = ManagedWindow {
            id,
            variant: WindowVariant::XWindow { window },
            min_geometry,
            mode: Mode::Min,
        };

        let geometry = managed_window.min_geometry.get_geometry(lua);
        self.change_window_geometry(lua, &managed_window, geometry, MIN_Z)?;

        self.managed_windows.insert(id, managed_window);
        self.current_id += 1;

        Ok(id)
    }

    pub fn manage_virtual_window(
        &mut self,
        lua: &Lua,
        name: String,
        callbacks: VirtualWindowCallbacks,
        min_geometry: MinGeometry,
    ) -> anyhow::Result<ManagedWid> {
        let id = self.current_id;
        let managed_window = ManagedWindow {
            id,
            variant: WindowVariant::VirtualWindow { name, callbacks },
            min_geometry,
            mode: Mode::Min,
        };

        let geometry = managed_window.min_geometry.get_geometry(lua);
        self.change_window_geometry(lua, &managed_window, geometry, MIN_Z)?;

        self.managed_windows.insert(id, managed_window);
        self.current_id += 1;

        Ok(id)
    }

    pub fn max_window(
        &mut self,
        lua: &Lua,
        id: ManagedWid,
        (width, height): (u16, u16),
    ) -> anyhow::Result<()> {
        self.ensure_managed(id)?;

        let window = self.managed_windows.get_mut(&id).unwrap();
        let was_hidden = window.mode == Mode::Hidden;
        window.mode = Mode::Max { width, height };
        drop(window);

        if was_hidden {
            let window = self.managed_windows.get(&id).unwrap();
            self.map_window(lua, window)?;
        }

        self.primary_window = Some(id);
        self.reposition_windows(lua)?;

        Ok(())
    }

    pub fn min_window(&mut self, lua: &Lua, id: ManagedWid) -> anyhow::Result<()> {
        self.ensure_managed(id)?;

        let window = self.managed_windows.get_mut(&id).unwrap();
        let was_hidden = window.mode == Mode::Hidden;
        window.mode = Mode::Min;

        if self.primary_window == Some(id) {
            debug!("primary window set to min, finding new primary window");
            self.primary_window = self.find_new_primary_window();
            match self.primary_window {
                Some(wid) => debug!("found new primary window with managed wid {}", wid),
                None => debug!("didn't find new primary window"),
            }
            self.reposition_windows(lua)?;
        }

        let window = self.managed_windows.get(&id).unwrap();
        if was_hidden {
            self.map_window(lua, window)?;
        }
        self.change_window_geometry(lua, window, window.min_geometry.get_geometry(lua), MIN_Z)?;

        Ok(())
    }

    pub fn hide_window(&mut self, lua: &Lua, id: ManagedWid) -> anyhow::Result<()> {
        self.ensure_managed(id)?;

        let window = self.managed_windows.get_mut(&id).unwrap();
        let was_shown = window.mode != Mode::Hidden;
        window.mode = Mode::Hidden;
        drop(window);

        if was_shown {
            let window = self.managed_windows.get(&id).unwrap();
            self.unmap_window(lua, window)?;

            if self.primary_window == Some(id) {
                debug!("primary window hidden, finding new primary window");
                self.primary_window = self.find_new_primary_window();
                match self.primary_window {
                    Some(wid) => debug!("found new primary window with managed wid {}", wid),
                    None => debug!("didn't find new primary window"),
                }
                self.reposition_windows(lua)?;
            }
        }

        Ok(())
    }

    pub fn release_window(&mut self, lua: &Lua, id: ManagedWid) -> anyhow::Result<()> {
        self.ensure_managed(id)?;

        let window = self.managed_windows.remove(&id).unwrap();
        if self.primary_window == Some(window.id) {
            debug!("primary window released, finding new primary window");
            self.primary_window = self.find_new_primary_window();
            match self.primary_window {
                Some(wid) => debug!("found new primary window with managed wid {}", wid),
                None => debug!("didn't find new primary window"),
            }
            self.reposition_windows(lua)?;
        }

        Ok(())
    }

    fn ensure_managed(&self, id: ManagedWid) -> anyhow::Result<()> {
        if self.managed_windows.contains_key(&id) {
            Ok(())
        } else {
            anyhow::bail!("there is no managed window for the managed wid {}", id)
        }
    }

    fn map_window(&self, lua: &Lua, window: &ManagedWindow) -> xcb::Result<()> {
        match &window.variant {
            WindowVariant::XWindow { window } => {
                self.conn
                    .send_and_check_request(&x::MapWindow { window: *window })?;
            }
            WindowVariant::VirtualWindow { name, callbacks } => {
                match lua.registry_value::<Function>(&callbacks.map_key) {
                    Ok(callback) => {
                        if let Err(e) = callback.call::<_, Value>(()) {
                            error!(
                                "error when calling map callback on virtual window with \
                                   name {} (managed wid {}): {}",
                                name, window.id, e
                            );
                        }
                    }
                    Err(_) => {
                        error!(
                            "callback wasn't a function in lua registry when calling map \
                               on virtual window with name {} (managed wid {})",
                            name, window.id
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn unmap_window(&self, lua: &Lua, window: &ManagedWindow) -> xcb::Result<()> {
        match &window.variant {
            WindowVariant::XWindow { window } => {
                self.conn
                    .send_and_check_request(&x::UnmapWindow { window: *window })?;
            }
            WindowVariant::VirtualWindow { name, callbacks } => {
                match lua.registry_value::<Function>(&callbacks.unmap_key) {
                    Ok(callback) => {
                        if let Err(e) = callback.call::<_, Value>(()) {
                            error!(
                                "error when calling unmap callback on virtual window with \
                                   name {} (managed wid {}): {}",
                                name, window.id, e
                            );
                        }
                    }
                    Err(_) => {
                        error!(
                            "callback wasn't a function in lua registry when calling unmap \
                               on virtual window with name {} (managed wid {})",
                            name, window.id
                        );
                    }
                }
            }
        }
        Ok(())
    }

    fn reposition_windows(&mut self, lua: &Lua) -> anyhow::Result<()> {
        if let Some(primary_window_id) = self.primary_window {
            let primary_window = self
                .managed_windows
                .get(&primary_window_id)
                .expect("primary window is not a managed window");
            match primary_window.mode {
                Mode::Max { width, height } => {
                    self.change_window_geometry(
                        lua,
                        primary_window,
                        AlignedGeometry::from_width_height(width, height),
                        MAX_Z,
                    )?;
                    self.change_screen_resolution((width, height))?;
                }
                Mode::Min | Mode::Hidden => {
                    anyhow::bail!("primary window isn't in max mode");
                }
            }
        }

        // TODO: Define some kind of z-order to handle overlapping min windows
        for window in self.managed_windows.values() {
            if window.mode == Mode::Min {
                self.change_window_geometry(lua, window, window.min_geometry.get_geometry(lua), MIN_Z)?;
            }
        }

        Ok(())
    }

    fn find_new_primary_window(&mut self) -> Option<ManagedWid> {
        // TODO: Implement some kind of order, so that the window that was max most recently is the
        // new primary window?
        self.managed_windows
            .values()
            .find(|w| matches!(w.mode, Mode::Max { .. }))
            .map(|w| w.id)
    }

    fn change_window_geometry(
        &self,
        lua: &Lua,
        managed_window: &ManagedWindow,
        aligned_geometry: AlignedGeometry,
        z: u16,
    ) -> xcb::Result<()> {
        match &managed_window.variant {
            WindowVariant::XWindow { window } => {
                let geometry = aligned_geometry.into_geometry(self);
                self.conn.send_and_check_request(&x::ConfigureWindow {
                    window: *window,
                    value_list: &[
                        x::ConfigWindow::X(geometry.x as i32),
                        x::ConfigWindow::Y(geometry.y as i32),
                        x::ConfigWindow::Width(geometry.width as u32),
                        x::ConfigWindow::Height(geometry.height as u32),
                        // This also raises the window
                        x::ConfigWindow::StackMode(x::StackMode::Above),
                    ],
                })?;
            }
            // TODO: Either implement 'raise' or z-order
            WindowVariant::VirtualWindow { name, callbacks } => {
                match lua.registry_value::<Function>(&callbacks.set_geometry_key) {
                    Ok(callback) => {
                        if let Err(e) = callback.call::<_, Value>((
                            aligned_geometry.x_offset,
                            aligned_geometry.y_offset,
                            aligned_geometry.width,
                            aligned_geometry.height,
                            aligned_geometry.alignment.to_string(),
                            z,
                        )) {
                            error!(
                                "error when calling set geometry callback on virtual window with \
                                   name {} (managed wid {}): {}",
                                name, managed_window.id, e
                            );
                        }
                    }
                    Err(_) => {
                        error!(
                            "callback wasn't a function in lua registry when calling set_geometry \
                               on virtual window with name {} (managed wid {})",
                            name, managed_window.id
                        );
                    }
                }
            }
        }

        Ok(())
    }

    fn change_screen_resolution(
        &mut self,
        (target_width, target_height): (u16, u16),
    ) -> xcb::Result<()> {
        let cookie = self.conn.send_request(&randr::GetScreenSizeRange {
            window: self.screen.root(),
        });
        let screen_size_range = self.conn.wait_for_reply(cookie)?;

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

        let cookie = self.conn.send_request(&randr::GetScreenResources {
            window: self.screen.root(),
        });
        let screen_resources = self.conn.wait_for_reply(cookie)?;

        let crtc = *screen_resources
            .crtcs()
            .first()
            .expect("no crtc in screen resources");

        let cookie = self.conn.send_request(&randr::GetCrtcInfo {
            crtc,
            config_timestamp: x::CURRENT_TIME,
        });
        let crtc_info = self.conn.wait_for_reply(cookie)?;
        let current_width = crtc_info.width();
        let current_height = crtc_info.height();

        if current_width == target_width && current_height == target_height {
            return Ok(());
        }

        // If one dimension grows and the other shrinks, we resize in two steps:
        //  1. Grow the one dimension of the screen to the bigger target dimension
        //  2. Shrink the other dimension of the screen to the smaller target dimension
        if target_width < current_width && target_height > current_height {
            self.randr_set_screen_size((current_width, target_height))?;
        }

        if target_width > current_width && target_height < current_height {
            self.randr_set_screen_size((target_width, current_height))?;
        }

        // SetCrtcConfig only works if the target output size fits into the current screen size. We
        // can't use it directly when going from 200x300 to 300x200 because the width grows.
        if target_width < current_width || target_height < current_height {
            self.randr_set_output_size((target_width, target_height))?;
        }

        // SetScreenSize only works if the output size (mode from SetCrtcConfig) fits into the
        // target screen size. To achieve that, we shrink the output first.
        self.randr_set_screen_size((target_width, target_height))?;
        // Updating output size accordlingly, so that GetCrtcInfo returns the correct size
        self.randr_set_output_size((target_width, target_height))?;

        self.screen_width = target_width;
        self.screen_height = target_height;

        Ok(())
    }

    fn randr_set_screen_size(&self, (width, height): (u16, u16)) -> xcb::Result<()> {
        self.conn.send_and_check_request(&randr::SetScreenSize {
            window: self.screen.root(),
            width,
            height,
            // These two don't really matter for displays without a physical monitor
            mm_width: 200,
            mm_height: 200,
        })?;
        Ok(())
    }

    fn randr_set_output_size(&self, (width, height): (u16, u16)) -> xcb::Result<()> {
        let cookie = self.conn.send_request(&randr::GetScreenResources {
            window: self.screen.root(),
        });
        let screen_resources = self.conn.wait_for_reply(cookie)?;
        let output = screen_resources.outputs()[0];
        let crtc = screen_resources.crtcs()[0];

        let target_mode_opt = screen_resources
            .modes()
            .iter()
            .find(|m| m.width == width && m.height == height);

        let mode = match target_mode_opt {
            Some(target_mode) => {
                let target_mode_id = target_mode.id;
                let cookie = self.conn.send_request(&randr::GetOutputInfo {
                    output,
                    config_timestamp: x::CURRENT_TIME,
                });
                let output_info = self.conn.wait_for_reply(cookie)?;

                let mode = *output_info
                    .modes()
                    .iter()
                    .find(|m| m.resource_id() == target_mode_id)
                    .expect("no matching mode on output");
                mode
            }
            None => {
                let id = self.conn.generate_id::<randr::Mode>().resource_id();
                let name_len = width.to_string().len() + height.to_string().len() + 1;
                let name = format!("{}x{}", width, height);
                debug!(
                    "new mode, id: {:?}, name_len: {}, name: {}",
                    id, name_len, name
                );
                // Values reverse engineered from GetScreenResources output and existing modes
                let cookie = self.conn.send_request(&randr::CreateMode {
                    window: self.screen.root(),
                    mode_info: randr::ModeInfo {
                        id,
                        width,
                        height,
                        name_len: name_len as u16,
                        dot_clock: 60 * width as u32 * height as u32, // 60 fps
                        hsync_start: 0,
                        hsync_end: 0,
                        htotal: width,
                        hskew: 0,
                        vsync_start: 0,
                        vsync_end: 0,
                        vtotal: height,
                        mode_flags: randr::ModeFlag::empty(),
                    },
                    name: name.as_bytes(),
                });
                let create_mode_resp = self.conn.wait_for_reply(cookie)?;
                let mode = create_mode_resp.mode();

                self.conn
                    .send_and_check_request(&randr::AddOutputMode { output, mode })?;

                mode
            }
        };

        let cookie = self.conn.send_request(&randr::SetCrtcConfig {
            crtc,
            timestamp: x::CURRENT_TIME,
            config_timestamp: x::CURRENT_TIME,
            x: 0,
            y: 0,
            mode,
            rotation: randr::Rotation::ROTATE_0,
            outputs: &[output],
        });
        let _ = self.conn.wait_for_reply(cookie)?;

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
