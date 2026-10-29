// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Frames as typed messages: the header picks the server, the direction
//! and ID pick the template, the LL/CP parameters follow. `encode` gives
//! back exactly the bytes `decode` read.

use crate::frame::{Frame, Header, Param};
use crate::server::ServerId;
use crate::{Dir, Error, central, common, database, dtaq, file, netprt, rmtcmd, signon};

/// A frame's template, typed by server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// Seed exchange, start server, end job (any server).
    Common(common::Template),
    /// The sign-on server.
    Signon(signon::Template),
    /// The central server.
    Central(central::Template),
    /// Remote command and program call.
    RemoteCommand(rmtcmd::Template),
    /// The database server.
    Database(database::Template),
    /// The data queue server.
    DataQueue(dtaq::Template),
    /// The file server.
    File(file::Template),
    /// The network print server.
    NetPrint(netprt::Template),
    /// A server ID this codec does not know.
    Unknown(Vec<u8>),
}

impl Body {
    /// Type a template.
    pub fn decode(dir: Dir, server: u16, id: u16, b: &[u8]) -> Self {
        if common::Template::knows(dir, id) {
            return Self::Common(common::Template::decode(dir, id, b));
        }
        match ServerId::from_id(server) {
            Some(ServerId::Signon) => Self::Signon(signon::Template::decode(dir, id, b)),
            Some(ServerId::Central) => Self::Central(central::Template::decode(dir, id, b)),
            Some(ServerId::RemoteCommand) => {
                Self::RemoteCommand(rmtcmd::Template::decode(dir, id, b))
            }
            Some(ServerId::Database) => Self::Database(database::Template::decode(dir, id, b)),
            Some(ServerId::DataQueue) => Self::DataQueue(dtaq::Template::decode(dir, id, b)),
            Some(ServerId::File) => Self::File(file::Template::decode(dir, id, b)),
            Some(ServerId::NetPrint) => Self::NetPrint(netprt::Template::decode(dir, id, b)),
            None => Self::Unknown(b.to_vec()),
        }
    }

    /// The template bytes.
    pub fn encode(&self) -> Vec<u8> {
        match self {
            Self::Common(t) => t.encode(),
            Self::Signon(t) => t.encode(),
            Self::Central(t) => t.encode(),
            Self::RemoteCommand(t) => t.encode(),
            Self::Database(t) => t.encode(),
            Self::DataQueue(t) => t.encode(),
            Self::File(t) => t.encode(),
            Self::NetPrint(t) => t.encode(),
            Self::Unknown(b) => b.clone(),
        }
    }

    /// Whether the template has a typed layout.
    pub fn is_typed(&self) -> bool {
        match self {
            Self::Common(t) => t.is_typed(),
            Self::Signon(t) => t.is_typed(),
            Self::Central(t) => t.is_typed(),
            Self::RemoteCommand(t) => t.is_typed(),
            Self::Database(t) => t.is_typed(),
            Self::DataQueue(t) => t.is_typed(),
            Self::File(t) => t.is_typed(),
            Self::NetPrint(t) => t.is_typed(),
            Self::Unknown(_) => false,
        }
    }
}

/// A decoded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    /// Which way it went.
    pub dir: Dir,
    /// The header as sent.
    pub header: Header,
    /// The typed template.
    pub body: Body,
    /// The LL/CP parameters.
    pub params: Vec<Param>,
    /// Bytes after the last whole parameter.
    pub tail: Vec<u8>,
}

impl Message {
    /// Decode one whole frame.
    pub fn decode(dir: Dir, b: &[u8]) -> Result<Self, Error> {
        let f = Frame::decode(b)?;
        let body = Body::decode(dir, f.header.server_id, f.header.req_rep_id, &f.template);
        Ok(Self {
            dir,
            header: f.header,
            body,
            params: f.params,
            tail: f.tail,
        })
    }

    /// The bytes.
    pub fn encode(&self) -> Vec<u8> {
        Frame {
            header: self.header,
            template: self.body.encode(),
            params: self.params.clone(),
            tail: self.tail.clone(),
        }
        .encode()
    }

    /// The server.
    pub fn server(&self) -> Option<ServerId> {
        ServerId::from_id(self.header.server_id)
    }

    /// A short name: the server and the request or reply.
    pub fn name(&self) -> String {
        let id = self.header.req_rep_id;
        let table: &[(&str, u16)] = match self.body {
            Body::Common(_) => common::id::ALL,
            Body::Signon(_) => signon::id::ALL,
            Body::Central(_) => central::id::ALL,
            Body::RemoteCommand(_) => rmtcmd::id::ALL,
            Body::Database(_) => database::id::ALL,
            Body::DataQueue(_) => dtaq::id::ALL,
            Body::File(_) => file::id::ALL,
            Body::NetPrint(_) => netprt::id::ALL,
            Body::Unknown(_) => &[],
        };
        let server = self.server().map_or("unknown", ServerId::name);
        let lookup = if matches!(self.body, Body::NetPrint(_)) {
            id & 0x7FFF
        } else {
            id
        };
        match table.iter().find(|(_, v)| *v == lookup) {
            Some((n, _)) => format!("{server}.{n}"),
            None => format!("{server}.{id:04X}"),
        }
    }

    /// The first parameter with code point `cp`.
    pub fn param(&self, cp: u16) -> Option<&Param> {
        self.params.iter().find(|p| p.cp == cp)
    }
}

/// Decode one whole frame.
pub fn decode(dir: Dir, b: &[u8]) -> Result<Message, Error> {
    Message::decode(dir, b)
}

/// Encode a message.
pub fn encode(m: &Message) -> Vec<u8> {
    m.encode()
}
