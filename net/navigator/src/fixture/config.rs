// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Sample system values, disks, hardware resources and time.

use std::sync::Mutex;

use qsweb::msg::{Kind, Message};

use super::text;
use super::work::parse;
use crate::env::config::{Allowed, Asp, Config, DiskUnit, Disks, Resource, SysVal, Time};
use crate::util::secs;

/// The sample system's configuration, changeable by tests.
pub struct FixtureConfig {
    /// The system values.
    pub sysvals: Mutex<Vec<SysVal>>,
    /// The NTP client's servers.
    pub ntp: Mutex<Vec<String>>,
    /// Commands run, as typed.
    pub ran: Mutex<Vec<String>>,
}

impl Default for FixtureConfig {
    fn default() -> Self {
        Self {
            sysvals: Mutex::new(sysvals()),
            ntp: Mutex::new(vec!["10.0.10.1".into(), "time.quartz.example".into()]),
            ran: Mutex::new(Vec::new()),
        }
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(ToString::to_string).collect()
}

fn range(min: i64, max: i64, sp: &[&str]) -> Allowed {
    Allowed::Range(min, max, strs(sp))
}

fn values(v: &[&str]) -> Allowed {
    Allowed::Values(strs(v))
}

/// Name, category, value, shipped value, description, allowed.
type Row = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Allowed,
);

#[allow(clippy::too_many_lines)]
fn rows() -> Vec<Row> {
    let yn = || values(&["0", "1"]);
    vec![
        (
            "QSECURITY",
            "security",
            "40",
            "40",
            "System security level",
            values(&["20", "30", "40", "50"]),
        ),
        (
            "QPWDLVL",
            "security",
            "3",
            "3",
            "Password level",
            values(&["0", "1", "2", "3", "4"]),
        ),
        (
            "QPWDEXPITV",
            "security",
            "90",
            "*NOMAX",
            "Password expiration interval (days)",
            range(1, 366, &["*NOMAX"]),
        ),
        (
            "QPWDMINLEN",
            "security",
            "8",
            "6",
            "Minimum password length",
            range(1, 128, &[]),
        ),
        (
            "QPWDMAXLEN",
            "security",
            "128",
            "128",
            "Maximum password length",
            range(1, 128, &[]),
        ),
        (
            "QPWDRQDDIF",
            "security",
            "1",
            "0",
            "Duplicate password control",
            range(0, 8, &[]),
        ),
        (
            "QPWDRULES",
            "security",
            "*PWDSYSVAL",
            "*PWDSYSVAL",
            "Password rules",
            Allowed::Any,
        ),
        (
            "QMAXSIGN",
            "security",
            "3",
            "3",
            "Maximum sign-on attempts allowed",
            range(1, 25, &["*NOMAX"]),
        ),
        (
            "QMAXSGNACN",
            "security",
            "3",
            "3",
            "Action when sign-on attempts reached (disable profile and device)",
            values(&["1", "2", "3"]),
        ),
        (
            "QINACTITV",
            "security",
            "60",
            "*NONE",
            "Inactive job time-out (minutes)",
            range(5, 300, &["*NONE"]),
        ),
        (
            "QINACTMSGQ",
            "security",
            "*ENDJOB",
            "*ENDJOB",
            "Inactive job message queue / action",
            Allowed::Any,
        ),
        (
            "QDSCJOBITV",
            "security",
            "240",
            "240",
            "Disconnected job time-out (minutes)",
            range(5, 1440, &["*NONE"]),
        ),
        (
            "QLMTSECOFR",
            "security",
            "1",
            "1",
            "Limit security officer device access",
            yn(),
        ),
        (
            "QLMTDEVSSN",
            "security",
            "0",
            "0",
            "Limit device sessions",
            yn(),
        ),
        (
            "QALWOBJRST",
            "security",
            "*ALWPGMADP *ALWPTF",
            "*ALL",
            "Allow object restore options",
            Allowed::List(
                strs(&[
                    "*ALL",
                    "*NONE",
                    "*ALWSYSSTT",
                    "*ALWPGMADP",
                    "*ALWPTF",
                    "*ALWSETUID",
                    "*ALWVLDERR",
                ]),
                6,
            ),
        ),
        (
            "QVFYOBJRST",
            "security",
            "3",
            "1",
            "Verify object on restore",
            values(&["1", "2", "3", "4", "5"]),
        ),
        (
            "QFRCCVNRST",
            "security",
            "1",
            "1",
            "Force conversion on restore",
            values(&["0", "1", "2", "3", "4", "5", "6", "7"]),
        ),
        (
            "QAUDCTL",
            "security",
            "*AUDLVL *OBJAUD *NOQTEMP",
            "*NONE",
            "Auditing control",
            Allowed::List(strs(&["*NONE", "*AUDLVL", "*OBJAUD", "*NOQTEMP"]), 3),
        ),
        (
            "QAUDLVL",
            "security",
            "*AUTFAIL *CREATE *DELETE *SECURITY *SERVICE *PGMFAIL",
            "*NONE",
            "Security auditing level",
            Allowed::Any,
        ),
        (
            "QAUDENDACN",
            "security",
            "*NOTIFY",
            "*NOTIFY",
            "Auditing end action",
            values(&["*NOTIFY", "*PWRDWNSYS"]),
        ),
        (
            "QAUDFRCLVL",
            "security",
            "*SYS",
            "*SYS",
            "Force auditing data",
            range(1, 100, &["*SYS"]),
        ),
        (
            "QCRTAUT",
            "security",
            "*CHANGE",
            "*CHANGE",
            "Create default public authority",
            values(&["*CHANGE", "*USE", "*ALL", "*EXCLUDE"]),
        ),
        (
            "QUSEADPAUT",
            "security",
            "*NONE",
            "*NONE",
            "Use adopted authority",
            Allowed::Any,
        ),
        (
            "QRETSVRSEC",
            "security",
            "1",
            "0",
            "Retain server security data",
            yn(),
        ),
        (
            "QSSLCSL",
            "security",
            "*OPSYS",
            "*OPSYS",
            "TLS cipher specification list",
            Allowed::Any,
        ),
        (
            "QSSLPCL",
            "security",
            "*TLSV1.3 *TLSV1.2",
            "*OPSYS",
            "TLS protocols",
            Allowed::List(strs(&["*OPSYS", "*TLSV1.3", "*TLSV1.2"]), 2),
        ),
        (
            "QSHRMEMCTL",
            "security",
            "1",
            "1",
            "Shared memory control",
            yn(),
        ),
        (
            "QALWUSRDMN",
            "security",
            "*ALL",
            "*ALL",
            "Allow user domain objects in libraries",
            Allowed::Any,
        ),
        // Date and time.
        (
            "QTIMZON",
            "datetime",
            "QN0500EST3",
            "Q0000UTC",
            "Time zone",
            Allowed::Any,
        ),
        (
            "QDATFMT",
            "datetime",
            "MDY",
            "MDY",
            "Date format",
            values(&["MDY", "DMY", "YMD", "JUL"]),
        ),
        (
            "QDATSEP",
            "datetime",
            "/",
            "/",
            "Date separator",
            values(&["/", "-", ".", ","]),
        ),
        (
            "QTIMSEP",
            "datetime",
            ":",
            ":",
            "Time separator",
            values(&[":", ".", ","]),
        ),
        (
            "QTIMADJ",
            "datetime",
            "QS400/QTOTNTP",
            "*NONE",
            "Time adjustment software",
            Allowed::Any,
        ),
        (
            "QLEAPADJ",
            "datetime",
            "0",
            "0",
            "Leap year adjustment",
            range(0, 3, &[]),
        ),
        // Jobs.
        (
            "QMAXJOB",
            "jobs",
            "163520",
            "163520",
            "Maximum jobs allowed",
            range(32_000, 970_000, &[]),
        ),
        (
            "QACTJOB",
            "jobs",
            "400",
            "200",
            "Initial number of active jobs",
            range(1, 32_767, &[]),
        ),
        (
            "QTOTJOB",
            "jobs",
            "1500",
            "30",
            "Initial total number of jobs",
            range(1, 32_767, &[]),
        ),
        (
            "QMAXSPLF",
            "jobs",
            "9999",
            "9999",
            "Maximum spooled files per job",
            range(9999, 999_999, &[]),
        ),
        // Storage.
        (
            "QSTGLOWLMT",
            "storage",
            "5.0000",
            "5.0000",
            "Auxiliary storage lower limit (%)",
            Allowed::Any,
        ),
        (
            "QSTGLOWACN",
            "storage",
            "*MSG",
            "*MSG",
            "Auxiliary storage lower limit action",
            values(&["*MSG", "*CRITMSG", "*REGFAC", "*ENDSYS", "*PWRDWNSYS"]),
        ),
        // Messages and logging.
        (
            "QHSTLOGSIZ",
            "msgs",
            "5000",
            "5000",
            "History log file size (records)",
            range(1, 10_000_000, &["*DAILY"]),
        ),
        (
            "QLOGOUTPUT",
            "msgs",
            "*JOBEND",
            "*JOBEND",
            "Job log output",
            values(&["*SYSVAL", "*JOBLOGSVR", "*JOBEND", "*PND"]),
        ),
        // Devices.
        (
            "QAUTOVRT",
            "devices",
            "200",
            "0",
            "Automatic virtual devices",
            range(0, 32_500, &["*NOMAX", "*REGFAC"]),
        ),
        (
            "QDEVNAMING",
            "devices",
            "*NORMAL",
            "*NORMAL",
            "Device naming",
            values(&["*NORMAL", "*S36", "*DEVADR"]),
        ),
        // Library list.
        (
            "QSYSLIBL",
            "libl",
            "QSYS QSYS2 QHLPSYS QUSRSYS",
            "QSYS QSYS2 QHLPSYS QUSRSYS",
            "System part of the library list",
            Allowed::Any,
        ),
        (
            "QUSRLIBL",
            "libl",
            "QGPL QTEMP",
            "QGPL QTEMP",
            "User part of the library list",
            Allowed::Any,
        ),
        // Performance.
        (
            "QPFRADJ",
            "perf",
            "2",
            "2",
            "Automatic performance adjustment",
            values(&["0", "1", "2", "3"]),
        ),
        (
            "QDYNPTYSCD",
            "perf",
            "1",
            "1",
            "Dynamic priority scheduler",
            values(&["0", "1"]),
        ),
        // Power.
        (
            "QPWRDWNLMT",
            "power",
            "900",
            "900",
            "Maximum time for an immediate power down (seconds)",
            range(0, 32_767, &[]),
        ),
        (
            "QUPSDLYTIM",
            "power",
            "*CALC",
            "*CALC",
            "Uninterruptible power supply delay",
            Allowed::Any,
        ),
        // Save and restore.
        (
            "QSAVACCPTH",
            "save",
            "1",
            "1",
            "Save access paths",
            values(&["0", "1"]),
        ),
        // System control.
        (
            "QSTRUPPGM",
            "sysctl",
            "QSYS/QSTRUP",
            "QSYS/QSTRUP",
            "Startup program",
            Allowed::Any,
        ),
        (
            "QCTLSBSD",
            "sysctl",
            "QSYS/QCTL",
            "QSYS/QCTL",
            "Controlling subsystem",
            Allowed::Any,
        ),
        (
            "QIPLTYPE",
            "sysctl",
            "0",
            "0",
            "Type of IPL",
            values(&["0", "1", "2"]),
        ),
    ]
}

fn sysvals() -> Vec<SysVal> {
    rows()
        .into_iter()
        .map(|(name, category, value, shipped, text, allowed)| SysVal {
            name: name.into(),
            category: category.into(),
            value: value.into(),
            shipped: shipped.into(),
            text: text.into(),
            allowed,
            ipl: matches!(
                name,
                "QCTLSBSD" | "QIPLTYPE" | "QMAXJOB" | "QACTJOB" | "QTOTJOB"
            ),
            changed: (name == "QMAXSIGN")
                .then(|| (secs(2026, 8, 14, 9, 12, 0), "QSECOFR".to_string())),
        })
        .collect()
}

const GB10: u64 = (1 << 30) / 10;

#[allow(clippy::too_many_arguments)]
fn unit(
    n: u32,
    kind: &str,
    model: &str,
    cap: u64,
    used: u32,
    prot: &str,
    asp: Option<u32>,
    serial: &str,
) -> DiskUnit {
    DiskUnit {
        unit: n,
        resource: format!("DD{n:03}"),
        kind: kind.into(),
        model: model.into(),
        capacity: cap * GB10,
        used_pct: used,
        protection: prot.into(),
        status: if asp.is_some() {
            "Active"
        } else {
            "Non-configured"
        }
        .into(),
        asp,
        serial: serial.into(),
    }
}

/// The sample system's disks.
pub fn disks() -> Disks {
    let asp = |number, kind: &str, cap: u64, used: u64, pct, prot: &str, overflow| Asp {
        number,
        kind: kind.into(),
        capacity: cap * GB10,
        used: used * GB10,
        used_pct: pct,
        protection: prot.into(),
        overflow,
        threshold: 90,
    };
    let m = |n: u32| {
        if n % 2 == 1 {
            "Mirrored (A)"
        } else {
            "Mirrored (B)"
        }
    };
    let mut units = Vec::new();
    for (n, used, serial) in [
        (1, 674, "Y3B0A1C9"),
        (2, 674, "Y3B0A1D0"),
        (3, 672, "Y3B0A1D4"),
        (4, 672, "Y3B0A1D8"),
        (5, 673, "Y3B0A1E1"),
        (6, 673, "Y3B0A1E6"),
        (7, 673, "Y3B0A1F0"),
        (8, 673, "Y3B0A1F3"),
    ] {
        units.push(unit(n, "6B22", "050", 4656, used, m(n), Some(1), serial));
    }
    units.push(unit(
        9,
        "6B23",
        "072",
        9312,
        442,
        "None",
        Some(2),
        "Y3C1B220",
    ));
    units.push(unit(
        10,
        "6B22",
        "050",
        4656,
        189,
        "Mirrored (A)",
        Some(33),
        "Y3B0A201",
    ));
    units.push(unit(
        11,
        "6B22",
        "050",
        4656,
        189,
        "Mirrored (B)",
        Some(33),
        "Y3B0A202",
    ));
    units.push(unit(
        12,
        "6B22",
        "050",
        4656,
        0,
        "Non-configured",
        None,
        "Y3B0A210",
    ));
    Disks {
        asps: vec![
            asp(1, "System", 18_624, 12_533, 673, "Mirrored", Some(false)),
            asp(2, "User (basic)", 9_312, 4_120, 442, "None", Some(false)),
            asp(
                33,
                "Independent (IASP INVIASP)",
                4_656,
                881,
                189,
                "Mirrored",
                None,
            ),
        ],
        units,
        elapsed: 300,
    }
}

/// Name, depth, type-model, text, status, serial, location, category,
/// configuration descriptions.
type Hw = (
    &'static str,
    u32,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static [(&'static str, &'static str)],
);

const HARDWARE: &[Hw] = &[
    (
        "CEC01",
        0,
        "9Q04-M2",
        "System unit",
        "Operational",
        "10-7A2B9C",
        "Frame 1",
        "",
        &[],
    ),
    (
        "CMB01",
        1,
        "2E2A",
        "Combined function I/O processor",
        "Operational",
        "",
        "U04",
        "Storage",
        &[],
    ),
    (
        "LIN01",
        2,
        "5706",
        "Ethernet port (10 GbE SFP+)",
        "Operational",
        "10-A1B2C3",
        "U07",
        "Communications",
        &[],
    ),
    (
        "CMN01",
        3,
        "5706",
        "Ethernet port - ETHLIN01",
        "Operational",
        "",
        "U10",
        "Communications",
        &[("ETHLIN01", "*LIN")],
    ),
    (
        "CMN02",
        3,
        "5706",
        "Ethernet port - ETHLIN02",
        "Operational",
        "",
        "U10",
        "Communications",
        &[("ETHLIN02", "*LIN")],
    ),
    (
        "LIN02",
        2,
        "5767",
        "Ethernet port (1 GbE)",
        "Operational",
        "10-A1B2D0",
        "U07",
        "Communications",
        &[],
    ),
    (
        "CMN03",
        3,
        "5767",
        "Ethernet port - ETHLIN03",
        "Operational",
        "",
        "U10",
        "Communications",
        &[("ETHLIN03", "*LIN")],
    ),
    (
        "CMN04",
        3,
        "5767",
        "Ethernet port - ETHLIN04",
        "Not connected",
        "",
        "U10",
        "Communications",
        &[("ETHLIN04", "*LIN")],
    ),
    (
        "DC01",
        2,
        "57B8",
        "Disk controller (SAS, mirrored)",
        "Operational",
        "10-D1E2F3",
        "U07",
        "Storage",
        &[],
    ),
    (
        "DD001",
        3,
        "6B22",
        "Disk unit",
        "Operational",
        "Y3B0A1C9",
        "U10",
        "Storage",
        &[],
    ),
    (
        "DD002",
        3,
        "6B22",
        "Disk unit",
        "Operational",
        "Y3B0A1D0",
        "U10",
        "Storage",
        &[],
    ),
    (
        "DD003",
        3,
        "6B22",
        "Disk unit",
        "Operational",
        "Y3B0A1D4",
        "U10",
        "Storage",
        &[],
    ),
    (
        "DD004",
        3,
        "6B22",
        "Disk unit",
        "Operational",
        "Y3B0A1D8",
        "U10",
        "Storage",
        &[],
    ),
    (
        "DD005 - DD012",
        3,
        "6B22/6B23",
        "Disk units (8 more)",
        "Operational",
        "",
        "U10",
        "Storage",
        &[],
    ),
    (
        "DC02",
        2,
        "5774",
        "Fibre Channel adapter (QLE2562)",
        "Operational",
        "BFE1234567",
        "U07",
        "Storage",
        &[],
    ),
    (
        "TAP01",
        3,
        "3580-TD6",
        "Tape unit (virtual tape library QS-HQ-VTL)",
        "Operational",
        "VTL0000001",
        "U10",
        "Storage",
        &[("TAP01", "*DEV")],
    ),
    (
        "TAPMLB01",
        3,
        "3584",
        "Tape media library (virtual)",
        "Operational",
        "VTL0000000",
        "U10",
        "Storage",
        &[("TAPMLB01", "*DEV")],
    ),
    (
        "OPT01",
        2,
        "632C",
        "Optical / image catalog device",
        "Operational",
        "",
        "U07",
        "Storage",
        &[("OPT01", "*DEV")],
    ),
    (
        "CMB02",
        1,
        "2E2B",
        "Combined function I/O processor",
        "Operational",
        "",
        "U04",
        "Workstation",
        &[],
    ),
    (
        "CTL01",
        2,
        "6A59",
        "Workstation controller (virtual)",
        "Operational",
        "",
        "U07",
        "Workstation",
        &[("QPACTL01", "*CTL")],
    ),
    (
        "DSP01",
        3,
        "3179",
        "Console display",
        "Operational",
        "",
        "U10",
        "Workstation",
        &[("DSP01", "*DEV")],
    ),
    (
        "SP01",
        1,
        "SP",
        "Service processor",
        "Operational",
        "",
        "U04",
        "Processor",
        &[],
    ),
];

fn resources() -> Vec<Resource> {
    HARDWARE
        .iter()
        .map(
            |(name, depth, tm, text, status, serial, loc, cat, cfg)| Resource {
                name: (*name).into(),
                depth: *depth,
                type_model: (*tm).into(),
                text: (*text).into(),
                status: (*status).into(),
                serial: (*serial).into(),
                location: (*loc).into(),
                category: (*cat).into(),
                cfg: cfg
                    .iter()
                    .map(|(n, t)| ((*n).to_string(), (*t).to_string()))
                    .collect(),
            },
        )
        .collect()
}

fn msg(kind: Kind, id: &str, data: &[&str]) -> Message {
    let t = text::second_level(id, data)
        .map(|s| s.text)
        .unwrap_or_default();
    Message::new(kind, id, &t)
}

/// A CL string literal's text.
fn unquote(s: &str) -> String {
    let s = s.trim();
    match s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(r) => r.replace("''", "'"),
        None => s.to_string(),
    }
}

impl Config for FixtureConfig {
    fn sysvals(&self, _user: &str) -> Vec<SysVal> {
        self.sysvals.lock().expect("sysvals").clone()
    }

    fn disks(&self, _user: &str, _reset: bool) -> Disks {
        disks()
    }

    fn resources(&self, _user: &str) -> Vec<Resource> {
        resources()
    }

    fn time(&self, _user: &str) -> Time {
        let sv = self.sysvals.lock().expect("sysvals");
        let get = |n: &str| {
            sv.iter()
                .find(|s| s.name == n)
                .map(|s| s.value.clone())
                .unwrap_or_default()
        };
        Time {
            zone: get("QTIMZON"),
            zones: vec![
                ("QN0500EST3".into(), "Eastern, US DST".into()),
                ("QN0600CST3".into(), String::new()),
                ("Q0000UTC".into(), String::new()),
            ],
            offset: "-04:00".into(),
            dst: true,
            date_format: get("QDATFMT"),
            date_sep: get("QDATSEP"),
            time_sep: get("QTIMSEP"),
            adjust: get("QTIMADJ"),
            adjusters: vec![
                ("*NONE".into(), String::new()),
                ("QS400/QTOTNTP".into(), "NTP client".into()),
            ],
            ntp_servers: self.ntp.lock().expect("ntp").clone(),
            last_sync: Some((secs(2026, 10, 29, 14, 0, 0), 4_000)),
            last_fail: None,
            ntp_active: true,
        }
    }

    fn sync_now(&self, _user: &str) -> bool {
        self.ran.lock().expect("ran").push("SYNC NOW".to_string());
        true
    }

    fn run(&self, user: &str, command: &str) -> Result<Message, Message> {
        self.ran.lock().expect("ran").push(command.to_string());
        let (name, p) = parse(command);
        match name.as_str() {
            "CHGSYSVAL" => {
                let sv = p.get("SYSVAL").cloned().unwrap_or_default();
                let mut all = self.sysvals.lock().expect("sysvals");
                let Some(v) = all.iter_mut().find(|s| s.name == sv) else {
                    return Err(msg(Kind::Error, "CPF1842", &[&sv]));
                };
                if v.category == "security" && user != "QSECOFR" {
                    return Err(msg(Kind::Error, "CPF2207", &["CHGSYSVAL", "QSYS", "*CMD"]));
                }
                let value = unquote(p.get("VALUE").map_or("", String::as_str));
                if !v.allowed.accepts(&value) {
                    return Err(msg(Kind::Error, "CPD9B8D", &[&sv, &value]));
                }
                v.value = value.to_ascii_uppercase();
                v.changed = Some((super::now(), user.to_string()));
                Ok(msg(Kind::Done, "CPI9C01", &[&sv]))
            }
            "CHGNTPA" => {
                let r = p.get("RMTSYS").cloned().unwrap_or_default();
                let mut out = Vec::new();
                let mut cur = String::new();
                let mut quoted = false;
                for c in r.chars() {
                    match c {
                        '\'' => quoted = !quoted,
                        ' ' if !quoted => out.push(std::mem::take(&mut cur)),
                        _ => cur.push(c),
                    }
                }
                out.push(cur);
                *self.ntp.lock().expect("ntp") =
                    out.into_iter().filter(|s| !s.is_empty()).collect();
                Ok(msg(Kind::Done, "TCP1A10", &["NTP"]))
            }
            "VRYCFG" => {
                let obj = p.get("CFGOBJ").cloned().unwrap_or_default();
                let on = p.get("STATUS").is_some_and(|s| s == "*ON");
                Ok(msg(
                    Kind::Done,
                    if on { "CPC2605" } else { "CPC2606" },
                    &[&obj],
                ))
            }
            _ => Err(msg(Kind::Error, "CPD0030", &[&name])),
        }
    }
}
