// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `GO SAVE` options 20 to 23 and `GO RESTORE` 21 to 23: the command
//! defaults the "Specify Command Defaults" panel shows (option 20 keeps
//! them in `QUSRSYS/QEZSAVDFT`), and the commands each option runs, from
//! `ENDSBS *ALL` to the `STRSBS` of the controlling subsystem.

use std::collections::BTreeMap;

use super::{decode_kv, encode_kv, time_of_day, yes_no, yn};

/// The user space that holds the defaults.
pub const OBJECT: (&str, &str) = ("QUSRSYS", "QEZSAVDFT");

/// The panel's values.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the panel's Y/N fields
pub struct Defaults {
    /// Devices, up to four.
    pub devices: Vec<String>,
    /// Prompt for commands.
    pub prompt: bool,
    /// Check for active files on the media.
    pub check_active: bool,
    /// `*BREAK` or `*NOTIFY`.
    pub delivery: String,
    /// `*CURRENT` or `HHMM`.
    pub start: String,
    /// `*NONE`, `*ALL`, `*WINDOWSNT`, `*GUEST`.
    pub vary_off: String,
    /// Unmount file systems.
    pub unmount: bool,
    /// Print system information.
    pub print_info: bool,
    /// Use the system reply list.
    pub reply_list: bool,
    /// `*NONE` or `*ALL`.
    pub splfdta: String,
}

impl Default for Defaults {
    fn default() -> Self {
        Self {
            devices: vec!["TAP01".into()],
            prompt: true,
            check_active: true,
            delivery: "*BREAK".into(),
            start: "*CURRENT".into(),
            vary_off: "*ALL".into(),
            unmount: false,
            print_info: false,
            reply_list: false,
            splfdta: "*NONE".into(),
        }
    }
}

/// A value the panel refuses: the field and what was typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bad(pub &'static str, pub String);

fn y_n(s: &str, field: &'static str) -> Result<bool, Bad> {
    match s.trim().to_ascii_uppercase().as_str() {
        "Y" => Ok(true),
        "N" => Ok(false),
        _ => Err(Bad(field, s.into())),
    }
}

impl Defaults {
    /// Decode, the shipped values for what is missing.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let kv = decode_kv(text);
        let mut d = Self::default();
        if let Some(v) = kv.get("devices") {
            d.devices = v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect();
        }
        for (k, slot) in [
            ("prompt", &mut d.prompt),
            ("check", &mut d.check_active),
            ("unmount", &mut d.unmount),
            ("print", &mut d.print_info),
            ("rpyl", &mut d.reply_list),
        ] {
            if let Some(b) = kv.get(k).and_then(|v| yes_no(v)) {
                *slot = b;
            }
        }
        for (k, slot) in [
            ("delivery", &mut d.delivery),
            ("start", &mut d.start),
            ("varyoff", &mut d.vary_off),
            ("splfdta", &mut d.splfdta),
        ] {
            if let Some(v) = kv.get(k) {
                slot.clone_from(v);
            }
        }
        d
    }

    /// Encode.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut kv = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            kv.insert(k.to_string(), v);
        };
        put("devices", self.devices.join(","));
        put("prompt", yn(self.prompt).into());
        put("check", yn(self.check_active).into());
        put("delivery", self.delivery.clone());
        put("start", self.start.clone());
        put("varyoff", self.vary_off.clone());
        put("unmount", yn(self.unmount).into());
        put("print", yn(self.print_info).into());
        put("rpyl", yn(self.reply_list).into());
        put("splfdta", self.splfdta.clone());
        encode_kv(&kv)
    }

    /// The panel's fields, by variable: `DEV1` to `DEV4`, `PMTCMD`,
    /// `CHKACT`, `MSGDLV`, `STRTIM`, `VRYOFF`, `UNMNT`, `PRTINF`, `RPYL`,
    /// `SPLDTA`.
    #[must_use]
    pub fn fields(&self) -> Vec<(&'static str, String)> {
        let yn1 = |b: bool| if b { "Y" } else { "N" }.to_string();
        let mut v = Vec::new();
        for (i, k) in ["DEV1", "DEV2", "DEV3", "DEV4"].iter().enumerate() {
            v.push((*k, self.devices.get(i).cloned().unwrap_or_default()));
        }
        v.push(("PMTCMD", yn1(self.prompt)));
        v.push(("CHKACT", yn1(self.check_active)));
        v.push(("MSGDLV", self.delivery.clone()));
        v.push(("STRTIM", self.start.clone()));
        v.push(("VRYOFF", self.vary_off.clone()));
        v.push(("UNMNT", yn1(self.unmount)));
        v.push(("PRTINF", yn1(self.print_info)));
        v.push(("RPYL", yn1(self.reply_list)));
        v.push(("SPLDTA", self.splfdta.clone()));
        v
    }

    /// The values typed on the panel, checked.
    pub fn from_fields(get: impl Fn(&str) -> String) -> Result<Self, Bad> {
        let up = |k: &str| get(k).trim().to_ascii_uppercase();
        let devices: Vec<String> = ["DEV1", "DEV2", "DEV3", "DEV4"]
            .iter()
            .map(|k| up(k))
            .filter(|d| !d.is_empty())
            .collect();
        if devices.is_empty() {
            return Err(Bad("DEV1", String::new()));
        }
        if let Some(d) = devices.iter().find(|d| qsname::Name::parse(d).is_err()) {
            return Err(Bad("DEV1", d.clone()));
        }
        let delivery = up("MSGDLV");
        if !matches!(delivery.as_str(), "*BREAK" | "*NOTIFY") {
            return Err(Bad("MSGDLV", delivery));
        }
        let start = up("STRTIM");
        if start != "*CURRENT" && time_of_day(&format!("{start}00")).is_none() {
            return Err(Bad("STRTIM", start));
        }
        let vary_off = up("VRYOFF");
        if !matches!(vary_off.as_str(), "*NONE" | "*ALL" | "*WINDOWSNT" | "*GUEST") {
            return Err(Bad("VRYOFF", vary_off));
        }
        let splfdta = up("SPLDTA");
        if !matches!(splfdta.as_str(), "*NONE" | "*ALL") {
            return Err(Bad("SPLDTA", splfdta));
        }
        Ok(Self {
            devices,
            prompt: y_n(&up("PMTCMD"), "PMTCMD")?,
            check_active: y_n(&up("CHKACT"), "CHKACT")?,
            delivery,
            start,
            vary_off,
            unmount: y_n(&up("UNMNT"), "UNMNT")?,
            print_info: y_n(&up("PRTINF"), "PRTINF")?,
            reply_list: y_n(&up("RPYL"), "RPYL")?,
            splfdta,
        })
    }
}

/// Which menu an option is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Menu {
    /// `GO SAVE`.
    Save,
    /// `GO RESTORE`.
    Restore,
}

/// The directories of the system's own products.
const PROD: &str = "('/QIBM/ProdData') ('/QOpenSys/QIBM/ProdData')";
/// Every directory but the libraries and the folders.
const ALL_DIRS: &str = "('/*') ('/QSYS.LIB' *OMIT) ('/QDLS' *OMIT)";
/// Every user directory.
const USR_DIRS: &str =
    "('/*') ('/QSYS.LIB' *OMIT) ('/QDLS' *OMIT) ('/QIBM/ProdData' *OMIT) ('/QOpenSys/QIBM/ProdData' *OMIT)";

/// The commands option `option` of `menu` runs with `d`. `ctl` is the
/// controlling subsystem (`QCTLSBSD`); `have(name)` says whether a
/// command exists on this system (a missing one is left out).
#[must_use]
pub fn sequence(menu: Menu, option: u8, d: &Defaults, ctl: &str, have: &dyn Fn(&str) -> bool) -> Vec<String> {
    let dev = d.devices.first().cloned().unwrap_or_else(|| "TAP01".into());
    let mut v: Vec<String> = Vec::new();
    if d.start != "*CURRENT" {
        v.push(format!("DLYJOB RSMTIME({}00)", d.start));
    }
    v.push("ENDSBS SBS(*ALL) OPTION(*IMMED)".into());
    v.push(format!("CHGMSGQ MSGQ(QSYSOPR) DLVRY({}) SEV(60)", d.delivery));
    if d.reply_list {
        v.push("CHGJOB INQMSGRPY(*SYSRPYL)".into());
    }
    if d.unmount && have("UNMOUNT") {
        v.push("UNMOUNT TYPE(*ALL) MNTOVRDIR(*ALL)".into());
    }
    if d.vary_off != "*NONE" && have("VRYCFG") {
        v.push(format!("VRYCFG CFGOBJ({}) CFGTYPE(*NWS) STATUS(*OFF)", d.vary_off));
    }
    let saves: Vec<String> = match (menu, option) {
        (Menu::Save, 21 | 22) => {
            if d.print_info {
                v.push("PRTSYSINF".into());
            }
            let mut s = Vec::new();
            // BLOCKED(phase11): SAVSYS is the install agent's; until it is on
            // the system, options 21 and 22 save everything after it.
            if have("SAVSYS") {
                s.push(format!("SAVSYS DEV({dev})"));
            }
            if option == 21 {
                s.push(format!("SAVLIB LIB(*NONSYS) DEV({dev}) ACCPTH(*YES) SPLFDTA({})", d.splfdta));
                s.push(format!("SAV DEV('/QSYS.LIB/{dev}.DEVD') OBJ({ALL_DIRS}) UPDHST(*YES)"));
            } else {
                s.push(format!("SAVLIB LIB(*IBM) DEV({dev}) ACCPTH(*YES)"));
                s.push(format!("SAV DEV('/QSYS.LIB/{dev}.DEVD') OBJ({PROD}) UPDHST(*YES)"));
            }
            s
        }
        (Menu::Save, _) => {
            if d.print_info {
                v.push("PRTSYSINF".into());
            }
            vec![
                format!("SAVSECDTA DEV({dev})"),
                format!("SAVCFG DEV({dev})"),
                format!("SAVLIB LIB(*ALLUSR) DEV({dev}) ACCPTH(*YES) SPLFDTA({})", d.splfdta),
                format!("SAV DEV('/QSYS.LIB/{dev}.DEVD') OBJ({USR_DIRS}) UPDHST(*YES)"),
            ]
        }
        (Menu::Restore, _) => {
            let (libs, dirs) = match option {
                21 => ("*NONSYS", ALL_DIRS),
                22 => ("*IBM", PROD),
                _ => ("*ALLUSR", USR_DIRS),
            };
            vec![
                format!("RSTUSRPRF DEV({dev}) USRPRF(*ALL)"),
                format!("RSTCFG OBJ(*ALL) DEV({dev})"),
                format!("RSTLIB SAVLIB({libs}) DEV({dev})"),
                format!("RST DEV('/QSYS.LIB/{dev}.DEVD') OBJ({dirs})"),
                "RSTAUT USRPRF(*ALL)".into(),
            ]
        }
    };
    let n = saves.len();
    for (i, s) in saves.into_iter().enumerate() {
        let mut s = s;
        if menu == Menu::Save && !s.starts_with("SAV ") {
            // The first save starts the tape; the others follow it.
            if i == 0 {
                if !d.check_active {
                    s.push_str(" CLEAR(*ALL)");
                }
            } else {
                s.push_str(" SEQNBR(*END)");
            }
            if i + 1 < n {
                s.push_str(" ENDOPT(*LEAVE)");
            }
        }
        v.push(if d.prompt { format!("?{s}") } else { s });
    }
    v.push(format!("STRSBS SBSD({ctl})"));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all(_: &str) -> bool {
        true
    }

    fn none(_: &str) -> bool {
        false
    }

    #[test]
    fn save_21_ends_saves_and_starts() {
        let d = Defaults {
            prompt: false,
            vary_off: "*NONE".into(),
            print_info: true,
            ..Defaults::default()
        };
        let s = sequence(Menu::Save, 21, &d, "QSYS/QCTL", &all);
        assert_eq!(s[0], "ENDSBS SBS(*ALL) OPTION(*IMMED)");
        assert_eq!(s[1], "CHGMSGQ MSGQ(QSYSOPR) DLVRY(*BREAK) SEV(60)");
        assert_eq!(s[2], "PRTSYSINF");
        assert_eq!(s[3], "SAVSYS DEV(TAP01) ENDOPT(*LEAVE)");
        assert!(s[4].starts_with("SAVLIB LIB(*NONSYS) DEV(TAP01) ACCPTH(*YES) SPLFDTA(*NONE) SEQNBR(*END)"));
        assert!(s[5].starts_with("SAV DEV('/QSYS.LIB/TAP01.DEVD')"));
        assert_eq!(s.last().unwrap(), "STRSBS SBSD(QSYS/QCTL)");
        let without = sequence(Menu::Save, 21, &d, "QSYS/QCTL", &none);
        assert!(!without.iter().any(|c| c.starts_with("SAVSYS")));
    }

    #[test]
    fn save_23_prompts_each_save() {
        let d = Defaults {
            check_active: false,
            reply_list: true,
            start: "2300".into(),
            ..Defaults::default()
        };
        let s = sequence(Menu::Save, 23, &d, "QSYS/QCTL", &none);
        assert_eq!(s[0], "DLYJOB RSMTIME(230000)");
        assert!(s.contains(&"CHGJOB INQMSGRPY(*SYSRPYL)".to_string()));
        assert!(s.contains(&"?SAVSECDTA DEV(TAP01) CLEAR(*ALL) ENDOPT(*LEAVE)".to_string()));
        assert!(s.iter().any(|c| c.starts_with("?SAVLIB LIB(*ALLUSR)")));
    }

    #[test]
    fn restore_21_in_ibm_order() {
        let d = Defaults {
            prompt: false,
            ..Defaults::default()
        };
        let s = sequence(Menu::Restore, 21, &d, "QSYS/QBASE", &none);
        let verbs: Vec<&str> = s.iter().map(|c| c.split_whitespace().next().unwrap()).collect();
        assert_eq!(
            verbs,
            ["ENDSBS", "CHGMSGQ", "RSTUSRPRF", "RSTCFG", "RSTLIB", "RST", "RSTAUT", "STRSBS"]
        );
        assert!(s[4].contains("SAVLIB(*NONSYS)"));
    }

    #[test]
    fn defaults_persist_and_check() {
        let d = Defaults {
            devices: vec!["TAP01".into(), "TAP02".into()],
            unmount: true,
            splfdta: "*ALL".into(),
            ..Defaults::default()
        };
        assert_eq!(Defaults::decode(&d.encode()), d);
        let f: BTreeMap<&str, String> = d.fields().into_iter().collect();
        let back = Defaults::from_fields(|k| f.get(k).cloned().unwrap_or_default()).unwrap();
        assert_eq!(back, d);
        let mut bad = f.clone();
        bad.insert("MSGDLV", "*LOUD".into());
        assert_eq!(
            Defaults::from_fields(|k| bad.get(k).cloned().unwrap_or_default()),
            Err(Bad("MSGDLV", "*LOUD".into()))
        );
    }
}
