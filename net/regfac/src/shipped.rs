// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The exit points QS/400 ships, registered when the facility is first
//! made: IBM's names, formats, maximums, flags and preprocessing
//! programs (`tools/conformance/oracle/net/pub400/exit-points.txt`), with
//! Quartz descriptions. Only the servers of this release are here.

use crate::model::{Desc, ExitPoint, Prep};

/// One shipped point: name, format, allow deregistration, allow
/// change, maximum programs, preprocessing library and programs (add,
/// remove), description.
struct Row {
    name: &'static str,
    format: &'static str,
    dereg: bool,
    change: bool,
    max: Option<u32>,
    prep: Option<(&'static str, &'static str, &'static str)>,
    text: &'static str,
}

const fn row(
    name: &'static str,
    format: &'static str,
    (dereg, change): (bool, bool),
    max: Option<u32>,
    prep: Option<(&'static str, &'static str, &'static str)>,
    text: &'static str,
) -> Row {
    Row {
        name,
        format,
        dereg,
        change,
        max,
        prep,
        text,
    }
}

/// The host servers' points: no deregistration, change allowed, one
/// program.
const HOST: (bool, bool) = (false, true);
/// The TCP/IP application points: deregistration allowed.
const TCP: (bool, bool) = (true, false);
const FTP_PREP: Option<(&str, &str, &str)> = Some(("QTCP", "QTMFADDP", "QTMFRMVP"));
const REXEC_PREP: Option<(&str, &str, &str)> = Some(("QTCP", "QTMXADDP", "QTMXRMVP"));

static ROWS: &[Row] = &[
    // Host servers.
    row(
        "QIBM_QZSO_SIGNONSRV",
        "ZSOY0100",
        HOST,
        Some(1),
        None,
        "Sign-on server requests",
    ),
    row(
        "QIBM_QZSC_LM",
        "ZSCL0100",
        HOST,
        Some(1),
        None,
        "Central server license requests",
    ),
    row(
        "QIBM_QZSC_NLS",
        "ZSCN0100",
        HOST,
        Some(1),
        None,
        "Central server conversion tables",
    ),
    row(
        "QIBM_QZSC_SM",
        "ZSCS0100",
        HOST,
        Some(1),
        None,
        "Central server client registration",
    ),
    row(
        "QIBM_QZDA_INIT",
        "ZDAI0100",
        HOST,
        Some(1),
        None,
        "Database server connections",
    ),
    row(
        "QIBM_QZDA_NDB1",
        "ZDAD0100",
        HOST,
        Some(1),
        None,
        "Database server file requests",
    ),
    row(
        "QIBM_QZDA_NDB1",
        "ZDAD0200",
        HOST,
        Some(1),
        None,
        "Database server library lists",
    ),
    row(
        "QIBM_QZDA_ROI1",
        "ZDAR0100",
        HOST,
        Some(1),
        None,
        "Database server catalog requests",
    ),
    row(
        "QIBM_QZDA_ROI1",
        "ZDAR0200",
        HOST,
        Some(1),
        None,
        "Database server key requests",
    ),
    row(
        "QIBM_QZDA_SQL1",
        "ZDAQ0100",
        HOST,
        Some(1),
        None,
        "Database server SQL, short text",
    ),
    row(
        "QIBM_QZDA_SQL2",
        "ZDAQ0200",
        HOST,
        Some(1),
        None,
        "Database server SQL, full text",
    ),
    row(
        "QIBM_QZHQ_DATA_QUEUE",
        "ZHQ00100",
        HOST,
        Some(1),
        None,
        "Data queue server requests",
    ),
    row(
        "QIBM_QPWFS_FILE_SERV",
        "PWFS0100",
        (false, false),
        Some(1),
        Some(("QSYS", "QUSEPAUT", "QUSEPAUT")),
        "File server requests",
    ),
    row(
        "QIBM_QPWFS_FILE_SERV",
        "PWFS0200",
        (false, false),
        Some(1),
        Some(("QSYS", "QUSEPAUT", "QUSEPAUT")),
        "File server requests, two names",
    ),
    row(
        "QIBM_QNPS_ENTRY",
        "ENTR0100",
        HOST,
        None,
        None,
        "Network print server sessions",
    ),
    row(
        "QIBM_QNPS_SPLF",
        "SPLF0100",
        HOST,
        None,
        None,
        "Network print server spooled files",
    ),
    row(
        "QIBM_QZRC_RMT",
        "CZRC0100",
        HOST,
        Some(1),
        None,
        "Remote commands and program calls",
    ),
    // FTP, REXEC and TFTP.
    row(
        "QIBM_QTMF_CLIENT_REQ",
        "VLRQ0100",
        TCP,
        Some(1),
        FTP_PREP,
        "FTP client requests",
    ),
    row(
        "QIBM_QTMF_SERVER_REQ",
        "VLRQ0100",
        TCP,
        Some(1),
        FTP_PREP,
        "FTP server requests",
    ),
    row(
        "QIBM_QTMF_SVR_LOGON",
        "TCPL0100",
        TCP,
        Some(1),
        FTP_PREP,
        "FTP server sign-on",
    ),
    row(
        "QIBM_QTMF_SVR_LOGON",
        "TCPL0200",
        TCP,
        Some(1),
        FTP_PREP,
        "FTP server sign-on, home directory",
    ),
    row(
        "QIBM_QTMF_SVR_LOGON",
        "TCPL0300",
        TCP,
        Some(1),
        FTP_PREP,
        "FTP server sign-on, long passwords",
    ),
    row(
        "QIBM_QTMX_SERVER_REQ",
        "VLRQ0100",
        TCP,
        Some(1),
        REXEC_PREP,
        "REXEC server requests",
    ),
    row(
        "QIBM_QTMX_SVR_LOGON",
        "TCPL0100",
        TCP,
        Some(1),
        REXEC_PREP,
        "REXEC server sign-on",
    ),
    row(
        "QIBM_QTMX_SVR_LOGON",
        "TCPL0300",
        TCP,
        Some(1),
        REXEC_PREP,
        "REXEC server sign-on, long passwords",
    ),
    row(
        "QIBM_QTOD_SERVER_REQ",
        "VLRQ0100",
        (true, true),
        Some(1),
        Some(("QSYS", "QTODADDP", "QTODRMVP")),
        "TFTP server requests",
    ),
    // Telnet.
    row(
        "QIBM_QTG_DEVINIT",
        "INIT0100",
        (false, false),
        Some(1),
        Some(("QSYS", "QTVADDP", "QTVRMVP")),
        "Telnet session start",
    ),
    row(
        "QIBM_QTG_DEVTERM",
        "TERM0100",
        (false, false),
        Some(1),
        Some(("QSYS", "QTVADDP", "QTVRMVP")),
        "Telnet session end",
    ),
];

/// The shipped points, registered and without programs.
#[must_use]
pub fn points() -> Vec<ExitPoint> {
    ROWS.iter()
        .map(|r| ExitPoint {
            name: r.name.into(),
            format: r.format.into(),
            registered: true,
            allow_dereg: r.dereg,
            allow_change: r.change,
            max: r.max,
            prep_add: r.prep.map(|(l, a, _)| Prep::new(l, a, "ADDP0100")),
            prep_rmv: r.prep.map(|(l, _, m)| Prep::new(l, m, "RMVP0100")),
            prep_rtv: None,
            desc: Desc::text(r.text),
            programs: Vec::new(),
        })
        .collect()
}
