// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! TCP/IP over the link: one request (`tcp`, tag 690) carries a
//! [`TcpOp`] and comes back as a [`TcpReply`]. The storage service
//! holds the configuration (`QUSRSYS` files) and the run-time state
//! (`tcpcfg::State`); the job asks it to start and stop TCP/IP, reads
//! the panels' data, and the sockets layer reads the resolver's
//! configuration, the port restrictions and the services table.

use ipc::{wire_enum, wire_struct};
use tcpcfg::model::port::PortRestriction;
use tcpcfg::model::tables::ServicesTable;
use tcpcfg::msg::Note;
use tcpcfg::resolver::HostConfig;
use tcpcfg::state::Snapshot;

/// One TCP/IP request.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)] // the fields are named as the commands name them
pub enum TcpOp {
    /// `STRTCP`: the configuration applied; the reply names the
    /// autostart servers when `strsvr`.
    Start { strsvr: bool, strifc: bool, strip6: bool },
    /// `ENDTCP`: the reply names the servers to end first when `endsvr`
    /// (`ending` true asks for the list only, before the stack ends).
    End { immediate: bool, endsvr: bool, ending: bool },
    /// `QTCPWRK`'s heartbeat: line and carrier changes.
    Tick,
    /// The status of TCP/IP, every interface and route.
    Snapshot,
    /// A panel's variables and lists (`IFC`, `RTE`, `PORT`, `HTE`,
    /// `SRV`, `PCL`, `NET`, and the displays `IFC5 key`, `RTE5 key`,
    /// `HTE5 addr`).
    Panel { what: String, args: Vec<String> },
    /// The resolver's configuration.
    HostConfig,
    /// The port restrictions.
    PortRules,
    /// The services table.
    Services,
    /// Every server's autostart and attributes.
    Servers,
    /// Change one server's autostart (when given) and attributes.
    SetServer { server: String, autostart: Option<bool>, bag: Vec<(String, String)> },
    /// What the kernel has against the configuration (`svc.tcp`).
    Report { what: String },
}

wire_enum!(TcpOp {
    0 => Start { strsvr, strifc, strip6 },
    1 => End { immediate, endsvr, ending },
    2 => Tick,
    3 => Snapshot,
    4 => Panel { what, args },
    5 => HostConfig,
    6 => PortRules,
    7 => Services,
    8 => Servers,
    9 => SetServer { server, autostart, bag },
    10 => Report { what },
});

/// A panel's data: variables and named lists of entries.
pub type PanelData = (Vec<(String, String)>, Vec<(String, Vec<Vec<(String, String)>>)>);

/// One server's autostart and attributes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerInfo {
    /// `*TELNET`, `*FTP`, a user-defined `*MYSVR`.
    pub server: String,
    /// Started by `STRTCP`.
    pub autostart: bool,
    /// The server's own attributes.
    pub bag: Vec<(String, String)>,
}

wire_struct!(ServerInfo { server, autostart, bag });

/// The answer to a [`TcpOp`].
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(missing_docs)] // one variant per kind of answer
pub enum TcpReply {
    /// Done: the messages, and the servers the job is to start or end.
    Done { notes: Vec<Note>, servers: Vec<String> },
    /// Refused: the escape.
    Failed { note: Note },
    Snapshot { snap: Snapshot },
    Panel { data: PanelData },
    HostConfig { cfg: HostConfig },
    PortRules { rules: Vec<PortRestriction> },
    Services { table: ServicesTable },
    Servers { list: Vec<ServerInfo> },
    Lines { lines: Vec<String> },
}

wire_enum!(TcpReply {
    0 => Done { notes, servers },
    1 => Failed { note },
    2 => Snapshot { snap },
    3 => Panel { data },
    4 => HostConfig { cfg },
    5 => PortRules { rules },
    6 => Services { table },
    7 => Servers { list },
    8 => Lines { lines },
});

impl TcpReply {
    /// A refusal from an error.
    pub fn failed(e: &tcpcfg::Error) -> Self {
        TcpReply::Failed {
            note: Note::diag(&e.msgid, e.text.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        for op in [
            TcpOp::Start { strsvr: true, strifc: false, strip6: true },
            TcpOp::End { immediate: true, endsvr: true, ending: false },
            TcpOp::Panel { what: "IFC".into(), args: vec!["x".into()] },
            TcpOp::SetServer { server: "*FTP".into(), autostart: Some(false), bag: vec![("PORT".into(), "21".into())] },
        ] {
            assert_eq!(ipc::decode::<TcpOp>(&ipc::encode(&op)).unwrap(), op);
        }
        let r = TcpReply::Done {
            notes: vec![Note::info("TCP1A00", "started")],
            servers: vec!["*TELNET".into()],
        };
        assert_eq!(ipc::decode::<TcpReply>(&ipc::encode(&r)).unwrap(), r);
    }
}
