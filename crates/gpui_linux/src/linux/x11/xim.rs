//! The X Input Method (XIM) client: what an input method server (IBus, Fcitx, uim, the X
//! server's own) and this application say to each other about keys, preedit and committed text.
//!
//! This is the client half of zed-xim 0.4.0-zed (https://github.com/zed-industries/xim-rs,
//! a fork of https://github.com/Riey/xim-rs; MIT, see `xim/LICENSE`), kept here because text
//! from a keyboard could end the application:
//!
//! - Committed and preedit text is decoded by `compound_text`, which knows the character sets
//!   Xlib writes (ISO 8859-x, KS C 5601 and GB 2312 in either half, JIS, UTF-8 and extended
//!   segments) and never fails. The crate decoded UTF-8 and one Japanese set and panicked on
//!   the rest, so the first Cyrillic, Greek or accented letter typed through an input method
//!   was a crash.
//! - A request longer than one client message goes through a property instead of `todo!()`.
//! - The server and Xlib halves are left out, and the maps are the standard library's.

// A protocol's worth of names, of which gpui uses some.
#![allow(dead_code)]

mod attribute_builder;
mod client;
mod compound_text;
mod transport;

pub(crate) use client::{Client, ClientError, ClientHandler};
pub(crate) use transport::X11rbClient;
pub(crate) use xim_parser::*;

pub(crate) type AHashMap<K, V> = std::collections::HashMap<K, V>;

#[allow(non_snake_case)]
struct Atoms<Atom> {
    XIM_SERVERS: Atom,
    LOCALES: Atom,
    TRANSPORT: Atom,
    XIM_XCONNECT: Atom,
    XIM_PROTOCOL: Atom,
}

impl<Atom> Atoms<Atom> {
    fn new<E, F>(f: F) -> Result<Self, E>
    where
        F: Fn(&'static str) -> Result<Atom, E>,
    {
        Ok(Self {
            XIM_SERVERS: f("XIM_SERVERS")?,
            LOCALES: f("LOCALES")?,
            TRANSPORT: f("TRANSPORT")?,
            XIM_XCONNECT: f("_XIM_XCONNECT")?,
            XIM_PROTOCOL: f("_XIM_PROTOCOL")?,
        })
    }
}
