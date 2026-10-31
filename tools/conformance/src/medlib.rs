// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 QEMU step 76: a virtual media library and its cartridges,
//! a file per library, an optical save across volumes, the tape copies
//! and the next-volume inquiry answered from the reply list, at the
//! service console.

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

/// The job's `INQMSGRPY` from `dspjob`.
fn inqmsgrpy(s: &mut Session) -> String {
    let out = s.command("dspjob", CMD).unwrap_or_default();
    out.lines()
        .find(|l| l.contains("Inquiry message reply"))
        .and_then(|l| l.split_whitespace().last())
        .filter(|v| v.starts_with('*'))
        .unwrap_or("*RQD")
        .to_string()
}

/// The cartridges: inserted, added, mounted by a save, demounted by
/// `ENDOPT(*UNLOAD)`, ejected; then two libraries saved as a file each.
fn library(s: &mut Session) -> Result<(), String> {
    run(s, "crtdevmlb DEVD(TAPMLB76) RSRCNAME(*VRT)", &["CPC2618"])?;
    run(s, "crtimgclg IMGCLG(QS76LIB) DIR('/qs76/lib') TYPE(*TAP)", &["CPCBC01"])?;
    for v in ["QS76A", "QS76B", "QS76C"] {
        run(
            s,
            &format!("addimgclge IMGCLG(QS76LIB) FROMFILE(*NEW) TOFILE({v}) VOLNAM({v}) IMGSIZ(8)"),
            &["CPCBC03"],
        )?;
    }
    run(s, "lodimgclg IMGCLG(QS76LIB) DEV(TAPMLB76)", &["CPCBC06"])?;
    run(s, "vrycfg CFGOBJ(TAPMLB76) CFGTYPE(*DEV) STATUS(*ON)", &["CPC2605"])?;
    run(s, "wrktapctg DEV(TAPMLB76)", &["QS76A", "*INSERT", "Inserted", "Slot 1"])?;
    run(s, "savlib LIB(QS76) DEV(TAPMLB76) VOL(QS76A)", &["CPF67D2"])?;
    run(s, "addtapctg DEV(TAPMLB76) CTG(QS76A QS76B) CGY(*SHARE400)", &["CPC6776"])?;
    // Mount: the save moves the cartridge into a drive; it stays there.
    run(s, "savlib LIB(QS76) DEV(TAPMLB76) VOL(QS76A)", &["CPC3722", "QS76A"])?;
    run(s, "wrktapctg DEV(TAPMLB76) CTG(QS76A)", &["Mounted", "D1"])?;
    run(s, "wrkmlbsts", &["TAPMLB76", "D1", "*ALLOCATED", "QS76A"])?;
    // Demount: ENDOPT(*UNLOAD) sends it home.
    run(
        s,
        "rstlib SAVLIB(QS76) DEV(TAPMLB76) VOL(QS76A) RSTLIB(QS76R) ENDOPT(*UNLOAD)",
        &["CPC3703", "QS76R"],
    )?;
    run(s, "svc.dtaara get QS76R/DA1", &["library kept"])?;
    run(s, "wrktapctg DEV(TAPMLB76) CTG(QS76A)", &["Available", "Slot 1"])?;
    // Eject.
    run(s, "rmvtapctg DEV(TAPMLB76) CTG(QS76C)", &["CPC6777", "station"])?;
    run(s, "dsptapctg DEV(TAPMLB76) CTG(QS76C)", &["*EJECT", "Ejected", "Station 1"])?;
    // Two libraries, a tape file each.
    run(s, "inztap DEV(TAPMLB76) NEWVOL(QS76B) VOL(QS76B) CHECK(*NO)", &["CPC6773"])?;
    run(
        s,
        "savlib LIB(QS76 QS76R) DEV(TAPMLB76) VOL(QS76B) ENDOPT(*UNLOAD)",
        &["CPC3722", "sequence 1", "sequence 2"],
    )?;
    run(s, "dsptap DEV(TAPMLB76) VOL(QS76B) DATA(*LABELS)", &["QS76 ", "QS76R"])?;
    s.command("dltlib LIB(QS76R)", CMD)?;
    run(s, "rstlib SAVLIB(QS76R) DEV(TAPMLB76) VOL(QS76B) ENDOPT(*UNLOAD)", &["CPC3703", "QS76R"])?;
    run(s, "svc.dtaara get QS76R/DA1", &["library kept"])?;
    Ok(())
}

/// A save across two optical volumes of 1 MiB, and back.
fn optical(s: &mut Session) -> Result<(), String> {
    run(s, "crtdevopt DEVD(OPTVRT76) RSRCNAME(*VRT)", &["CPC2618"])?;
    run(s, "crtimgclg IMGCLG(QS76OPT) DIR('/qs76/opt') TYPE(*OPT)", &["CPCBC01"])?;
    for (f, v) in [("O1.ISO", "QS76O1"), ("O2.ISO", "QS76O2")] {
        run(
            s,
            &format!("addimgclge IMGCLG(QS76OPT) FROMFILE(*NEW) TOFILE({f}) VOLNAM({v}) IMGSIZ(1)"),
            &["CPCBC03"],
        )?;
    }
    run(s, "lodimgclg IMGCLG(QS76OPT) DEV(OPTVRT76)", &["CPCBC06"])?;
    run(s, "vrycfg CFGOBJ(OPTVRT76) CFGTYPE(*DEV) STATUS(*ON)", &["CPC2605"])?;
    run(s, "savlib LIB(QS76) DEV(OPTVRT76) VOL(QS76O1)", &["CPC3722", "QS76O1 QS76O2"])?;
    run(s, "svc.ifs ls /QOPT/QS76O2", &["QS76"])?;
    run(s, "rstlib SAVLIB(QS76) DEV(OPTVRT76) VOL(QS76O1) RSTLIB(QS76P)", &["CPC3703", "QS76P"])?;
    run(s, "svc.usrspc rd QS76P/BIG 1500000 3", &["end"])?;
    Ok(())
}

/// `CPYTOTAP` and `CPYFRMTAP` through a tape file; then `ENDOPT(*UNLOAD)`
/// on the virtual tape device mounts the catalog's next entry.
fn copies(s: &mut Session) -> Result<(), String> {
    run(s, "crtdevtap DEVD(TAPVRT76) RSRCNAME(*VRT)", &["CPC2618"])?;
    run(s, "crtimgclg IMGCLG(QS76TAP) DIR('/qs76/tap') TYPE(*TAP)", &["CPCBC01"])?;
    for v in ["QS76T1", "QS76T2"] {
        run(
            s,
            &format!("addimgclge IMGCLG(QS76TAP) FROMFILE(*NEW) TOFILE({v}) VOLNAM({v}) IMGSIZ(8)"),
            &["CPCBC03"],
        )?;
    }
    run(s, "lodimgclg IMGCLG(QS76TAP) DEV(TAPVRT76)", &["CPCBC06"])?;
    run(s, "vrycfg CFGOBJ(TAPVRT76) CFGTYPE(*DEV) STATUS(*ON)", &["CPC2605"])?;
    s.command("svc.sql CREATE TABLE QS76/RECS (NAME CHAR(12));", CMD)?;
    s.command("svc.sql INSERT INTO QS76/RECS VALUES ('ALPHA'), ('BRAVO'), ('CHARLIE');", CMD)?;
    s.command("svc.sql CREATE TABLE QS76/RECS2 (NAME CHAR(12));", CMD)?;
    run(s, "crttapf FILE(QS76/RECTAP) DEV(TAPVRT76) SEQNBR(*END) RCDBLKTYP(*FB)", &["CPC7301"])?;
    run(s, "dspfd FILE(QS76/RECTAP)", &["TAPF", "TAPVRT76", "*FB"])?;
    run(s, "cpytotap FROMFILE(QS76/RECS) TOFILE(QS76/RECTAP)", &["CPC6778", "3 records", "RECS"])?;
    run(s, "dsptap DEV(TAPVRT76) DATA(*LABELS)", &["RECS"])?;
    run(
        s,
        "cpyfrmtap FROMFILE(QS76/RECTAP) TOFILE(QS76/RECS2) FROMLABEL(RECS) TOMBR(*FIRST) MBROPT(*ADD)",
        &["CPC6779", "3 records"],
    )?;
    run(s, "svc.sql SELECT COUNT(*) FROM QS76/RECS2;", &["3"])?;
    run(
        s,
        "cpyfrmtap FROMFILE(QS76/RECTAP) TOFILE(*PRINT) FROMLABEL(RECS)",
        &["CPC6779", "ALPHA", "CHARLIE"],
    )?;
    run(s, "chktap DEV(TAPVRT76) ENDOPT(*UNLOAD)", &["CPC6774"])?;
    let e = run(s, "wrkimgclge IMGCLG(QS76TAP)", &["*MOUNTED"])?;
    let mounted = e.lines().find(|l| l.contains("*MOUNTED")).unwrap_or_default();
    want(mounted, "after ENDOPT(*UNLOAD)", &["QS76T2"])
}

/// The next-volume inquiry: the default reply cancels; from the reply
/// list, G goes on to the catalog's other volume.
fn reply_list(s: &mut Session) -> Result<(), String> {
    run(s, "crtimgclg IMGCLG(QS76TAP2) DIR('/qs76/tap2') TYPE(*TAP)", &["CPCBC01"])?;
    for v in ["QS76X", "QS76Y"] {
        run(
            s,
            &format!("addimgclge IMGCLG(QS76TAP2) FROMFILE(*NEW) TOFILE({v}) VOLNAM({v}) IMGSIZ(2)"),
            &["CPCBC03"],
        )?;
    }
    run(s, "lodimgclg IMGCLG(QS76TAP) OPTION(*UNLOAD)", &["CPCBC07"])?;
    run(s, "lodimgclg IMGCLG(QS76TAP2) DEV(TAPVRT76)", &["CPCBC06"])?;
    run(s, "lodimgclge IMGCLG(QS76TAP2) IMGCLGIDX(2) OPTION(*MOUNT)", &["CPCBC09"])?;
    let before = inqmsgrpy(s);
    s.command("chgjob INQMSGRPY(*DFT)", CMD)?;
    let r = run(s, "savlib LIB(QS76) DEV(TAPVRT76)", &["CPF67"]).and_then(|_| {
        s.command("addrpyle SEQNBR(7601) MSGID(CPA4075) RPY(G)", CMD)?;
        s.command("chgjob INQMSGRPY(*SYSRPYL)", CMD)?;
        run(s, "savlib LIB(QS76) DEV(TAPVRT76) SEQNBR(1) CLEAR(*ALL)", &["CPC3722", "QS76Y QS76X"])
    });
    s.command("rmvrpyle SEQNBR(7601)", CMD).ok();
    s.command(&format!("chgjob INQMSGRPY({before})"), CMD).ok();
    r?;
    run(s, "dspmsg MSGQ(QSYSOPR)", &["CPA4075"])?;
    Ok(())
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 76: a virtual media library's cartridge moves and a file per
    /// library, a save across two virtual optical volumes, `CPYTOTAP` and
    /// `CPYFRMTAP`, `ENDOPT(*UNLOAD)` on virtual tape, and the next-volume
    /// inquiry answered from the reply list.
    pub(crate) fn media_library(&self, s: &mut Session) -> Result<String, String> {
        s.command("crtlib LIB(QS76)", CMD)?;
        s.command("crtdtaara DTAARA(QS76/DA1) TYPE(*CHAR) LEN(20) VALUE('library kept')", CMD)?;
        s.command("svc.obj crt *USRSPC QS76/BIG", CMD)?;
        s.command("svc.usrspc wr QS76/BIG 1500000 end", CMD)?;
        let r = library(s).and_then(|()| optical(s)).and_then(|()| copies(s)).and_then(|()| reply_list(s));
        for l in ["QS76", "QS76R", "QS76P"] {
            s.command(&format!("dltlib LIB({l})"), CMD).ok();
        }
        for c in ["QS76LIB", "QS76OPT", "QS76TAP", "QS76TAP2"] {
            s.command(&format!("lodimgclg IMGCLG({c}) OPTION(*UNLOAD)"), CMD).ok();
            s.command(&format!("dltimgclg IMGCLG({c}) KEEP(*NO)"), CMD).ok();
        }
        for d in ["TAPMLB76", "OPTVRT76", "TAPVRT76"] {
            s.command(&format!("vrycfg CFGOBJ({d}) CFGTYPE(*DEV) STATUS(*OFF)"), CMD).ok();
            s.command(&format!("dltdevd DEVD({d})"), CMD).ok();
        }
        r?;
        Ok("cartridge moves in a media library, a file per library, optical across two volumes, CPYTOTAP/CPYFRMTAP, ENDOPT(*UNLOAD), the reply list".into())
    }
}
