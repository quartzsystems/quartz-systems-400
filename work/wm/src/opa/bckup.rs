// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The backup options (`CHGBCKUP`), kept in `QUSRSYS/QEZBCKOPT`: for each
//! of the daily, weekly and monthly backups, where it goes, what it
//! saves and how; and what `RUNBCKUP` runs for one of them.

use std::collections::BTreeMap;

use super::{decode_kv, encode_kv, yes_no, yn};

/// The user space that holds the options.
pub const OBJECT: (&str, &str) = ("QUSRSYS", "QEZBCKOPT");

/// Which backup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Which {
    /// `*DAILY`.
    Daily,
    /// `*WEEKLY`.
    Weekly,
    /// `*MONTHLY`.
    Monthly,
}

impl Which {
    /// All three.
    pub const ALL: [Self; 3] = [Self::Daily, Self::Weekly, Self::Monthly];

    /// Parse `*DAILY`, `*WEEKLY`, `*MONTHLY`.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*DAILY" => Some(Self::Daily),
            "*WEEKLY" => Some(Self::Weekly),
            "*MONTHLY" => Some(Self::Monthly),
            _ => None,
        }
    }

    /// The keyword value.
    #[must_use]
    pub fn kwd(self) -> &'static str {
        match self {
            Self::Daily => "*DAILY",
            Self::Weekly => "*WEEKLY",
            Self::Monthly => "*MONTHLY",
        }
    }

    /// The word the displays use.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
        }
    }
}

/// What the libraries, folders or directories of a backup are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sel {
    /// `*ALLUSR`: every user one.
    AllUsr,
    /// `*BCKUPL`: the backup list's.
    List,
    /// `*NONE`.
    None,
}

impl Sel {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_uppercase().as_str() {
            "*ALLUSR" | "*ALL" => Some(Self::AllUsr),
            "*BCKUPL" => Some(Self::List),
            "*NONE" => Some(Self::None),
            _ => None,
        }
    }

    /// The keyword value.
    #[must_use]
    pub fn kwd(self) -> &'static str {
        match self {
            Self::AllUsr => "*ALLUSR",
            Self::List => "*BCKUPL",
            Self::None => "*NONE",
        }
    }

    /// The words the display uses.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::AllUsr => "All",
            Self::List => "From list",
            Self::None => "None",
        }
    }
}

/// One backup's options.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // IBM's yes-or-no options
pub struct Options {
    /// `DEV`: up to four devices; empty before one is chosen.
    pub dev: Vec<String>,
    /// `TAPSET`: the tape sets rotated, up to seven.
    pub tapset: Vec<String>,
    /// `CLRTAP`.
    pub clrtap: bool,
    /// `SBMJOB`.
    pub sbmjob: bool,
    /// `CHGONLY`.
    pub chgonly: bool,
    /// `PRTRPT`.
    pub prtrpt: bool,
    /// `LIB`.
    pub lib: Sel,
    /// `FLR`.
    pub flr: Sel,
    /// `DIR`.
    pub dir: Sel,
    /// `SECDTA`.
    pub secdta: bool,
    /// `CFG`.
    pub cfg: bool,
    /// `MAIL`.
    pub mail: bool,
    /// `CAL`.
    pub cal: bool,
    /// `EXITPGM`: `*NONE` or `LIB/NAME`.
    pub exitpgm: String,
    /// IPL the system after the backup (the "Change Backup Schedule"
    /// choice).
    pub ipl_after: bool,
    /// The last run, Unix seconds; 0 for never.
    pub last: u64,
    /// The tape set the last run used.
    pub last_tapset: String,
}

impl Options {
    /// The shipped options of `w`.
    #[must_use]
    pub fn shipped(w: Which) -> Self {
        let sets: &[&str] = match w {
            Which::Daily => &["DAYA", "DAYB", "DAYC"],
            Which::Weekly => &["WEEKA", "WEEKB"],
            Which::Monthly => &["MONTHA", "MONTHB"],
        };
        let full = w != Which::Daily;
        Self {
            dev: Vec::new(),
            tapset: sets.iter().map(|s| (*s).to_string()).collect(),
            clrtap: true,
            sbmjob: true,
            chgonly: !full,
            prtrpt: true,
            lib: Sel::List,
            flr: Sel::List,
            dir: Sel::AllUsr,
            secdta: full,
            cfg: full,
            mail: full,
            cal: full,
            exitpgm: "*NONE".into(),
            ipl_after: false,
            last: 0,
            last_tapset: String::new(),
        }
    }

    /// The tape set the next run uses: the one after the last, round.
    #[must_use]
    pub fn next_tapset(&self) -> String {
        let at = self.tapset.iter().position(|t| *t == self.last_tapset);
        let i = at.map_or(0, |i| (i + 1) % self.tapset.len().max(1));
        self.tapset.get(i).cloned().unwrap_or_default()
    }

    fn put(&self, kv: &mut BTreeMap<String, String>, p: &str) {
        let mut put = |k: &str, v: String| {
            kv.insert(format!("{p}.{k}"), v);
        };
        put("dev", self.dev.join(","));
        put("tapset", self.tapset.join(","));
        put("clrtap", yn(self.clrtap).into());
        put("sbmjob", yn(self.sbmjob).into());
        put("chgonly", yn(self.chgonly).into());
        put("prtrpt", yn(self.prtrpt).into());
        put("lib", self.lib.kwd().into());
        put("flr", self.flr.kwd().into());
        put("dir", self.dir.kwd().into());
        put("secdta", yn(self.secdta).into());
        put("cfg", yn(self.cfg).into());
        put("mail", yn(self.mail).into());
        put("cal", yn(self.cal).into());
        put("exitpgm", self.exitpgm.clone());
        put("ipl", yn(self.ipl_after).into());
        put("last", self.last.to_string());
        put("lasttap", self.last_tapset.clone());
    }

    fn get(kv: &BTreeMap<String, String>, p: &str, w: Which) -> Self {
        let mut o = Self::shipped(w);
        let g = |k: &str| kv.get(&format!("{p}.{k}"));
        let list = |v: &String| -> Vec<String> {
            v.split(',').filter(|s| !s.is_empty()).map(str::to_string).collect()
        };
        if let Some(v) = g("dev") {
            o.dev = list(v);
        }
        if let Some(v) = g("tapset") {
            o.tapset = list(v);
        }
        for (k, slot) in [
            ("clrtap", &mut o.clrtap),
            ("sbmjob", &mut o.sbmjob),
            ("chgonly", &mut o.chgonly),
            ("prtrpt", &mut o.prtrpt),
            ("secdta", &mut o.secdta),
            ("cfg", &mut o.cfg),
            ("mail", &mut o.mail),
            ("cal", &mut o.cal),
            ("ipl", &mut o.ipl_after),
        ] {
            if let Some(b) = g(k).and_then(|v| yes_no(v)) {
                *slot = b;
            }
        }
        for (k, slot) in [("lib", &mut o.lib), ("flr", &mut o.flr), ("dir", &mut o.dir)] {
            if let Some(s) = g(k).and_then(|v| Sel::parse(v)) {
                *slot = s;
            }
        }
        if let Some(v) = g("exitpgm") {
            o.exitpgm.clone_from(v);
        }
        o.last = g("last").and_then(|v| v.parse().ok()).unwrap_or(0);
        o.last_tapset = g("lasttap").cloned().unwrap_or_default();
        o
    }
}

/// All three backups' options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backup {
    /// Daily, weekly, monthly.
    pub sets: [Options; 3],
}

impl Default for Backup {
    fn default() -> Self {
        Self {
            sets: Which::ALL.map(Options::shipped),
        }
    }
}

/// A value `CHGBCKUP` refuses: the keyword and the value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bad(pub &'static str, pub String);

/// The values of a list parameter, `*SAME` when it is not given.
fn same(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("*SAME"))
}

impl Backup {
    /// One backup's options.
    #[must_use]
    pub fn of(&self, w: Which) -> &Options {
        &self.sets[w as usize]
    }

    /// One backup's options, to change.
    pub fn of_mut(&mut self, w: Which) -> &mut Options {
        &mut self.sets[w as usize]
    }

    /// Decode, shipped options for what is missing.
    #[must_use]
    pub fn decode(text: &str) -> Self {
        let kv = decode_kv(text);
        Self {
            sets: Which::ALL.map(|w| Options::get(&kv, w.key(), w)),
        }
    }

    /// Encode.
    #[must_use]
    pub fn encode(&self) -> String {
        let mut kv = BTreeMap::new();
        for w in Which::ALL {
            self.of(w).put(&mut kv, w.key());
        }
        encode_kv(&kv)
    }

    /// `CHGBCKUP BCKUPOPT(w)`: `get` gives each keyword's value as
    /// typed (a list's elements space-separated); `*SAME` or nothing
    /// keeps the option. `*ALL` changes the three.
    pub fn apply(&mut self, which: &[Which], get: impl Fn(&str) -> Option<String>) -> Result<(), Bad> {
        let mut out = self.clone();
        for w in which {
            let o = out.of_mut(*w);
            if let Some(v) = same(get("DEV")) {
                let devs: Vec<String> = v.split_whitespace().map(str::to_ascii_uppercase).collect();
                if devs.len() > 4 || devs.iter().any(|d| qsname::Name::parse(d).is_err()) {
                    return Err(Bad("DEV", v));
                }
                o.dev = devs;
            }
            if let Some(v) = same(get("TAPSET")) {
                let sets: Vec<String> = v.split_whitespace().map(str::to_ascii_uppercase).collect();
                if sets.len() > 7 || sets.iter().any(|s| s.len() > 4 || s.starts_with('*')) {
                    return Err(Bad("TAPSET", v));
                }
                o.tapset = sets;
            }
            for (k, slot) in [
                ("CLRTAP", &mut o.clrtap),
                ("SBMJOB", &mut o.sbmjob),
                ("CHGONLY", &mut o.chgonly),
                ("PRTRPT", &mut o.prtrpt),
                ("SECDTA", &mut o.secdta),
                ("CFG", &mut o.cfg),
                ("MAIL", &mut o.mail),
                ("CAL", &mut o.cal),
            ] {
                if let Some(v) = same(get(k)) {
                    *slot = yes_no(&v).ok_or(Bad(k, v))?;
                }
            }
            for (k, slot) in [("LIB", &mut o.lib), ("FLR", &mut o.flr), ("DIR", &mut o.dir)] {
                if let Some(v) = same(get(k)) {
                    *slot = Sel::parse(&v).ok_or(Bad(k, v))?;
                }
            }
            if let Some(v) = same(get("EXITPGM")) {
                o.exitpgm = v.to_ascii_uppercase();
            }
        }
        *self = out;
        Ok(())
    }

    /// The keyword form of one backup's options, for the prompt.
    #[must_use]
    pub fn keyword_form(&self, w: Which) -> String {
        let o = self.of(w);
        let list = |v: &[String]| if v.is_empty() { "*NONE".to_string() } else { v.join(" ") };
        format!(
            "BCKUPOPT({}) DEV({}) TAPSET({}) CLRTAP({}) SBMJOB({}) CHGONLY({}) PRTRPT({}) \
             LIB({}) FLR({}) DIR({}) SECDTA({}) CFG({}) MAIL({}) CAL({}) EXITPGM({})",
            w.kwd(),
            list(&o.dev),
            list(&o.tapset),
            yn(o.clrtap),
            yn(o.sbmjob),
            yn(o.chgonly),
            yn(o.prtrpt),
            o.lib.kwd(),
            o.flr.kwd(),
            o.dir.kwd(),
            yn(o.secdta),
            yn(o.cfg),
            yn(o.mail),
            yn(o.cal),
            o.exitpgm,
        )
    }
}

/// One save `RUNBCKUP` runs, as a command string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    /// The command.
    pub cmd: String,
    /// It needs the restricted state.
    pub restricted: bool,
}

/// The saves of a backup to `dev`, in IBM's order: security data,
/// configuration, the libraries, the directories. `user_libs` are the
/// user libraries `*ALLUSR` and the backup list stand for here.
#[must_use]
pub fn plan(o: &Options, dev: &str, user_libs: &[String]) -> Vec<Step> {
    let mut v = Vec::new();
    if o.secdta {
        v.push(Step {
            cmd: format!("SAVSECDTA DEV({dev})"),
            restricted: false,
        });
    }
    if o.cfg {
        v.push(Step {
            cmd: format!("SAVCFG DEV({dev})"),
            restricted: false,
        });
    }
    if o.lib != Sel::None && !user_libs.is_empty() {
        let libs = if o.lib == Sel::AllUsr {
            "*ALLUSR".to_string()
        } else {
            user_libs.join(" ")
        };
        let verb = if o.chgonly { "SAVCHGOBJ OBJ(*ALL)" } else { "SAVLIB" };
        v.push(Step {
            cmd: format!("{verb} LIB({libs}) DEV({dev})"),
            restricted: o.lib == Sel::AllUsr,
        });
    }
    if o.dir != Sel::None {
        v.push(Step {
            cmd: format!("SAV DEV('/QSYS.LIB/{dev}.DEVD') OBJ(('/*') ('/QSYS.LIB' *OMIT) ('/QDLS' *OMIT))"),
            restricted: false,
        });
    }
    // The first save starts the tape (cleared when asked); the others
    // follow it, and all but the last leave it where it is.
    let n = v.len();
    for (i, s) in v.iter_mut().enumerate() {
        if i == 0 {
            if o.clrtap {
                s.cmd.push_str(" CLEAR(*ALL)");
            }
        } else {
            s.cmd.push_str(" SEQNBR(*END)");
        }
        if i + 1 < n {
            s.cmd.push_str(" ENDOPT(*LEAVE)");
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get<'a>(kv: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |k| kv.iter().find(|(x, _)| *x == k).map(|(_, v)| (*v).to_string())
    }

    #[test]
    fn shipped_daily_options() {
        let b = Backup::default();
        let d = b.of(Which::Daily);
        assert_eq!(d.tapset, ["DAYA", "DAYB", "DAYC"]);
        assert!(d.clrtap && d.chgonly && !d.secdta);
        assert_eq!(d.lib.word(), "From list");
        assert!(b.of(Which::Weekly).secdta);
    }

    #[test]
    fn options_persist() {
        let mut b = Backup::default();
        b.apply(&[Which::Weekly], get(&[("DEV", "tap01 TAP02"), ("LIB", "*ALLUSR"), ("CFG", "*NO")]))
            .unwrap();
        b.of_mut(Which::Weekly).ipl_after = true;
        b.of_mut(Which::Weekly).last = 1_792_540_800;
        b.of_mut(Which::Weekly).last_tapset = "WEEKA".into();
        let back = Backup::decode(&b.encode());
        assert_eq!(back, b);
        let w = back.of(Which::Weekly);
        assert_eq!(w.dev, ["TAP01", "TAP02"]);
        assert_eq!(w.lib, Sel::AllUsr);
        assert!(!w.cfg && w.ipl_after);
        assert_eq!(w.next_tapset(), "WEEKB");
        assert_eq!(back.of(Which::Daily), &Options::shipped(Which::Daily), "daily untouched");
        assert_eq!(Backup::decode(""), Backup::default());
    }

    #[test]
    fn a_bad_value_changes_nothing() {
        let mut b = Backup::default();
        let r = b.apply(&Which::ALL, get(&[("CLRTAP", "*NO"), ("SECDTA", "*MAYBE")]));
        assert_eq!(r, Err(Bad("SECDTA", "*MAYBE".into())));
        assert_eq!(b, Backup::default());
    }

    #[test]
    fn the_saves_of_a_backup() {
        let mut o = Options::shipped(Which::Weekly);
        o.lib = Sel::AllUsr;
        let p = plan(&o, "TAP01", &["APP".into()]);
        let cmds: Vec<&str> = p.iter().map(|s| s.cmd.as_str()).collect();
        assert_eq!(cmds[0], "SAVSECDTA DEV(TAP01) CLEAR(*ALL) ENDOPT(*LEAVE)");
        assert_eq!(cmds[1], "SAVCFG DEV(TAP01) SEQNBR(*END) ENDOPT(*LEAVE)");
        assert_eq!(cmds[2], "SAVLIB LIB(*ALLUSR) DEV(TAP01) SEQNBR(*END) ENDOPT(*LEAVE)");
        assert!(p[2].restricted, "*ALLUSR needs the restricted state");
        assert!(cmds[3].starts_with("SAV DEV('/QSYS.LIB/TAP01.DEVD')"));
        let d = plan(&Options::shipped(Which::Daily), "TAP01", &["APP".into(), "WEB".into()]);
        assert_eq!(d[0].cmd, "SAVCHGOBJ OBJ(*ALL) LIB(APP WEB) DEV(TAP01) CLEAR(*ALL) ENDOPT(*LEAVE)");
        assert!(!d[0].restricted);
    }
}
