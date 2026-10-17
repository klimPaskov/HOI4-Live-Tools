//! Explicit owned lifecycle close; no XTEST, focus, activation or cursor calls.
//! Server acknowledgement is not application exit or a clean-shutdown claim.
use super::{
    x11::{connect_owned, BoundedStream},
    Identity, Process,
};
use crate::error::{Error, Result};
use serde::Serialize;
use std::{path::Path, time::Instant};
use x11rb::{
    connection::Connection,
    protocol::xproto::{
        AtomEnum, ClientMessageEvent, ConnectionExt as _, EventMask, MapState, WindowClass,
    },
    rust_connection::RustConnection,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CloseDelivery {
    NotQueued,
    Unknown,
    ServerConfirmed,
}
#[derive(Debug, Serialize)]
pub struct CloseDispatch {
    pub delivery: CloseDelivery,
    pub error: Option<Error>,
}
impl CloseDispatch {
    pub(crate) fn refused(error: Error) -> Self {
        Self {
            delivery: CloseDelivery::NotQueued,
            error: Some(error),
        }
    }
}

pub(crate) fn valid_class(class: &[u8]) -> bool {
    if class.is_empty() || class.len() > 256 || class.last() != Some(&0) {
        return false;
    }
    let parts: Vec<_> = class.split(|byte| *byte == 0).collect();
    parts.len() == 3 && !parts[0].is_empty() && !parts[1].is_empty() && parts[2].is_empty()
}
fn unavailable(error: impl std::fmt::Display) -> Error {
    Error::new("window_close_unavailable", error.to_string())
}

#[derive(PartialEq, Eq)]
struct WindowWitness {
    class: Vec<u8>,
    protocols: Vec<u32>,
}

pub struct X11WindowClose {
    connection: RustConnection<BoundedStream>,
    root: u32,
    window: u32,
    game: Identity,
    class: Vec<u8>,
    pid_atom: u32,
    protocols_atom: u32,
    delete_atom: u32,
    attempted: bool,
    delivery: CloseDelivery,
}
impl X11WindowClose {
    /// Bind only the explicit window. The same fixed deadline covers connect,
    /// authentication, every property read and this object's one close attempt.
    #[allow(clippy::too_many_arguments)]
    pub fn open_until(
        process: &dyn Process,
        display: u16,
        display_server: &Identity,
        authority: &Path,
        window: u32,
        class: &[u8],
        until: Instant,
    ) -> Result<Self> {
        if window == 0 || !valid_class(class) || Instant::now() >= until {
            return Err(Error::new(
                "invalid_window_binding",
                "Bounded explicit owned window required",
            ));
        }
        process.check()?;
        let game = process
            .identity()
            .ok_or_else(|| Error::new("window_close_unavailable", "Owned process reader required"))?
            .clone();
        let (connection, root) = connect_owned(display, display_server, authority, until)?;
        let atom = |name: &[u8]| -> Result<u32> {
            let atom = connection
                .intern_atom(true, name)
                .map_err(unavailable)?
                .reply()
                .map_err(unavailable)?
                .atom;
            if atom == 0 {
                return Err(Error::new(
                    "window_close_unavailable",
                    "Window protocol atom absent",
                ));
            }
            Ok(atom)
        };
        let closer = Self {
            pid_atom: atom(b"_NET_WM_PID")?,
            protocols_atom: atom(b"WM_PROTOCOLS")?,
            delete_atom: atom(b"WM_DELETE_WINDOW")?,
            connection,
            root,
            window,
            game,
            class: class.to_vec(),
            attempted: false,
            delivery: CloseDelivery::NotQueued,
        };
        closer.check(process)?;
        Ok(closer)
    }
    fn property(&self, atom: u32, kind: AtomEnum, size: u32, format: u8) -> Result<Vec<u8>> {
        let reply = self
            .connection
            .get_property(false, self.window, atom, kind, 0, size)
            .map_err(unavailable)?
            .reply()
            .map_err(unavailable)?;
        if reply.type_ != u32::from(kind) || reply.format != format || reply.bytes_after != 0 {
            return Err(Error::new(
                "invalid_window_binding",
                "Complete typed window property required",
            ));
        }
        Ok(reply.value)
    }
    fn fields(&self) -> Result<WindowWitness> {
        let pid = self.property(self.pid_atom, AtomEnum::CARDINAL, 1, 32)?;
        if pid.as_slice() != (self.game.pid as u32).to_ne_bytes() {
            return Err(Error::new(
                "ownership_changed",
                "Window PID differs from owned game",
            ));
        }
        let class = self.property(AtomEnum::WM_CLASS.into(), AtomEnum::STRING, 64, 8)?;
        if class != self.class {
            return Err(Error::new(
                "ownership_changed",
                "Owned window class changed",
            ));
        }
        let bytes = self.property(self.protocols_atom, AtomEnum::ATOM, 64, 32)?;
        if bytes.is_empty() || bytes.len() > 256 || bytes.len() % 4 != 0 {
            return Err(Error::new(
                "invalid_window_binding",
                "Bounded protocol atom list required",
            ));
        }
        let protocols: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|bytes| u32::from_ne_bytes(bytes.try_into().unwrap()))
            .collect();
        if protocols.contains(&0)
            || protocols
                .iter()
                .filter(|atom| **atom == self.delete_atom)
                .count()
                != 1
        {
            return Err(Error::new(
                "window_close_unavailable",
                "Unique advertised delete protocol required",
            ));
        }
        let attributes = self
            .connection
            .get_window_attributes(self.window)
            .map_err(unavailable)?
            .reply()
            .map_err(unavailable)?;
        let geometry = self
            .connection
            .get_geometry(self.window)
            .map_err(unavailable)?
            .reply()
            .map_err(unavailable)?;
        if attributes.class != WindowClass::INPUT_OUTPUT
            || attributes.map_state != MapState::VIEWABLE
            || geometry.root != self.root
        {
            return Err(Error::new(
                "invalid_window_binding",
                "Owned visible client window on configured screen required",
            ));
        }
        Ok(WindowWitness { class, protocols })
    }
    fn check(&self, process: &dyn Process) -> Result<()> {
        self.connection.stream().remaining().map_err(unavailable)?;
        self.connection.stream().verify_peer()?;
        process.check()?;
        if process.identity() != Some(&self.game) {
            return Err(Error::new(
                "ownership_changed",
                "Close process identity changed",
            ));
        }
        let before = self.fields()?;
        if self.fields()? != before {
            return Err(Error::new(
                "ownership_changed",
                "Window protocol witnesses changed",
            ));
        }
        process.check()?;
        self.connection.stream().verify_peer()?;
        self.connection.stream().remaining().map_err(unavailable)?;
        Ok(())
    }
    /// Exactly one invocation, including preflight refusal. Never rediscover or
    /// replay after a possibly queued request. CURRENT_TIME is the declared SDL
    /// close profile; no application/library acceptance or exit is inferred.
    pub fn request_once(&mut self, process: &dyn Process) -> CloseDispatch {
        if self.attempted {
            return CloseDispatch {
                delivery: self.delivery,
                error: Some(Error::new(
                    "window_close_already_attempted",
                    "Never replay a window close",
                )),
            };
        }
        self.attempted = true;
        if let Err(error) = self.check(process) {
            return CloseDispatch::refused(error);
        }
        self.delivery = CloseDelivery::Unknown; // Before a request may be written.
        let result = (|| -> Result<()> {
            let event = ClientMessageEvent::new(
                32,
                self.window,
                self.protocols_atom,
                [self.delete_atom, x11rb::CURRENT_TIME, 0, 0, 0],
            );
            let cookie = self
                .connection
                .send_event(false, self.window, EventMask::NO_EVENT, event)
                .map_err(unavailable)?;
            self.connection.flush().map_err(unavailable)?;
            cookie.check().map_err(unavailable)?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.delivery = CloseDelivery::ServerConfirmed;
                CloseDispatch {
                    delivery: self.delivery,
                    error: None,
                }
            }
            Err(error) => CloseDispatch {
                delivery: self.delivery,
                error: Some(error),
            },
        }
    }
}
