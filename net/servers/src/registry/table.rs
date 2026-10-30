// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The registry's entries. Job names, users and subsystems are the
//! oracle's (pub400, V7R5M0, `docs/NET_OPEN_QUESTIONS.md` "Server jobs
//! and subsystems") except `QTLPD00nnn`, which no oracle runs (IBM's
//! documentation names it; it runs as `QTMPLPD`, the profile LPD
//! spooled files belong to).

use super::{JobKind, JobSpec, Port, Readiness, ServerSpec};

const fn job(name: &'static str, user: &'static str, subsystem: &'static str, kind: JobKind, role: &'static str) -> JobSpec {
    JobSpec {
        name,
        user,
        subsystem,
        kind,
        role,
    }
}

const fn tcp(port: u16, tls: bool) -> Port {
    Port {
        port,
        protocol: "tcp",
        tls,
    }
}

/// A server this phase does not run: no jobs, no ports.
const fn unplanned(special: &'static str, name: &'static str) -> ServerSpec {
    ServerSpec {
        special,
        name,
        jobs: &[],
        ports: &[],
        attrs_cmd: None,
        app_id: None,
        exit_points: &[],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::NotPlanned,
    }
}

/// Every server, in `STRTCPSVR`'s order.
pub static SPECS: &[ServerSpec] = &[
    ServerSpec {
        special: "*TELNET",
        name: "TELNET",
        jobs: &[
            job("QTVTELNET", "QTCP", "QSYSWRK", JobKind::Listener, "TELNET"),
            job("QTVDEVICE", "QTCP", "QSYSWRK", JobKind::Worker, ""),
        ],
        ports: &[tcp(23, false), tcp(992, true)],
        attrs_cmd: Some("CHGTELNA"),
        app_id: Some("QIBM_QTV_TELNET_SERVER"),
        exit_points: &["QIBM_QTG_DEVINIT", "QIBM_QTG_DEVTERM"],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Built,
    },
    ServerSpec {
        special: "*FTP",
        name: "FTP",
        jobs: &[
            job("QTFTP00nnn", "QTCP", "QSYSWRK", JobKind::Listener, "FTP"),
            job("QTFTP00nnn", "QTCP", "QSYSWRK", JobKind::Worker, "FTPSESS"),
        ],
        ports: &[tcp(21, false), tcp(990, true)],
        attrs_cmd: Some("CHGFTPA"),
        app_id: Some("QIBM_QTMF_FTP_SERVER"),
        exit_points: &["QIBM_QTMF_SERVER_REQ", "QIBM_QTMF_SVR_LOGON"],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Built,
    },
    unplanned("*SMTP", "SMTP"),
    ServerSpec {
        special: "*LPD",
        name: "LPD",
        jobs: &[job("QTLPD00nnn", "QTMPLPD", "QSYSWRK", JobKind::Listener, "LPD")],
        ports: &[tcp(515, false)],
        attrs_cmd: Some("CHGLPDA"),
        app_id: Some("QIBM_QSO_LPD_SERVER"),
        exit_points: &[],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Built,
    },
    ServerSpec {
        special: "*HTTP",
        name: "HTTP",
        jobs: &[job("ADMIN", "QTMHHTTP", "QHTTPSVR", JobKind::Listener, "HTTPADM")],
        ports: &[tcp(2001, false), tcp(2005, true)],
        attrs_cmd: None,
        app_id: Some("QIBM_HTTP_SERVER_ADMIN"),
        exit_points: &[],
        start_subsystem: true,
        instances: &["*ADMIN"],
        readiness: Readiness::Built,
    },
    unplanned("*SNMP", "SNMP"),
    unplanned("*ROUTED", "ROUTED"),
    unplanned("*BOOTP", "BOOTP"),
    unplanned("*TFTP", "TFTP"),
    unplanned("*REXEC", "REXEC"),
    unplanned("*DHCP", "DHCP"),
    unplanned("*DNS", "DNS"),
    unplanned("*POP", "POP"),
    ServerSpec {
        special: "*DDM",
        name: "DDM",
        jobs: &[
            job("QRWTLSTN", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QRWTSRVR", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
        ],
        ports: &[tcp(446, false), tcp(447, false), tcp(448, true)],
        attrs_cmd: Some("CHGDDMTCPA"),
        app_id: Some("QIBM_QRW_SERVER_DDM_DRDA"),
        exit_points: &[],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Blocked(
            "the DDM server's SQL, file and password backends over the storage service",
        ),
    },
    ServerSpec {
        special: "*NTP",
        name: "NTP",
        jobs: &[job("QTOTNTP", "QNTP", "QSYSWRK", JobKind::Listener, "NTP")],
        ports: &[Port {
            port: 123,
            protocol: "udp",
            tls: false,
        }],
        attrs_cmd: Some("CHGNTPA"),
        app_id: None,
        exit_points: &[],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Built,
    },
    ServerSpec {
        special: "*SSHD",
        name: "SSHD",
        jobs: &[
            job("QP0ZSPWT", "QUSER", "QUSRWRK", JobKind::Listener, ""),
            job("QP0ZSPWT", "QUSER", "QUSRWRK", JobKind::Worker, ""),
        ],
        ports: &[tcp(22, false)],
        attrs_cmd: None,
        app_id: None,
        exit_points: &[],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Slot("the SSH server (net/sshd)"),
    },
    ServerSpec {
        special: "*HOSTSVR",
        name: "HOSTSVR",
        jobs: &[
            job("QZSOSMAPD", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QZSOSGND", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QZSCSRVSD", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QZDASRVSD", "QUSER", "QSERVER", JobKind::Listener, ""),
            job("QZHQSRVD", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QPWFSERVSD", "QUSER", "QSERVER", JobKind::Listener, ""),
            job("QNPSERVD", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QZRCSRVSD", "QUSER", "QSYSWRK", JobKind::Listener, ""),
            job("QZSOSIGN", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
            job("QZSCSRVS", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
            job("QZDASOINIT", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
            job("QZHQSSRV", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
            job("QPWFSERVSO", "QUSER", "QSERVER", JobKind::Prestart, ""),
            job("QNPSERVS", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
            job("QZRCSRVS", "QUSER", "QUSRWRK", JobKind::Prestart, ""),
        ],
        ports: &[
            tcp(449, false),
            tcp(8470, false),
            tcp(8471, false),
            tcp(8472, false),
            tcp(8473, false),
            tcp(8474, false),
            tcp(8475, false),
            tcp(8476, false),
            tcp(9470, true),
            tcp(9471, true),
            tcp(9472, true),
            tcp(9473, true),
            tcp(9474, true),
            tcp(9475, true),
            tcp(9476, true),
        ],
        attrs_cmd: None,
        app_id: None,
        exit_points: &[],
        start_subsystem: false,
        instances: &[],
        readiness: Readiness::Slot("the host servers (net/hostsvr, STRHOSTSVR)"),
    },
];
