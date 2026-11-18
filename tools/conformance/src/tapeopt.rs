// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 QEMU step: saves and restores on virtual tape and virtual
//! optical devices over image catalogs, at the service console.

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| out.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?}:\n{out}"))
    }
}

/// Run `line` and want every needle in its output.
fn run(s: &mut Session, line: &str, needles: &[&str]) -> Result<String, String> {
    let out = s.command(line, CMD)?;
    want(&out, line, needles)?;
    Ok(out)
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 75: a virtual tape device over a `*TAP` catalog (INZTAP,
    /// SAVLIB, DSPTAP of the saves, RSTLIB to a new name), a save across
    /// two small volumes and back, and the same save on a virtual optical
    /// device, seen through `/QOPT`.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn tape_optical(&self, s: &mut Session) -> Result<String, String> {
        s.command("crtlib LIB(QS72)", CMD)?;
        s.command(
            "crtdtaara DTAARA(QS72/DA1) TYPE(*CHAR) LEN(20) VALUE('kept value')",
            CMD,
        )?;
        run(
            s,
            "crtdevtap DEVD(TAPVRT72) RSRCNAME(*VRT)",
            &["Device description TAPVRT72 is made"],
        )?;
        run(
            s,
            "crtimgclg IMGCLG(QS72TAP) DIR('/qs72/tap') TYPE(*TAP)",
            &["Image catalog QS72TAP is made in QUSRSYS"],
        )?;
        run(
            s,
            "addimgclge IMGCLG(QS72TAP) FROMFILE(*NEW) TOFILE(VOL1) VOLNAM(QS72V1) IMGSIZ(64)",
            &["has a new entry"],
        )?;
        run(
            s,
            "lodimgclg IMGCLG(QS72TAP) DEV(TAPVRT72)",
            &["Image catalog QS72TAP is ready on device TAPVRT72"],
        )?;
        run(
            s,
            "vrycfg CFGOBJ(TAPVRT72) CFGTYPE(*DEV) STATUS(*ON)",
            &["CPC2605"],
        )?;
        run(
            s,
            "wrkcfgsts CFGTYPE(*DEV) CFGD(TAPVRT72)",
            &["TAPVRT72", "VARIED ON"],
        )?;
        run(
            s,
            "inztap DEV(TAPVRT72) NEWVOL(QS72V1) CHECK(*NO)",
            &["Volume QS72V1 is initialized on device TAPVRT72"],
        )?;
        run(
            s,
            "savlib LIB(QS72) DEV(TAPVRT72)",
            &["from library QS72 were saved on QS72V1"],
        )?;
        run(
            s,
            "dsptap DEV(TAPVRT72) DATA(*SAVRST)",
            &["SAVLIB", "DA1", "QS72"],
        )?;
        run(
            s,
            "dsptap DEV(TAPVRT72) DATA(*LABELS)",
            &["QS72V1", "QS72", "EOF"],
        )?;
        run(
            s,
            "rstlib SAVLIB(QS72) DEV(TAPVRT72) RSTLIB(QS72B)",
            &["restored to library QS72B"],
        )?;
        run(s, "svc.dtaara get QS72B/DA1", &["kept value"])?;

        // Two volumes of 2 MiB: the save passes the first one's early
        // warning and goes on to the second, from the VOL list.
        s.command("svc.obj crt *USRSPC QS72/BIG", CMD)?;
        s.command("svc.usrspc wr QS72/BIG 1500000 end", CMD)?;
        run(
            s,
            "crtimgclg IMGCLG(QS72TAP2) DIR('/qs72/tap2') TYPE(*TAP)",
            &["Image catalog QS72TAP2 is made in QUSRSYS"],
        )?;
        for (file, vol) in [("V1", "QS72A"), ("V2", "QS72B")] {
            run(
                s,
                &format!(
                    "addimgclge IMGCLG(QS72TAP2) FROMFILE(*NEW) TOFILE({file}) VOLNAM({vol}) IMGSIZ(2)"
                ),
                &["has a new entry"],
            )?;
        }
        run(
            s,
            "lodimgclg IMGCLG(QS72TAP) OPTION(*UNLOAD)",
            &["Image catalog QS72TAP is off device"],
        )?;
        run(
            s,
            "lodimgclg IMGCLG(QS72TAP2) DEV(TAPVRT72)",
            &["Image catalog QS72TAP2 is ready on device TAPVRT72"],
        )?;
        run(
            s,
            "savlib LIB(QS72) DEV(TAPVRT72) VOL(QS72A QS72B)",
            &["were saved on QS72A QS72B"],
        )?;
        run(
            s,
            "dsptap DEV(TAPVRT72) VOL(QS72A) DATA(*LABELS)",
            &["QS72", "EOV"],
        )?;
        run(
            s,
            "rstlib SAVLIB(QS72) DEV(TAPVRT72) VOL(QS72A QS72B) RSTLIB(QS72C)",
            &["restored to library QS72C"],
        )?;
        run(s, "svc.usrspc rd QS72C/BIG 1500000 3", &["end"])?;
        run(
            s,
            "wrkimgclge IMGCLG(QS72TAP2)",
            &["QS72A", "QS72B", "*MOUNTED"],
        )?;

        // Virtual optical.
        run(
            s,
            "crtdevopt DEVD(OPTVRT72) RSRCNAME(*VRT)",
            &["Device description OPTVRT72 is made"],
        )?;
        run(
            s,
            "crtimgclg IMGCLG(QS72OPT) DIR('/qs72/opt') TYPE(*OPT)",
            &["Image catalog QS72OPT is made in QUSRSYS"],
        )?;
        run(
            s,
            "addimgclge IMGCLG(QS72OPT) FROMFILE(*NEW) TOFILE(OPT1.ISO) VOLNAM(QS72O) IMGSIZ(64)",
            &["has a new entry"],
        )?;
        run(
            s,
            "lodimgclg IMGCLG(QS72OPT) DEV(OPTVRT72)",
            &["Image catalog QS72OPT is ready on device OPTVRT72"],
        )?;
        run(
            s,
            "vrycfg CFGOBJ(OPTVRT72) CFGTYPE(*DEV) STATUS(*ON)",
            &["CPC2605"],
        )?;
        run(
            s,
            "inzopt VOL(QS72O) DEV(OPTVRT72) CHECK(*NO)",
            &["Optical volume QS72O is initialized on device OPTVRT72"],
        )?;
        run(
            s,
            "savlib LIB(QS72) DEV(OPTVRT72)",
            &["were saved on", "QS72O"],
        )?;
        run(
            s,
            "dspopt VOL(QS72O) DEV(OPTVRT72) DATA(*SAVRST)",
            &["/QS72", "DA1"],
        )?;
        run(s, "svc.ifs ls /QOPT/QS72O", &["QS72"])?;
        run(
            s,
            "rstlib SAVLIB(QS72) DEV(OPTVRT72) RSTLIB(QS72D)",
            &["restored to library QS72D"],
        )?;
        run(s, "svc.dtaara get QS72D/DA1", &["kept value"])?;

        for l in ["QS72", "QS72B", "QS72C", "QS72D"] {
            s.command(&format!("dltlib LIB({l})"), CMD).ok();
        }
        for c in ["QS72TAP", "QS72TAP2", "QS72OPT"] {
            s.command(&format!("lodimgclg IMGCLG({c}) OPTION(*UNLOAD)"), CMD)
                .ok();
            s.command(&format!("dltimgclg IMGCLG({c}) KEEP(*NO)"), CMD)
                .ok();
        }
        for d in ["TAPVRT72", "OPTVRT72"] {
            s.command(
                &format!("vrycfg CFGOBJ({d}) CFGTYPE(*DEV) STATUS(*OFF)"),
                CMD,
            )
            .ok();
            s.command(&format!("dltdevd DEVD({d})"), CMD).ok();
        }
        Ok("SAVLIB and RSTLIB on virtual tape (one and two volumes) and virtual optical".into())
    }
}
