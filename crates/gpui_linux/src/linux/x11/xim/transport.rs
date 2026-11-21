// From zed-xim 0.4.0-zed (https://github.com/zed-industries/xim-rs, MIT, see LICENSE here),
// the client half only. Changes are listed in ../xim.rs.

//! The XIM transport over [`x11rb`]: client messages, and a window property for what does
//! not fit in one.

use std::{rc::Rc, sync::Arc};
use x11rb::protocol::xproto::EventMask;

use super::client::{
    ClientCore, ClientError, ClientHandler, handle_request as client_handle_request,
};
use super::{AHashMap, Atoms};
use xim_parser::{Attr, AttributeName};

use x11rb::xcb_ffi::XCBConnection;

#[allow(unused_imports)]
use x11rb::{
    COPY_DEPTH_FROM_PARENT, CURRENT_TIME,
    connection::Connection,
    errors::{ConnectError, ConnectionError, ParseError, ReplyError, ReplyOrIdError},
    protocol::{
        Event,
        xproto::{
            Atom, AtomEnum, CLIENT_MESSAGE_EVENT, ClientMessageEvent, ConnectionExt, KeyPressEvent,
            PropMode, SELECTION_NOTIFY_EVENT, Screen, SelectionNotifyEvent, SelectionRequestEvent,
            Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

use xim_parser::{Request, XimWrite};

macro_rules! convert_error {
    ($($ty:ty,)+) => {
        $(
            impl From<$ty> for ClientError {
                fn from(err: $ty) -> Self {
                    ClientError::Other(err.into())
                }
            }
        )+
    };
}

convert_error!(
    ConnectError,
    ConnectionError,
    ReplyError,
    ReplyOrIdError,
    ParseError,
);

pub trait HasConnection {
    type Connection: Connection + ConnectionExt;

    fn conn(&self) -> &Self::Connection;
}

impl HasConnection for XCBConnection {
    type Connection = Self;

    #[inline(always)]
    fn conn(&self) -> &Self::Connection {
        self
    }
}

impl HasConnection for RustConnection {
    type Connection = Self;

    #[inline(always)]
    fn conn(&self) -> &Self::Connection {
        self
    }
}

impl<C: HasConnection> HasConnection for X11rbClient<C> {
    type Connection = C::Connection;

    #[inline(always)]
    fn conn(&self) -> &Self::Connection {
        self.has_conn.conn()
    }
}

impl<C: HasConnection> HasConnection for &C {
    type Connection = C::Connection;

    #[inline(always)]
    fn conn(&self) -> &Self::Connection {
        (**self).conn()
    }
}

impl<C: HasConnection> HasConnection for Rc<C> {
    type Connection = C::Connection;

    #[inline(always)]
    fn conn(&self) -> &Self::Connection {
        (**self).conn()
    }
}

impl<C: HasConnection> HasConnection for Arc<C> {
    type Connection = C::Connection;

    #[inline(always)]
    fn conn(&self) -> &Self::Connection {
        (**self).conn()
    }
}

pub struct X11rbClient<C: HasConnection> {
    has_conn: C,
    server_owner_window: Window,
    im_window: Window,
    server_atom: Atom,
    atoms: Atoms<Atom>,
    transport_max: usize,
    client_window: u32,
    im_attributes: AHashMap<AttributeName, u16>,
    ic_attributes: AHashMap<AttributeName, u16>,
    sequence: u16,
    buf: Vec<u8>,
}

impl<C: HasConnection> X11rbClient<C> {
    pub fn init(
        has_conn: C,
        screen_num: usize,
        im_name: Option<&str>,
    ) -> Result<Self, ClientError> {
        let conn = has_conn.conn();
        let screen = &conn.setup().roots[screen_num];
        let client_window = conn.generate_id()?;

        conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            client_window,
            screen.root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_ONLY,
            screen.root_visual,
            &Default::default(),
        )?;

        let var = std::env::var("XMODIFIERS").ok();
        let var = var.as_ref().and_then(|n| n.strip_prefix("@im="));
        let im_name = im_name.or(var).ok_or(ClientError::NoXimServer)?;

        log::info!("Try connect {}", im_name);

        let atoms = Atoms::new::<ClientError, _>(|name| {
            Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
        })?;
        let server_reply = conn
            .get_property(
                false,
                screen.root,
                atoms.XIM_SERVERS,
                AtomEnum::ATOM,
                0,
                u32::MAX,
            )?
            .reply()?;

        if server_reply.type_ != u32::from(AtomEnum::ATOM) || server_reply.format != 32 {
            Err(ClientError::InvalidReply)
        } else {
            for server_atom in server_reply.value32().ok_or(ClientError::InvalidReply)? {
                let server_owner = conn.get_selection_owner(server_atom)?.reply()?.owner;
                let name = conn.get_atom_name(server_atom)?.reply()?.name;

                let name = match String::from_utf8(name) {
                    Ok(name) => name,
                    _ => continue,
                };

                if let Some(name) = name.strip_prefix("@server=") {
                    if name == im_name {
                        conn.convert_selection(
                            client_window,
                            server_atom,
                            atoms.TRANSPORT,
                            atoms.TRANSPORT,
                            CURRENT_TIME,
                        )?;

                        conn.flush()?;

                        return Ok(Self {
                            has_conn,
                            atoms,
                            server_atom,
                            server_owner_window: server_owner,
                            im_attributes: AHashMap::with_hasher(Default::default()),
                            ic_attributes: AHashMap::with_hasher(Default::default()),
                            im_window: x11rb::NONE,
                            transport_max: 20,
                            client_window,
                            sequence: 0,
                            buf: Vec::with_capacity(1024),
                        });
                    }
                }
            }

            Err(ClientError::NoXimServer)
        }
    }

    pub fn filter_event(
        &mut self,
        e: &Event,
        handler: &mut impl ClientHandler<Self>,
    ) -> Result<bool, ClientError> {
        match e {
            Event::SelectionNotify(e) if e.requestor == self.client_window => {
                if e.property == self.atoms.LOCALES {
                    // TODO: set locale
                    let _locale = self
                        .conn()
                        .get_property(
                            true,
                            self.client_window,
                            self.atoms.LOCALES,
                            self.atoms.LOCALES,
                            0,
                            u32::MAX,
                        )?
                        .reply()?;

                    self.xconnect()?;

                    Ok(true)
                } else if e.property == self.atoms.TRANSPORT {
                    let transport = self
                        .conn()
                        .get_property(
                            true,
                            self.client_window,
                            self.atoms.TRANSPORT,
                            self.atoms.TRANSPORT,
                            0,
                            u32::MAX,
                        )?
                        .reply()?;

                    if !transport.value.starts_with(b"@transport=X/") {
                        return Err(ClientError::UnsupportedTransport);
                    }

                    self.conn().convert_selection(
                        self.client_window,
                        self.server_atom,
                        self.atoms.LOCALES,
                        self.atoms.LOCALES,
                        CURRENT_TIME,
                    )?;

                    self.conn().flush()?;

                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Event::ClientMessage(msg) if msg.window == self.client_window => {
                if msg.type_ == self.atoms.XIM_XCONNECT {
                    let [im_window, major, minor, max, _] = msg.data.as_data32();
                    log::info!(
                        "XConnected server on {}, transport version: {}.{}, TRANSPORT_MAX: {}",
                        im_window,
                        major,
                        minor,
                        max
                    );
                    self.im_window = im_window;
                    self.transport_max = max as usize;
                    self.send_req(Request::Connect {
                        client_major_protocol_version: 1,
                        client_minor_protocol_version: 0,
                        endian: xim_parser::Endian::Native,
                        client_auth_protocol_names: Vec::new(),
                    })?;
                    Ok(true)
                } else if msg.type_ == self.atoms.XIM_PROTOCOL {
                    self.handle_xim_protocol(msg, handler)?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Event::Error(err)
                if err.error_kind == x11rb::protocol::ErrorKind::Window
                    && err.bad_value == self.im_window =>
            {
                Err(ClientError::NoXimServer)
            }
            _ => Ok(false),
        }
    }

    fn handle_xim_protocol(
        &mut self,
        msg: &ClientMessageEvent,
        handler: &mut impl ClientHandler<Self>,
    ) -> Result<(), ClientError> {
        if msg.format == 32 {
            let [length, atom, ..] = msg.data.as_data32();
            let reply = self
                .conn()
                .get_property(true, msg.window, atom, AtomEnum::ANY, 0, length)?
                .reply()?;
            // handle fcitx4 occasionally sending empty reply
            if reply.value_len == 0 {
                return Err(ClientError::InvalidReply);
            }
            let data = reply.value;
            let req = xim_parser::read(&data)?;
            client_handle_request(self, handler, req)?;
        } else if msg.format == 8 {
            let data = msg.data.as_data8();
            let req: xim_parser::Request = xim_parser::read(&data)?;
            client_handle_request(self, handler, req)?;
        }

        Ok(())
    }

    fn xconnect(&mut self) -> Result<(), ClientError> {
        self.conn().send_event(
            false,
            self.server_owner_window,
            EventMask::NO_EVENT,
            ClientMessageEvent {
                data: [self.client_window, 0, 0, 0, 0].into(),
                format: 32,
                response_type: CLIENT_MESSAGE_EVENT,
                sequence: 0,
                type_: self.atoms.XIM_XCONNECT,
                window: self.server_owner_window,
            },
        )?;

        self.conn().flush()?;

        Ok(())
    }
}

impl<C: HasConnection> ClientCore for X11rbClient<C> {
    type XEvent = KeyPressEvent;
    fn set_attrs(&mut self, im_attrs: Vec<Attr>, ic_attrs: Vec<Attr>) {
        for im_attr in im_attrs {
            self.im_attributes.insert(im_attr.name, im_attr.id);
        }

        for ic_attr in ic_attrs {
            self.ic_attributes.insert(ic_attr.name, ic_attr.id);
        }
    }

    #[inline]
    fn ic_attributes(&self) -> &AHashMap<AttributeName, u16> {
        &self.ic_attributes
    }

    #[inline]
    fn im_attributes(&self) -> &AHashMap<AttributeName, u16> {
        &self.im_attributes
    }

    #[inline]
    fn serialize_event(&self, xev: &Self::XEvent) -> xim_parser::XEvent {
        xim_parser::XEvent {
            response_type: xev.response_type,
            detail: xev.detail,
            sequence: xev.sequence,
            time: xev.time,
            root: xev.root,
            event: xev.event,
            child: xev.child,
            root_x: xev.root_x,
            root_y: xev.root_y,
            event_x: xev.event_x,
            event_y: xev.event_y,
            state: xev.state.into(),
            same_screen: xev.same_screen,
        }
    }

    #[inline]
    fn deserialize_event(&self, xev: &xim_parser::XEvent) -> Self::XEvent {
        deserialize_event_impl(xev)
    }

    #[inline]
    fn send_req(&mut self, req: Request) -> Result<(), ClientError> {
        send_req_impl(
            &self.has_conn,
            &self.atoms,
            self.im_window,
            &mut self.buf,
            &mut self.sequence,
            self.transport_max,
            &req,
        )
    }
}

fn send_req_impl<C: HasConnection>(
    c: &C,
    atoms: &Atoms<Atom>,
    target: Window,
    buf: &mut Vec<u8>,
    sequence: &mut u16,
    transport_max: usize,
    req: &Request,
) -> Result<(), ClientError> {
    if log::log_enabled!(log::Level::Trace) {
        log::trace!("->: {:?}", req);
    } else {
        log::debug!("->: {}", req.name());
    }
    buf.resize(req.size(), 0);
    xim_parser::write(req, buf);

    // One client message holds 20 bytes. Anything longer goes through a property, whatever
    // the server's TRANSPORT_MAX: this client connects with transport version 0.0, which has
    // no message split over several client messages.
    if buf.len() <= 20 && buf.len() < transport_max {
        let mut data = [0u8; 20];
        data[..buf.len()].copy_from_slice(buf);
        c.conn().send_event(
            false,
            target,
            EventMask::NO_EVENT,
            ClientMessageEvent {
                response_type: CLIENT_MESSAGE_EVENT,
                data: data.into(),
                format: 8,
                sequence: 0,
                type_: atoms.XIM_PROTOCOL,
                window: target,
            },
        )?;
    } else {
        let prop = c
            .conn()
            .intern_atom(false, format!("_XIM_DATA_{}", sequence).as_bytes())?
            .reply()?
            .atom;
        *sequence = sequence.wrapping_add(1);
        c.conn().change_property(
            PropMode::APPEND,
            target,
            prop,
            AtomEnum::STRING,
            8,
            buf.len() as u32,
            buf,
        )?;
        c.conn().send_event(
            false,
            target,
            EventMask::NO_EVENT,
            ClientMessageEvent {
                data: [buf.len() as u32, prop, 0, 0, 0].into(),
                format: 32,
                sequence: 0,
                response_type: CLIENT_MESSAGE_EVENT,
                type_: atoms.XIM_PROTOCOL,
                window: target,
            },
        )?;
    }
    buf.clear();
    c.conn().flush()?;
    Ok(())
}

#[inline]
fn deserialize_event_impl(xev: &xim_parser::XEvent) -> KeyPressEvent {
    KeyPressEvent {
        response_type: xev.response_type,
        detail: xev.detail,
        sequence: xev.sequence,
        time: xev.time,
        root: xev.root,
        event: xev.event,
        child: xev.child,
        root_x: xev.root_x,
        root_y: xev.root_y,
        event_x: xev.event_x,
        event_y: xev.event_y,
        state: xev.state.into(),
        same_screen: xev.same_screen,
    }
}
