// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Steps 143 to 145: time zone descriptions and CCSIDs. 143 makes a
//! description with `CRTTIMZON`, makes it the system's with `CHGSYSVAL
//! QTIMZON` (no IPL: `QUTCOFFSET` and SQL's `CURRENT TIMEZONE` follow at
//! once), changes it with `CHGTIMZON`, is refused deleting it while it
//! is `QTIMZON`'s, saves and restores it, and deletes it; 144 drives
//! `WRKTIMZON` at a TN5250 session: the views, the `>` on `QTIMZON`'s
//! zone, option 5's details and `CHGTIMZON`'s prompt with the current
//! values; 145 converts through the new CCSID tables with QSH `iconv`.

use std::time::Duration;

use crate::qemu::session::Session;
use crate::sqlqemu::{Tn, setup, sql};
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, words: &[&str]) -> Result<(), String> {
    match words.iter().find(|w| !out.contains(*w)) {
        None => Ok(()),
        Some(w) => Err(format!("{what}: no {w:?} in:\n{out}")),
    }
}

/// `QUTCOFFSET` as the service console shows it.
fn utc_offset(s: &mut Session) -> Result<String, String> {
    s.command("svc.sysval QUTCOFFSET", CMD)
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 143: a time zone description made, made the system's,
    /// changed, saved, restored and deleted.
    pub(crate) fn time_zones(&self, s: &mut Session) -> Result<String, String> {
        let shipped = s.command("WRKTIMZON TIMZON(QN0500EST*)", CMD)?;
        want(&shipped, "WRKTIMZON of the shipped Eastern zones", &["QN0500EST3", "QN0500EST2", "-05:00", "EST"])?;
        let objd = s.command("DSPOBJD OBJ(QSYS/Q0000UTC) OBJTYPE(*TIMZON)", CMD)?;
        want(&objd, "DSPOBJD of a shipped *TIMZON", &["Q0000UTC", "*TIMZON"])?;
        s.command("DLTTIMZON TIMZON(QSTZTEST)", CMD).ok();
        let made = s.command("CRTTIMZON TIMZON(QSTZTEST) OFFSET(330) STDNAME(QST 'Quartz test time') TEXT('Step 143')", CMD)?;
        want(&made, "CRTTIMZON", &["QSTZTEST"])?;
        let again = s.command("CRTTIMZON TIMZON(QSTZTEST) OFFSET(330)", CMD)?;
        want(&again, "CRTTIMZON of one that exists", &["CPF2112"])?;
        let set = s.command("CHGSYSVAL SYSVAL(QTIMZON) VALUE(QSTZTEST)", CMD)?;
        if set.contains("CPF") || set.contains("not") {
            return Err(format!("CHGSYSVAL QTIMZON: {set}"));
        }
        want(&utc_offset(s)?, "QUTCOFFSET after CHGSYSVAL", &["+0530"])?;
        want(&sql(s, "VALUES CURRENT TIMEZONE")?, "CURRENT TIMEZONE", &["53000"])?;
        let refused = s.command("DLTTIMZON TIMZON(QSTZTEST)", CMD)?;
        want(&refused, "DLTTIMZON of QTIMZON's zone", &["CPF2117"])?;
        // Daylight saving time all year but the last days of December.
        let chg = s.command(
            "CHGTIMZON TIMZON(QSTZTEST) OFFSET(-300) DSTNAME(QDT 'Quartz test daylight') \
             DSTSTR(*JAN *SUN 1 '00:00:00') DSTEND(*DEC *SUN *LAST '23:00:00')",
            CMD,
        )?;
        want(&chg, "CHGTIMZON", &["QSTZTEST"])?;
        want(&utc_offset(s)?, "QUTCOFFSET after CHGTIMZON", &["-0400"])?;
        want(&sql(s, "VALUES CURRENT TIMEZONE")?, "CURRENT TIMEZONE after CHGTIMZON", &["-40000"])?;
        s.command("CHGSYSVAL SYSVAL(QTIMZON) VALUE(Q0000UTC)", CMD)?;
        want(&utc_offset(s)?, "QUTCOFFSET back on UTC", &["+0000"])?;
        // Save, delete, restore.
        s.command("DLTF FILE(QGPL/TZSAVF)", CMD).ok();
        s.command("CRTSAVF FILE(QGPL/TZSAVF)", CMD)?;
        let saved = s.command("SAVOBJ OBJ(QSTZTEST) LIB(QSYS) DEV(*SAVF) OBJTYPE(*TIMZON) SAVF(QGPL/TZSAVF)", CMD)?;
        want(&saved, "SAVOBJ of a *TIMZON", &["CPC3722", "QSYS"])?;
        let gone = s.command("DLTTIMZON TIMZON(QSTZTEST)", CMD)?;
        want(&gone, "DLTTIMZON", &["QSTZTEST"])?;
        let none = s.command("WRKTIMZON TIMZON(QSTZTEST)", CMD)?;
        if none.contains("QSTZTEST") {
            return Err(format!("WRKTIMZON after DLTTIMZON:\n{none}"));
        }
        let restored = s.command("RSTOBJ OBJ(QSTZTEST) SAVLIB(QSYS) DEV(*SAVF) OBJTYPE(*TIMZON) SAVF(QGPL/TZSAVF)", CMD)?;
        want(&restored, "RSTOBJ of a *TIMZON", &["restored"])?;
        let back = s.command("WRKTIMZON TIMZON(QSTZTEST)", CMD)?;
        want(&back, "WRKTIMZON after RSTOBJ", &["QSTZTEST", "-05:00", "QST"])?;
        s.command("DLTTIMZON TIMZON(QSTZTEST)", CMD)?;
        s.command("DLTF FILE(QGPL/TZSAVF)", CMD).ok();
        Ok("CRTTIMZON, QTIMZON +0530 then -0400 with no IPL (QUTCOFFSET, CURRENT TIMEZONE), DLTTIMZON refused, SAVOBJ/RSTOBJ".into())
    }

    /// Step 144: `WRKTIMZON` at a TN5250 session.
    pub(crate) fn work_time_zones(&self, s: &mut Session) -> Result<String, String> {
        setup(s)?;
        let mut tn = Tn::open(self)?;
        let list = tn.cmd("WRKTIMZON", "Work with Time Zone Descriptions")?;
        want(&list, "WRKTIMZON", &["Q0000GMT", "Standard Time", "8=Update system value", "F11=Display DST name"])?;
        if !list.lines().any(|l| l.contains("> Q0000UTC")) {
            return Err(format!("QTIMZON's zone is not marked:\n{list}"));
        }
        tn.run("key f11\nsettle 1000\n")?;
        want(&tn.screen(), "F11", &["Daylight Saving Time", "BST"])?;
        tn.run("key f11\nsettle 1000\n")?;
        want(&tn.screen(), "F11 twice", &["Alternate", "Europe/London"])?;
        tn.run("key f3\nsettle 1000\nunlock\n")?;
        tn.cmd("WRKTIMZON TIMZON(QN0500EST3)", "QN0500EST3")?;
        tn.run("pick \"QN0500EST3\" 5\nkey enter\nwait \"Display Time Zone Description Details\"\n")?;
        want(&tn.screen(), "option 5", &["QN0500EST3", "-05:00", "CPX091E", "EDT"])?;
        tn.run("key pgdn\nsettle 800\n")?;
        want(&tn.screen(), "option 5, page 2", &["March", "Second", "November", "First", "60"])?;
        tn.run("key pgdn\nsettle 800\n")?;
        want(&tn.screen(), "option 5, page 3", &["America/New_York"])?;
        tn.run("key f12\nsettle 800\nkey f3\nsettle 800\nunlock\n")?;
        // The prompter shows the key parameter alone first (IBM shows the
        // values at once); Enter brings the current values.
        tn.cmd("?CHGTIMZON TIMZON(QN0500EST3)", "Change Time Zone Description")?;
        tn.run("key enter\nsettle 1500\nunlock\n")?;
        want(&tn.screen(), "CHGTIMZON's prompt", &["-300", "*MSG"])?;
        tn.run("key f3\nsettle 800\nunlock\n")?;
        tn.close()?;
        Ok("WRKTIMZON: four views, QTIMZON marked, option 5's three pages, CHGTIMZON prompted with its values".into())
    }

    /// Step 145: the new CCSID tables through QSH `iconv`.
    pub(crate) fn ccsid_tables(&self, s: &mut Session) -> Result<String, String> {
        let out = s.command(
            "addenvvar ENVVAR(QIBM_QSH_CMD_OUTPUT) VALUE(STDOUT) LEVEL(*SYS) REPLACE(*YES)",
            CMD,
        )?;
        want(&out, "addenvvar", &["CPCA980"])?;
        let wait = Duration::from_secs(60);
        let list = s.command("qsh CMD('iconv -l')", wait)?;
        want(
            &list,
            "iconv -l",
            &[
                "IBMCCSID00420", "IBMCCSID00424", "IBMCCSID00870", "IBMCCSID00875", "IBMCCSID01025", "IBMCCSID01026",
                "IBMCCSID01142", "IBMCCSID01149", "IBMCCSID61952",
            ],
        )?;
        // The euro sign into 1148 and 1142, alpha into 875, alef into
        // 424; 1148's 9F back to UTF-8.
        let cases = [
            ("\\342\\202\\254", "UTF-8", "IBM-1148", "9f"),
            ("\\342\\202\\254", "UTF-8", "IBM-1142", "5a"),
            ("\\316\\261", "UTF-8", "IBM-875", "8a"),
            ("\\327\\220", "UTF-8", "IBM-424", "41"),
            ("\\237", "IBM-1148", "UTF-8", "e2 82 ac"),
        ];
        let mut seen = Vec::new();
        for (bytes, from, to, hex) in cases {
            let r = s.command(&format!("qsh CMD('printf ''{bytes}'' | iconv -f {from} -t {to} | od -An -tx1')"), wait)?;
            want(&r, &format!("{from} to {to}"), &[hex])?;
            seen.push(format!("{to} {hex}"));
        }
        s.command("rmvenvvar ENVVAR(QIBM_QSH_CMD_OUTPUT) LEVEL(*SYS)", CMD).ok();
        Ok(format!("iconv lists the new CCSIDs; {}", seen.join(", ")))
    }
}
