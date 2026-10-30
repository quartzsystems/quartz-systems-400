// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `*LIND`: an Ethernet line description, the parameters of
//! `CRTLINETH` as the pub400 prompt shows them. The ones QS/400 acts on
//! are named fields; the rest (`SSAP`, `GRPADR`, `SRVOPT`, `AUT`, ...)
//! are kept verbatim in `extras` so `DSPLIND` shows what was typed.
//! [`codec`] holds the payload.

mod codec;

use std::collections::BTreeMap;

pub use codec::LAYOUT;

/// `CMNRCYLMT`: recovery attempts and the minutes they count in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RcyLimit {
    /// `*SYSVAL`: `QCMNRCYLMT`.
    SysVal,
    /// `(count interval)`.
    Limit {
        /// Second-level recovery attempts, 0 for none.
        count: u16,
        /// Minutes the attempts are counted in.
        interval: u16,
    },
}

impl RcyLimit {
    /// As `CRTLINETH` spells it.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            RcyLimit::SysVal => "*SYSVAL".into(),
            RcyLimit::Limit { count, interval } => format!("{count} {interval}"),
        }
    }
}

/// `AUTODLTCTL`: minutes before an unused auto-created controller goes.
pub const AUTODLTCTL_NONE: u32 = u32::MAX;

/// An Ethernet line description.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the *YES/*NO parameters
pub struct Lind {
    /// `RSRCNAME`: a `CMNnn` port, `*AGG`, `*NWID` or `*NWSD`.
    pub rsrcname: String,
    /// `BRIDGE`.
    pub bridge: String,
    /// `ONLINE(*YES)`.
    pub online: bool,
    /// `VRYWAIT`: 0 for `*NOWAIT`, else 15 to 180 seconds.
    pub vrywait: u16,
    /// `AGGPCY`: standard (`*ETHCHL`, `*LNKAGG`) and policy type
    /// (`*DFT`, `*SRCPORT`, `*DESTPORT`, `*SRCDESTP`, `*RNDRBN`).
    pub aggpcy: (String, String),
    /// `AGGRSCL`: the ports of an aggregated line.
    pub aggrscl: Vec<String>,
    /// `NWS`: network server description and port number.
    pub nws: (String, u16),
    /// `ASSOCPORT`.
    pub assocport: String,
    /// `ADPTADR`: `*ADPT` or twelve hexadecimal digits.
    pub adptadr: String,
    /// `ETHSTD`: `*ETHV2`, `*IEEE8023`, `*ALL`.
    pub ethstd: String,
    /// `LINESPEED`: `*AUTO`, `10M`, `100M`, `1G`, `10G`, ...
    pub linespeed: String,
    /// `DUPLEX`: `*AUTO`, `*HALF`, `*FULL`.
    pub duplex: String,
    /// `MAXFRAME`: 1496 to 8996; the MTU is four more.
    pub maxframe: u32,
    /// `MAXCTL`.
    pub maxctl: u16,
    /// `EXCHID`: `*SYSGEN` or eight hexadecimal digits.
    pub exchid: String,
    /// `THRESHOLD`: `*OFF`, `*MIN`, `*MED`, `*MAX`.
    pub threshold: String,
    /// `GENTSTFRM(*YES)`.
    pub gentstfrm: bool,
    /// `LINKSPEED`.
    pub linkspeed: String,
    /// `COSTCNN`.
    pub costcnn: u8,
    /// `COSTBYTE`.
    pub costbyte: u8,
    /// `SECURITY`.
    pub security: String,
    /// `PRPDLY`.
    pub prpdly: String,
    /// `USRDFN1`, `USRDFN2`, `USRDFN3`.
    pub usrdfn: [u8; 3],
    /// `AUTOCRTCTL(*YES)`.
    pub autocrtctl: bool,
    /// `AUTODLTCTL` minutes, [`AUTODLTCTL_NONE`] for `*NONE`.
    pub autodltctl: u32,
    /// `CMNRCYLMT`.
    pub cmnrcylmt: RcyLimit,
    /// `MSGQ`: `*SYSVAL` or `LIB/NAME`.
    pub msgq: String,
    /// `TEXT`.
    pub text: String,
    /// Every other parameter as typed, by keyword.
    pub extras: BTreeMap<String, String>,
}

impl Default for Lind {
    fn default() -> Self {
        Self {
            rsrcname: String::new(),
            bridge: "*NONE".into(),
            online: true,
            vrywait: 0,
            aggpcy: ("*ETHCHL".into(), "*DFT".into()),
            aggrscl: Vec::new(),
            nws: ("*NONE".into(), 0),
            assocport: "*NONE".into(),
            adptadr: "*ADPT".into(),
            ethstd: "*ETHV2".into(),
            linespeed: "*AUTO".into(),
            duplex: "*AUTO".into(),
            maxframe: 1496,
            maxctl: 40,
            exchid: "*SYSGEN".into(),
            threshold: "*OFF".into(),
            gentstfrm: true,
            linkspeed: "*AUTO".into(),
            costcnn: 0,
            costbyte: 0,
            security: "*NONSECURE".into(),
            prpdly: "*LAN".into(),
            usrdfn: [128; 3],
            autocrtctl: false,
            autodltctl: 1440,
            cmnrcylmt: RcyLimit::SysVal,
            msgq: "*SYSVAL".into(),
            text: String::new(),
            extras: BTreeMap::new(),
        }
    }
}

/// `*YES`/`*NO` as a flag.
fn yes_no(kwd: &str, v: &str) -> Result<bool, String> {
    match v {
        "*YES" => Ok(true),
        "*NO" => Ok(false),
        _ => Err(format!("CPD0084 {kwd}({v}) is not *YES or *NO")),
    }
}

fn number<T: std::str::FromStr>(kwd: &str, v: &str) -> Result<T, String> {
    v.trim()
        .parse()
        .map_err(|_| format!("CPD0076 {kwd}({v}) is not a number"))
}

/// A `*NAME` with special values allowed.
fn name(kwd: &str, v: &str, special: &[&str]) -> Result<String, String> {
    let v = v.trim().to_ascii_uppercase();
    if special.contains(&v.as_str()) || qsname::Name::parse(&v).is_ok() {
        Ok(v)
    } else {
        Err(format!("CPD0078 {kwd}({v}) is not a valid name"))
    }
}

/// A value of a restricted set.
fn one_of(kwd: &str, v: &str, values: &[&str]) -> Result<String, String> {
    let v = v.trim().to_ascii_uppercase();
    if values.contains(&v.as_str()) {
        Ok(v)
    } else {
        Err(format!(
            "CPD0084 {kwd}({v}) is not one of {}",
            values.join(" ")
        ))
    }
}

/// The text of a `QUAL` value as `LIB/NAME`, a special value as is.
fn qualified(kwd: &str, v: &str) -> Result<String, String> {
    let v = v.trim().to_ascii_uppercase();
    if v.starts_with('*') {
        return Ok(v);
    }
    let (lib, n) = v.split_once('/').unwrap_or(("*LIBL", &v));
    let n = name(kwd, n, &[])?;
    let lib = name(kwd, lib, &["*LIBL", "*CURLIB"])?;
    Ok(format!("{lib}/{n}"))
}

/// The words of a group value, `(a b)` or `a b`.
fn words(v: &str) -> Vec<String> {
    v.trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .split_whitespace()
        .map(str::to_ascii_uppercase)
        .collect()
}

impl Lind {
    /// Set one parameter from its value as the command spells it: a
    /// scalar as its text, a group as `(a b)`, a qualified name as
    /// `LIB/NAME`, a list as blank-separated values. `*SAME` leaves the
    /// field alone. A refusal is a `CPDnnnn` line.
    #[allow(clippy::too_many_lines)] // one arm a parameter
    pub fn set(&mut self, kwd: &str, value: &str) -> Result<(), String> {
        let v = value.trim();
        // The block spells *SAME of a number -99.
        if v.eq_ignore_ascii_case("*SAME") || v == "-99" {
            return Ok(());
        }
        let up = v.to_ascii_uppercase();
        match kwd {
            "RSRCNAME" => self.rsrcname = name(kwd, &up, &["*AGG", "*NWID", "*NWSD"])?,
            "BRIDGE" => self.bridge = name(kwd, &up, &["*NONE"])?,
            "ONLINE" => self.online = yes_no(kwd, &up)?,
            "VRYWAIT" => {
                self.vrywait = if up == "*NOWAIT" || up == "0" {
                    0
                } else {
                    let n: u16 = number(kwd, &up)?;
                    if !(15..=180).contains(&n) {
                        return Err(format!("CPD0085 VRYWAIT({n}) is not 15 to 180"));
                    }
                    n
                };
            }
            "AGGPCY" => {
                let w = words(&up);
                let std = one_of(
                    kwd,
                    w.first().map_or("*ETHCHL", String::as_str),
                    &["*ETHCHL", "*LNKAGG"],
                )?;
                let pcy = one_of(
                    kwd,
                    w.get(1).map_or("*DFT", String::as_str),
                    &["*DFT", "*SRCPORT", "*DESTPORT", "*SRCDESTP", "*RNDRBN"],
                )?;
                self.aggpcy = (std, pcy);
            }
            "AGGRSCL" => {
                self.aggrscl = words(&up)
                    .iter()
                    .filter(|w| *w != "*NONE")
                    .map(|w| name(kwd, w, &[]))
                    .collect::<Result<_, _>>()?;
            }
            "NWS" => {
                let w = words(&up);
                let n = name(kwd, w.first().map_or("*NONE", String::as_str), &["*NONE"])?;
                let port = w.get(1).map_or(Ok(0), |p| number(kwd, p))?;
                self.nws = (n, port);
            }
            "ASSOCPORT" => self.assocport = name(kwd, &up, &["*NONE"])?,
            "ADPTADR" => {
                if up != "*ADPT" && !(up.len() == 12 && up.bytes().all(|b| b.is_ascii_hexdigit())) {
                    return Err(format!(
                        "CPD0084 ADPTADR({up}) is not *ADPT or 12 hexadecimal digits"
                    ));
                }
                self.adptadr = up;
            }
            "ETHSTD" => self.ethstd = one_of(kwd, &up, &["*ETHV2", "*IEEE8023", "*ALL"])?,
            "LINESPEED" => {
                self.linespeed = one_of(
                    kwd,
                    &up,
                    &["*AUTO", "10M", "100M", "1G", "10G", "25G", "100G"],
                )?;
            }
            "DUPLEX" => self.duplex = one_of(kwd, &up, &["*AUTO", "*HALF", "*FULL"])?,
            "MAXFRAME" => {
                let n: u32 = number(kwd, &up)?;
                if !(1496..=8996).contains(&n) {
                    return Err(format!("CPD0085 MAXFRAME({n}) is not 1496 to 8996"));
                }
                self.maxframe = n;
            }
            "MAXCTL" => {
                let n: u16 = number(kwd, &up)?;
                if !(1..=256).contains(&n) {
                    return Err(format!("CPD0085 MAXCTL({n}) is not 1 to 256"));
                }
                self.maxctl = n;
            }
            "EXCHID" => {
                if up != "*SYSGEN" && !(up.len() == 8 && up.bytes().all(|b| b.is_ascii_hexdigit()))
                {
                    return Err(format!(
                        "CPD0084 EXCHID({up}) is not *SYSGEN or 8 hexadecimal digits"
                    ));
                }
                self.exchid = up;
            }
            "THRESHOLD" => self.threshold = one_of(kwd, &up, &["*OFF", "*MIN", "*MED", "*MAX"])?,
            "GENTSTFRM" => self.gentstfrm = yes_no(kwd, &up)?,
            "LINKSPEED" => self.linkspeed = up,
            "COSTCNN" => self.costcnn = number(kwd, &up)?,
            "COSTBYTE" => self.costbyte = number(kwd, &up)?,
            "SECURITY" => self.security = up,
            "PRPDLY" => self.prpdly = up,
            "USRDFN1" => self.usrdfn[0] = number(kwd, &up)?,
            "USRDFN2" => self.usrdfn[1] = number(kwd, &up)?,
            "USRDFN3" => self.usrdfn[2] = number(kwd, &up)?,
            "AUTOCRTCTL" => self.autocrtctl = yes_no(kwd, &up)?,
            "AUTODLTCTL" => {
                self.autodltctl = if up == "*NONE" {
                    AUTODLTCTL_NONE
                } else {
                    number(kwd, &up)?
                };
            }
            "CMNRCYLMT" => {
                let w = words(&up);
                self.cmnrcylmt = if w.first().is_some_and(|s| s == "*SYSVAL" || s == "-1")
                    || w.is_empty()
                {
                    RcyLimit::SysVal
                } else {
                    let count: u16 = number(kwd, &w[0])?;
                    let interval: u16 = w.get(1).map_or(Ok(0), |s| number(kwd, s))?;
                    if count > 99 || interval > 120 {
                        return Err(format!(
                            "CPD0085 CMNRCYLMT({count} {interval}): count 0 to 99, interval 0 to 120"
                        ));
                    }
                    RcyLimit::Limit { count, interval }
                };
            }
            "MSGQ" => self.msgq = qualified(kwd, &up)?,
            "TEXT" => {
                self.text = if up == "*BLANK" {
                    String::new()
                } else {
                    v.trim_matches('\'').to_string()
                };
            }
            other => {
                self.extras.insert(other.to_string(), v.to_string());
            }
        }
        Ok(())
    }

    /// Whether the line is an aggregate (`RSRCNAME(*AGG)`).
    #[must_use]
    pub fn is_aggregate(&self) -> bool {
        self.rsrcname == "*AGG"
    }

    /// The message queue as `LIB/NAME`: `*SYSVAL` is `cfgmsgq` (the
    /// `QCFGMSGQ` system value, `LIB/NAME`), `*SYSOPR` is
    /// `QSYS/QSYSOPR`, `*LIBL/NAME` is taken as `QSYS/NAME`.
    #[must_use]
    pub fn resolved_msgq(&self, cfgmsgq: &str) -> String {
        match self.msgq.as_str() {
            "*SYSVAL" if !cfgmsgq.is_empty() => cfgmsgq.to_string(),
            "*SYSVAL" | "*SYSOPR" | "" => "QSYS/QSYSOPR".into(),
            q => match q.split_once('/') {
                Some(("*LIBL" | "*CURLIB", n)) => format!("QSYS/{n}"),
                _ => q.to_string(),
            },
        }
    }

    /// What the status machine needs: the recovery limit with
    /// `*SYSVAL` as `sysval` (`QCMNRCYLMT`: count and minutes), the
    /// queue resolved against `cfgmsgq`.
    #[must_use]
    pub fn line_config(&self, sysval: (u16, u16), cfgmsgq: &str) -> crate::status::LineConfig {
        let (rcy_count, rcy_interval) = match self.cmnrcylmt {
            RcyLimit::SysVal => sysval,
            RcyLimit::Limit { count, interval } => (count, interval),
        };
        crate::status::LineConfig {
            rcy_count,
            rcy_interval,
            msgq: self.resolved_msgq(cfgmsgq),
            online: self.online,
        }
    }

    /// `VRYWAIT` as the display spells it.
    #[must_use]
    pub fn vrywait_text(&self) -> String {
        if self.vrywait == 0 {
            "*NOWAIT".into()
        } else {
            self.vrywait.to_string()
        }
    }

    /// `AUTODLTCTL` as the display spells it.
    #[must_use]
    pub fn autodltctl_text(&self) -> String {
        if self.autodltctl == AUTODLTCTL_NONE {
            "*NONE".into()
        } else {
            self.autodltctl.to_string()
        }
    }

    /// `AGGRSCL` as one value: the names, or `*NONE`.
    #[must_use]
    pub fn aggrscl_text(&self) -> String {
        if self.aggrscl.is_empty() {
            "*NONE".into()
        } else {
            self.aggrscl.join(" ")
        }
    }

    /// The rows `DSPLIND` shows, `(keyword, label, value)`, in the
    /// capture's order; a blank keyword is a derived value. `netctl`
    /// is the network controller's name (blank for none) and `msgq`
    /// the resolved queue as `LIB/NAME`.
    #[must_use]
    pub fn display_rows(&self, netctl: &str, msgq: &str) -> Vec<(String, String, String)> {
        let yes = |b: bool| if b { "*YES" } else { "*NO" }.to_string();
        let (q_lib, q_name) = msgq.split_once('/').unwrap_or(("", msgq));
        let mut rows: Vec<(&str, &str, String)> = vec![
            ("RSRCNAME", "Resource name", self.rsrcname.clone()),
            ("BRIDGE", "Bridge identifier", self.bridge.clone()),
            ("ONLINE", "Online at IPL", yes(self.online)),
            ("VRYWAIT", "Vary on wait", self.vrywait_text()),
            ("NETCTL", "Network controller", netctl.to_string()),
            ("ADPTADR", "Local adapter address", self.adptadr.clone()),
            ("EXCHID", "Exchange identifier", self.exchid.clone()),
            ("ETHSTD", "Ethernet standard", self.ethstd.clone()),
            ("LINESPEED", "Line speed", self.linespeed.clone()),
            ("", "Current line speed", self.linespeed.clone()),
            ("DUPLEX", "Duplex", self.duplex.clone()),
            ("", "Current duplex", self.duplex.clone()),
            (
                "SRVOPT",
                "Serviceability options",
                self.extras
                    .get("SRVOPT")
                    .cloned()
                    .unwrap_or_else(|| "*NONE".into()),
            ),
            ("MAXFRAME", "Maximum frame size", self.maxframe.to_string()),
            ("MAXCTL", "Maximum controllers", self.maxctl.to_string()),
            ("THRESHOLD", "Error threshold level", self.threshold.clone()),
            ("GENTSTFRM", "Generate test frame", yes(self.gentstfrm)),
            ("MSGQ", "Message queue", self.msgq.clone()),
            ("", "Current message queue", q_name.to_string()),
            ("", "  Library", q_lib.to_string()),
            (
                "TEXT",
                "Text",
                if self.text.is_empty() {
                    "*BLANK".into()
                } else {
                    self.text.clone()
                },
            ),
        ];
        if self.is_aggregate() {
            rows.insert(
                1,
                (
                    "AGGPCY",
                    "Aggregate policy",
                    format!("{} {}", self.aggpcy.0, self.aggpcy.1),
                ),
            );
            rows.insert(
                2,
                ("AGGRSCL", "Aggregated resource list", self.aggrscl_text()),
            );
        }
        rows.push(("AUTOCRTCTL", "Autocreate controller", yes(self.autocrtctl)));
        rows.push((
            "AUTODLTCTL",
            "Autodelete controller",
            self.autodltctl_text(),
        ));
        rows.push(("CMNRCYLMT", "Recovery limits", self.cmnrcylmt.text()));
        rows.into_iter()
            .map(|(k, l, v)| (k.to_string(), l.to_string(), v))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setters_validate() {
        let mut l = Lind::default();
        l.set("RSRCNAME", "cmn01").unwrap();
        assert_eq!(l.rsrcname, "CMN01");
        l.set("MAXFRAME", "8996").unwrap();
        assert!(l.set("MAXFRAME", "100").unwrap_err().starts_with("CPD0085"));
        l.set("CMNRCYLMT", "(2 5)").unwrap();
        assert_eq!(
            l.cmnrcylmt,
            RcyLimit::Limit {
                count: 2,
                interval: 5
            }
        );
        l.set("ADPTADR", "020000000001").unwrap();
        assert!(l.set("ADPTADR", "zz").is_err());
        l.set("AGGPCY", "(*LNKAGG *SRCDESTP)").unwrap();
        l.set("AGGRSCL", "CMN01 CMN02").unwrap();
        assert_eq!(l.aggrscl, ["CMN01", "CMN02"]);
        l.set("MSGQ", "QSYSOPR").unwrap();
        assert_eq!(l.msgq, "*LIBL/QSYSOPR");
        l.set("SSAP", "((04 1496))").unwrap();
        assert_eq!(l.extras["SSAP"], "((04 1496))");
        l.set("TEXT", "*SAME").unwrap();
        assert_eq!(l.text, "");
    }

    #[test]
    fn display_follows_the_capture() {
        let l = Lind {
            rsrcname: "CMN04".into(),
            ..Lind::default()
        };
        let rows = l.display_rows("LINETNET02", "QSYS/QSYSOPR");
        let labels: Vec<&str> = rows.iter().map(|(_, l, _)| l.as_str()).collect();
        assert_eq!(
            &labels[..8],
            &[
                "Resource name",
                "Bridge identifier",
                "Online at IPL",
                "Vary on wait",
                "Network controller",
                "Local adapter address",
                "Exchange identifier",
                "Ethernet standard"
            ]
        );
        assert_eq!(rows[4].2, "LINETNET02");
        assert_eq!(
            rows[13],
            (
                "MAXFRAME".to_string(),
                "Maximum frame size".to_string(),
                "1496".to_string()
            )
        );
    }

    #[test]
    fn block_encodings_and_resolution() {
        let mut l = Lind::default();
        // CHGLINETH's *SAME of a number, CMNRCYLMT's *SYSVAL.
        l.set("MAXFRAME", "-99").unwrap();
        assert_eq!(l.maxframe, 1496);
        l.set("CMNRCYLMT", "(1 3)").unwrap();
        l.set("CMNRCYLMT", "-1").unwrap();
        assert_eq!(l.cmnrcylmt, RcyLimit::SysVal);
        assert!(l.set("LINESPEED", "40G").is_err());
        let c = l.line_config((2, 5), "QSYS/QSYSOPR");
        assert_eq!((c.rcy_count, c.rcy_interval), (2, 5));
        assert_eq!(c.msgq, "QSYS/QSYSOPR");
        l.set("CMNRCYLMT", "(0 0)").unwrap();
        l.set("MSGQ", "*SYSOPR").unwrap();
        let c = l.line_config((2, 5), "QUSRSYS/CFGQ");
        assert_eq!((c.rcy_count, c.msgq.as_str()), (0, "QSYS/QSYSOPR"));
        l.set("MSGQ", "*SYSVAL").unwrap();
        assert_eq!(l.resolved_msgq("QUSRSYS/CFGQ"), "QUSRSYS/CFGQ");
        l.set("MSGQ", "MYLIB/MYQ").unwrap();
        assert_eq!(l.resolved_msgq(""), "MYLIB/MYQ");
    }
}
