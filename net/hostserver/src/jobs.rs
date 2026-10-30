// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The host servers' jobs, as IBM i names them: a daemon per server
//! that listens on its ports and hands each connection to one of the
//! server's prestart jobs. The names, users and subsystems are the
//! primary oracle's (`tools/conformance/oracle/net/pub400/`).

use hostsvr::server::ServerId;

/// A server's daemon and prestart jobs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Daemon {
    /// `STRHOSTSVR SERVER` value (`*SIGNON`, ...).
    pub keyword: &'static str,
    /// The daemon job's name.
    pub name: &'static str,
    /// Its subsystem.
    pub subsystem: &'static str,
    /// The server; `None` for the port mapper.
    pub server: Option<ServerId>,
    /// The plain port.
    pub port: u16,
    /// The TLS port.
    pub tls_port: Option<u16>,
    /// The prestart job program for plain connections.
    pub prestart: &'static str,
    /// The prestart job program for TLS connections.
    pub prestart_tls: &'static str,
}

/// Every daemon, in start order (the port mapper first).
pub const DAEMONS: &[Daemon] = &[
    Daemon {
        keyword: "*SVRMAP",
        name: "QZSOSMAPD",
        subsystem: "QSYSWRK",
        server: None,
        port: 449,
        tls_port: None,
        prestart: "",
        prestart_tls: "",
    },
    Daemon {
        keyword: "*SIGNON",
        name: "QZSOSGND",
        subsystem: "QSYSWRK",
        server: Some(ServerId::Signon),
        port: 8476,
        tls_port: Some(9476),
        prestart: "QZSOSIGN",
        prestart_tls: "QZSOSIGN",
    },
    Daemon {
        keyword: "*CENTRAL",
        name: "QZSCSRVSD",
        subsystem: "QSYSWRK",
        server: Some(ServerId::Central),
        port: 8470,
        tls_port: Some(9470),
        prestart: "QZSCSRVS",
        prestart_tls: "QZSCSRVS",
    },
    Daemon {
        keyword: "*DATABASE",
        name: "QZDASRVSD",
        subsystem: "QSERVER",
        server: Some(ServerId::Database),
        port: 8471,
        tls_port: Some(9471),
        prestart: "QZDASOINIT",
        prestart_tls: "QZDASSINIT",
    },
    Daemon {
        keyword: "*DTAQ",
        name: "QZHQSRVD",
        subsystem: "QSYSWRK",
        server: Some(ServerId::DataQueue),
        port: 8472,
        tls_port: Some(9472),
        prestart: "QZHQSSRV",
        prestart_tls: "QZHQSSRV",
    },
    Daemon {
        keyword: "*FILE",
        name: "QPWFSERVSD",
        subsystem: "QSERVER",
        server: Some(ServerId::File),
        port: 8473,
        tls_port: Some(9473),
        prestart: "QPWFSERVSO",
        prestart_tls: "QPWFSERVSS",
    },
    Daemon {
        keyword: "*NETPRT",
        name: "QNPSERVD",
        subsystem: "QSYSWRK",
        server: Some(ServerId::NetPrint),
        port: 8474,
        tls_port: Some(9474),
        prestart: "QNPSERVS",
        prestart_tls: "QNPSERVS",
    },
    Daemon {
        keyword: "*RMTCMD",
        name: "QZRCSRVSD",
        subsystem: "QSYSWRK",
        server: Some(ServerId::RemoteCommand),
        port: 8475,
        tls_port: Some(9475),
        prestart: "QZRCSRVS",
        prestart_tls: "QZRCSRVS",
    },
];

/// The daemon for a `STRHOSTSVR` keyword.
pub fn daemon(keyword: &str) -> Option<&'static Daemon> {
    DAEMONS.iter().find(|d| d.keyword.eq_ignore_ascii_case(keyword))
}

/// The server a prestart job program serves.
pub fn server_of_prestart(program: &str) -> Option<ServerId> {
    DAEMONS
        .iter()
        .find(|d| d.prestart == program || d.prestart_tls == program)
        .and_then(|d| d.server)
}

/// The DCM application ID of a server's TLS port.
pub fn app_id(s: ServerId) -> &'static str {
    match s {
        ServerId::Central => "QIBM_OS400_QZBS_SVR_CENTRAL",
        ServerId::Database => "QIBM_OS400_QZBS_SVR_DATABASE",
        ServerId::DataQueue => "QIBM_OS400_QZBS_SVR_DTAQ",
        ServerId::File => "QIBM_OS400_QZBS_SVR_FILE",
        ServerId::NetPrint => "QIBM_OS400_QZBS_SVR_NETPRT",
        ServerId::RemoteCommand => "QIBM_OS400_QZBS_SVR_RMTCMD",
        ServerId::Signon => "QIBM_OS400_QZBS_SVR_SIGNON",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(daemon("*rmtcmd").unwrap().name, "QZRCSRVSD");
        assert_eq!(server_of_prestart("QZDASSINIT"), Some(ServerId::Database));
        assert_eq!(server_of_prestart("QZSOSMAPD"), None);
        for d in DAEMONS.iter().filter(|d| d.server.is_some()) {
            assert_eq!(d.server.unwrap().port(), d.port);
        }
    }
}
