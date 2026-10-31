// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The response file of an automatic install, `QS400/INSTALL.RSP` on
//! the media or the file `qs400.rsp=` names (docs/INSTALL.md, "Response
//! file"). Text, one `key = value` a line, `#` starts a comment, keys
//! in any case and order; `mirror` and `unit` may repeat. Every key
//! has a default, so an empty file installs with the defaults.

use std::fmt::Write as _;

use crate::{Error, Result};

/// The LIC install option (the `Install Licensed Internal Code (LIC)`
/// menu's number).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LicOption {
    /// 2: install and initialize the system.
    #[default]
    Initialize,
    /// 3: install and keep ASP 1 and its configuration.
    Recover,
    /// 4: install and restore the load source from its mirrored partner.
    RestoreUnits,
}

impl LicOption {
    /// The menu number.
    #[must_use]
    pub fn number(self) -> u8 {
        match self {
            Self::Initialize => 2,
            Self::Recover => 3,
            Self::RestoreUnits => 4,
        }
    }
}

/// What QSECOFR's password is after the install.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum QsecofrPolicy {
    /// The shipped password, expired: changed at the first sign-on.
    #[default]
    Expired,
    /// This password, not expired.
    Set(String),
}

/// The licensed programs to install after the operating system.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Licpgms {
    /// Every product on the media (`GO LICPGM` option 1).
    All,
    /// None.
    #[default]
    None,
    /// These, `PRODUCT` or `PRODUCT/OPTION`.
    List(Vec<(String, Option<String>)>),
}

/// The answers.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // one per IPL option
pub struct Response {
    /// The LIC install option.
    pub lic_option: LicOption,
    /// The load source: a resource name (`DD001`), a device node, or
    /// `None` for the first disk.
    pub load_source: Option<String>,
    /// Pairs to mirror: the load source's partner first among them;
    /// each a resource name or device node.
    pub mirror: Vec<(String, String)>,
    /// Further disks added to ASP 1 unprotected.
    pub units: Vec<String>,
    /// The install device: `*OPT`, `*TAP`, `*VRT` or `*MEDIA` (where
    /// the response file is).
    pub device: String,
    /// The language feature.
    pub language: String,
    /// The system name (`SYSNAME`), `None` to keep the default.
    pub system_name: Option<String>,
    /// The first link's IPv4 address and prefix.
    pub ip: Option<String>,
    /// The default route.
    pub gateway: Option<String>,
    /// `YYYY-MM-DD`, `None` for the clock as it is.
    pub date: Option<String>,
    /// `HH:MM:SS`, `None` for the clock as it is.
    pub time: Option<String>,
    /// QSECOFR's password.
    pub qsecofr: QsecofrPolicy,
    /// Licensed programs.
    pub licpgm: Licpgms,
    /// IPL option: start print writers (`QSTRPRTWTR`).
    pub start_print_writers: bool,
    /// IPL option: clear job queues.
    pub clear_job_queues: bool,
    /// IPL option: clear output queues.
    pub clear_output_queues: bool,
    /// IPL option: clear incomplete job logs.
    pub clear_job_logs: bool,
    /// IPL option: start in restricted state.
    pub restricted_state: bool,
    /// `QSTRUPPGM`: `QSYS/QSTRUP`, another program, or `*NONE`.
    pub qstrup: String,
    /// `QAUTOCFG`.
    pub auto_configure: bool,
    /// `QDEVNAMING`.
    pub device_naming: String,
    /// `QSPCENV`.
    pub special_environment: String,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            lic_option: LicOption::Initialize,
            load_source: None,
            mirror: Vec::new(),
            units: Vec::new(),
            device: "*MEDIA".into(),
            language: "2924".into(),
            system_name: None,
            ip: None,
            gateway: None,
            date: None,
            time: None,
            qsecofr: QsecofrPolicy::Expired,
            licpgm: Licpgms::None,
            start_print_writers: true,
            clear_job_queues: false,
            clear_output_queues: false,
            clear_job_logs: false,
            restricted_state: false,
            qstrup: "QSYS/QSTRUP".into(),
            auto_configure: true,
            device_naming: "*NORMAL".into(),
            special_environment: "*NONE".into(),
        }
    }
}

/// The first line a response file may start with; optional.
pub const MAGIC: &str = "QS400-INSTALL-RSP 1";

/// Every key, in the order [`Response::to_text`] writes them.
pub const KEYS: &[&str] = &[
    "lic-option",
    "load-source",
    "mirror",
    "unit",
    "device",
    "language",
    "system-name",
    "ip",
    "gateway",
    "date",
    "time",
    "qsecofr-password",
    "licpgm",
    "start-print-writers",
    "clear-job-queues",
    "clear-output-queues",
    "clear-job-logs",
    "restricted-state",
    "qstrup",
    "auto-configure",
    "device-naming",
    "special-environment",
];

fn yn(line: usize, v: &str) -> Result<bool> {
    match v.to_ascii_uppercase().as_str() {
        "Y" | "YES" | "*YES" => Ok(true),
        "N" | "NO" | "*NO" => Ok(false),
        _ => Err(bad(line, format!("'{v}' is not Y or N"))),
    }
}

fn bad(line: usize, what: impl Into<String>) -> Error {
    Error::Response {
        line,
        what: what.into(),
    }
}

fn digits(s: &str, n: usize) -> bool {
    s.len() == n && s.bytes().all(|b| b.is_ascii_digit())
}

fn check_date(line: usize, v: &str) -> Result<String> {
    let p: Vec<&str> = v.split('-').collect();
    let ok = p.len() == 3 && digits(p[0], 4) && digits(p[1], 2) && digits(p[2], 2);
    let ok = ok
        && (1..=12).contains(&p[1].parse::<u32>().unwrap_or(0))
        && (1..=31).contains(&p[2].parse::<u32>().unwrap_or(0));
    if ok {
        Ok(v.to_string())
    } else {
        Err(bad(line, format!("date '{v}' is not YYYY-MM-DD")))
    }
}

fn check_time(line: usize, v: &str) -> Result<String> {
    let p: Vec<&str> = v.split(':').collect();
    let ok = p.len() == 3 && p.iter().all(|x| digits(x, 2));
    let ok = ok
        && p[0].parse::<u32>().unwrap_or(99) < 24
        && p[1].parse::<u32>().unwrap_or(99) < 60
        && p[2].parse::<u32>().unwrap_or(99) < 60;
    if ok {
        Ok(v.to_string())
    } else {
        Err(bad(line, format!("time '{v}' is not HH:MM:SS")))
    }
}

fn check_ip(line: usize, v: &str, prefix: bool) -> Result<String> {
    let (addr, pfx) = match v.split_once('/') {
        Some((a, p)) if prefix => (a, Some(p)),
        None if !prefix => (v, None),
        _ => return Err(bad(line, format!("'{v}' is not an address{}", if prefix { "/prefix" } else { "" }))),
    };
    let ok = addr.split('.').count() == 4 && addr.split('.').all(|o| o.parse::<u8>().is_ok());
    let ok = ok && pfx.is_none_or(|p| p.parse::<u8>().is_ok_and(|n| n <= 32));
    if ok {
        Ok(v.to_string())
    } else {
        Err(bad(line, format!("'{v}' is not a valid IPv4 address")))
    }
}

fn special(v: &str) -> bool {
    v.eq_ignore_ascii_case("*CURRENT") || v.eq_ignore_ascii_case("*DFT")
}

impl Response {
    /// Parse a response file.
    #[allow(clippy::too_many_lines, clippy::many_single_char_names)]
    pub fn parse(text: &str) -> Result<Self> {
        let mut r = Self::default();
        for (i, raw) in text.lines().enumerate() {
            let n = i + 1;
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() || (n == 1 && line == MAGIC) {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| bad(n, "expected key = value"))?;
            let key = key.trim().to_ascii_lowercase();
            let v = value.trim();
            let up = v.to_ascii_uppercase();
            match key.as_str() {
                "lic-option" => {
                    r.lic_option = match v {
                        "2" => LicOption::Initialize,
                        "3" => LicOption::Recover,
                        "4" => LicOption::RestoreUnits,
                        _ => return Err(bad(n, format!("lic-option '{v}' is not 2, 3 or 4"))),
                    }
                }
                "load-source" => {
                    r.load_source = (!up.is_empty() && up != "*FIRST").then(|| v.to_string());
                }
                "mirror" => {
                    let w: Vec<&str> = v.split_whitespace().collect();
                    let [a, b] = w.as_slice() else {
                        return Err(bad(n, "mirror needs two disks"));
                    };
                    if a.eq_ignore_ascii_case(b) {
                        return Err(bad(n, "a disk cannot mirror itself"));
                    }
                    r.mirror.push(((*a).to_string(), (*b).to_string()));
                }
                "unit" => r.units.extend(v.split_whitespace().map(ToString::to_string)),
                "device" => {
                    if !["*OPT", "*TAP", "*VRT", "*MEDIA"].contains(&up.as_str()) {
                        return Err(bad(n, format!("device '{v}' is not *OPT, *TAP, *VRT or *MEDIA")));
                    }
                    r.device = up;
                }
                "language" => {
                    if !digits(v, 4) {
                        return Err(bad(n, format!("language '{v}' is not a feature number")));
                    }
                    r.language = v.to_string();
                }
                "system-name" => {
                    let ok = (1..=8).contains(&v.len())
                        && v.bytes().next().is_some_and(|b| b.is_ascii_alphabetic())
                        && v.bytes().all(|b| b.is_ascii_alphanumeric());
                    if !ok {
                        return Err(bad(n, format!("system name '{v}' is not 1 to 8 letters and digits")));
                    }
                    r.system_name = Some(up);
                }
                "ip" => r.ip = Some(check_ip(n, v, true)?),
                "gateway" => r.gateway = Some(check_ip(n, v, false)?),
                "date" => r.date = if special(v) { None } else { Some(check_date(n, v)?) },
                "time" => r.time = if special(v) { None } else { Some(check_time(n, v)?) },
                "qsecofr-password" => {
                    r.qsecofr = if up == "*EXPIRED" || v.is_empty() {
                        QsecofrPolicy::Expired
                    } else if v.len() > 128 || v.contains(char::is_whitespace) {
                        return Err(bad(n, "the password has blanks or is too long"));
                    } else {
                        QsecofrPolicy::Set(v.to_string())
                    }
                }
                "licpgm" => {
                    r.licpgm = match up.as_str() {
                        "*ALL" => Licpgms::All,
                        "*NONE" | "" => Licpgms::None,
                        _ => Licpgms::List(
                            up.split_whitespace()
                                .map(|p| match p.split_once('/') {
                                    Some((p, o)) => (p.to_string(), Some(o.to_string())),
                                    None => (p.to_string(), None),
                                })
                                .collect(),
                        ),
                    }
                }
                "start-print-writers" => r.start_print_writers = yn(n, v)?,
                "clear-job-queues" => r.clear_job_queues = yn(n, v)?,
                "clear-output-queues" => r.clear_output_queues = yn(n, v)?,
                "clear-job-logs" => r.clear_job_logs = yn(n, v)?,
                "restricted-state" => r.restricted_state = yn(n, v)?,
                "qstrup" => {
                    let ok = up == "*NONE"
                        || up.split_once('/').is_some_and(|(l, p)| {
                            !l.is_empty() && !p.is_empty() && l.len() <= 10 && p.len() <= 10
                        });
                    if !ok {
                        return Err(bad(n, format!("qstrup '{v}' is not LIB/PGM or *NONE")));
                    }
                    r.qstrup = up;
                }
                "auto-configure" => r.auto_configure = yn(n, v)?,
                "device-naming" => {
                    if !["*NORMAL", "*S36", "*DEVADR"].contains(&up.as_str()) {
                        return Err(bad(n, format!("device-naming '{v}' is not *NORMAL, *S36 or *DEVADR")));
                    }
                    r.device_naming = up;
                }
                "special-environment" => {
                    if !["*NONE", "*S36"].contains(&up.as_str()) {
                        return Err(bad(n, format!("special-environment '{v}' is not *NONE or *S36")));
                    }
                    r.special_environment = up;
                }
                other => return Err(bad(n, format!("unknown key '{other}'"))),
            }
        }
        Ok(r)
    }

    /// The text form, every key written.
    #[must_use]
    pub fn to_text(&self) -> String {
        let yn = |b: bool| if b { "Y" } else { "N" };
        let opt = |o: &Option<String>, d: &str| o.clone().unwrap_or_else(|| d.to_string());
        let mut s = format!("{MAGIC}\n");
        let _ = writeln!(s, "lic-option = {}", self.lic_option.number());
        let _ = writeln!(s, "load-source = {}", opt(&self.load_source, "*FIRST"));
        for (a, b) in &self.mirror {
            let _ = writeln!(s, "mirror = {a} {b}");
        }
        for u in &self.units {
            let _ = writeln!(s, "unit = {u}");
        }
        let _ = writeln!(s, "device = {}", self.device);
        let _ = writeln!(s, "language = {}", self.language);
        if let Some(n) = &self.system_name {
            let _ = writeln!(s, "system-name = {n}");
        }
        if let Some(ip) = &self.ip {
            let _ = writeln!(s, "ip = {ip}");
        }
        if let Some(g) = &self.gateway {
            let _ = writeln!(s, "gateway = {g}");
        }
        let _ = writeln!(s, "date = {}", opt(&self.date, "*CURRENT"));
        let _ = writeln!(s, "time = {}", opt(&self.time, "*CURRENT"));
        let pw = match &self.qsecofr {
            QsecofrPolicy::Expired => "*EXPIRED".to_string(),
            QsecofrPolicy::Set(p) => p.clone(),
        };
        let _ = writeln!(s, "qsecofr-password = {pw}");
        let lp = match &self.licpgm {
            Licpgms::All => "*ALL".to_string(),
            Licpgms::None => "*NONE".to_string(),
            Licpgms::List(v) => v
                .iter()
                .map(|(p, o)| o.as_ref().map_or_else(|| p.clone(), |o| format!("{p}/{o}")))
                .collect::<Vec<_>>()
                .join(" "),
        };
        let _ = writeln!(s, "licpgm = {lp}");
        let _ = writeln!(s, "start-print-writers = {}", yn(self.start_print_writers));
        let _ = writeln!(s, "clear-job-queues = {}", yn(self.clear_job_queues));
        let _ = writeln!(s, "clear-output-queues = {}", yn(self.clear_output_queues));
        let _ = writeln!(s, "clear-job-logs = {}", yn(self.clear_job_logs));
        let _ = writeln!(s, "restricted-state = {}", yn(self.restricted_state));
        let _ = writeln!(s, "qstrup = {}", self.qstrup);
        let _ = writeln!(s, "auto-configure = {}", yn(self.auto_configure));
        let _ = writeln!(s, "device-naming = {}", self.device_naming);
        let _ = writeln!(s, "special-environment = {}", self.special_environment);
        s
    }

    /// Whether `disk` (a resource name or device node) is named by
    /// `want`.
    #[must_use]
    pub fn names(want: &str, resource: &str, device: &str) -> bool {
        want.eq_ignore_ascii_case(resource)
            || want == device
            || device.rsplit('/').next().is_some_and(|d| want == d)
    }
}

/// Where `qs400.rsp=` points: a path on the media, or a URL fetched at
/// IPL (`BLOCKED(phase11)`: PXE fetching waits for the network boot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A path on the install media (or `file:` path).
    Media(String),
    /// A URL, not fetched yet.
    Url(String),
}

impl Source {
    /// From the command line value.
    #[must_use]
    pub fn of(v: &str) -> Self {
        if let Some(p) = v.strip_prefix("file:") {
            Self::Media(p.trim_start_matches('/').to_string())
        } else if v.contains("://") {
            Self::Url(v.to_string())
        } else {
            Self::Media(v.trim_start_matches('/').to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_defaults() {
        assert_eq!(Response::parse("").unwrap(), Response::default());
        assert_eq!(
            Response::parse(&Response::default().to_text()).unwrap(),
            Response::default()
        );
    }

    #[test]
    fn every_key_parses_and_round_trips() {
        let text = "QS400-INSTALL-RSP 1
# two disks, mirrored
LIC-Option = 2
load-source = DD001
mirror = DD001 DD002
unit = DD003 DD004
device = *opt
language = 2924
system-name = qsys01
ip = 10.0.2.15/24
gateway = 10.0.2.2
date = 2026-10-31
time = 12:30:00
qsecofr-password = Quartz1
licpgm = QS00WDS QS00WDS/31 QS00TC1
start-print-writers = N
clear-job-queues = Y
clear-output-queues = y
clear-job-logs = N
restricted-state = N
qstrup = QGPL/MYSTRUP
auto-configure = N
device-naming = *devadr
special-environment = *S36
";
        let r = Response::parse(text).unwrap();
        assert_eq!(r.load_source.as_deref(), Some("DD001"));
        assert_eq!(r.mirror, vec![("DD001".into(), "DD002".into())]);
        assert_eq!(r.units, vec!["DD003".to_string(), "DD004".into()]);
        assert_eq!(r.device, "*OPT");
        assert_eq!(r.system_name.as_deref(), Some("QSYS01"));
        assert_eq!(r.qsecofr, QsecofrPolicy::Set("Quartz1".into()));
        assert_eq!(
            r.licpgm,
            Licpgms::List(vec![
                ("QS00WDS".into(), None),
                ("QS00WDS".into(), Some("31".into())),
                ("QS00TC1".into(), None)
            ])
        );
        assert!(!r.start_print_writers && r.clear_job_queues && r.clear_output_queues);
        assert_eq!(r.device_naming, "*DEVADR");
        assert_eq!(Response::parse(&r.to_text()).unwrap(), r);
        // Every key is in the table, in the written order.
        let written: Vec<String> = r
            .to_text()
            .lines()
            .skip(1)
            .filter_map(|l| l.split_once(" = ").map(|(k, _)| k.to_string()))
            .collect();
        let order: Vec<usize> = written
            .iter()
            .map(|w| KEYS.iter().position(|k| k == w).expect("a known key"))
            .collect();
        assert!(order.windows(2).all(|w| w[0] <= w[1]), "{written:?}");
    }

    #[test]
    fn mistakes_name_their_line() {
        for (text, line) in [
            ("language = 29x4", 1),
            ("\nmirror = DD001", 2),
            ("date = 2026-13-01", 1),
            ("time = 25:00:00", 1),
            ("ip = 10.0.2.300/24", 1),
            ("colour = blue", 1),
            ("nonsense", 1),
            ("lic-option = 9", 1),
            ("mirror = DD001 dd001", 1),
            ("qstrup = MYSTRUP", 1),
        ] {
            match Response::parse(text) {
                Err(Error::Response { line: l, .. }) => assert_eq!(l, line, "{text}"),
                other => panic!("{text}: {other:?}"),
            }
        }
        assert_eq!(Response::parse("licpgm = *ALL").unwrap().licpgm, Licpgms::All);
        assert_eq!(Response::parse("date = *CURRENT").unwrap().date, None);
    }

    #[test]
    fn sources_and_names() {
        assert_eq!(Source::of("QS400/INSTALL.RSP"), Source::Media("QS400/INSTALL.RSP".into()));
        assert_eq!(Source::of("file:/QS400/X.RSP"), Source::Media("QS400/X.RSP".into()));
        assert!(matches!(Source::of("http://10.0.2.2/x.rsp"), Source::Url(_)));
        assert!(Response::names("dd001", "DD001", "/dev/vda"));
        assert!(Response::names("vda", "DD001", "/dev/vda"));
        assert!(Response::names("/dev/vda", "DD001", "/dev/vda"));
        assert!(!Response::names("DD002", "DD001", "/dev/vda"));
    }
}
