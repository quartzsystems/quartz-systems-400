// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host servers: header server IDs, service names, ports.

/// A host server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ServerId {
    /// Central (license, NLS, system management).
    Central,
    /// File (the IFS).
    File,
    /// Network print.
    NetPrint,
    /// Database (SQL and native database).
    Database,
    /// Data queue.
    DataQueue,
    /// Remote command and distributed program call.
    RemoteCommand,
    /// Sign-on.
    Signon,
}

/// Every server, in port order.
pub const ALL: [ServerId; 7] = [
    ServerId::Central,
    ServerId::Database,
    ServerId::DataQueue,
    ServerId::File,
    ServerId::NetPrint,
    ServerId::RemoteCommand,
    ServerId::Signon,
];

/// The port mapper's port (`as-svrmap`).
pub const SVRMAP_PORT: u16 = 449;

impl ServerId {
    /// The header's server ID.
    pub fn id(self) -> u16 {
        match self {
            Self::Central => 0xE000,
            Self::File => 0xE002,
            Self::NetPrint => 0xE003,
            Self::Database => 0xE004,
            Self::DataQueue => 0xE007,
            Self::RemoteCommand => 0xE008,
            Self::Signon => 0xE009,
        }
    }

    /// The server with a header server ID.
    pub fn from_id(id: u16) -> Option<Self> {
        ALL.into_iter().find(|s| s.id() == id)
    }

    /// The service name the port mapper knows it by.
    pub fn service(self) -> &'static str {
        match self {
            Self::Central => "as-central",
            Self::File => "as-file",
            Self::NetPrint => "as-netprt",
            Self::Database => "as-database",
            Self::DataQueue => "as-dtaq",
            Self::RemoteCommand => "as-rmtcmd",
            Self::Signon => "as-signon",
        }
    }

    /// The well-known plain port.
    pub fn port(self) -> u16 {
        match self {
            Self::Central => 8470,
            Self::Database => 8471,
            Self::DataQueue => 8472,
            Self::File => 8473,
            Self::NetPrint => 8474,
            Self::RemoteCommand => 8475,
            Self::Signon => 8476,
        }
    }

    /// The well-known TLS port.
    pub fn tls_port(self) -> u16 {
        self.port() + 1000
    }

    /// The short name trace folders use.
    pub fn name(self) -> &'static str {
        match self {
            Self::Central => "central",
            Self::File => "file",
            Self::NetPrint => "netprt",
            Self::Database => "database",
            Self::DataQueue => "dtaq",
            Self::RemoteCommand => "rmtcmd",
            Self::Signon => "signon",
        }
    }

    /// The server on a well-known plain port.
    pub fn from_port(port: u16) -> Option<Self> {
        ALL.into_iter().find(|s| s.port() == port)
    }

    /// The server with a short name.
    pub fn from_name(name: &str) -> Option<Self> {
        ALL.into_iter().find(|s| s.name() == name)
    }
}
