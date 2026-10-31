// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 QEMU step 81, the PTF matrix at the console: PTFs built in
//! the guest with `QSCRTPTF` and the build's key, loaded, applied
//! temporarily, removed, a prerequisite enforced, a delayed PTF applied
//! by the IPL, and a permanent apply.

use std::time::Duration;

use crate::qemu::session::Session;
use crate::steps::{CMD, SRC_IPL_COMPLETE, Suite};

fn want(out: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| out.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?}:\n{out}"))
    }
}

/// A PTF replacing `QS81/LEVEL`.
fn manifest(ptf: &str, kind: &str, prereqs: &[&str]) -> String {
    let pre: Vec<String> = prereqs.iter().map(|p| format!("\"{p}\"")).collect();
    format!(
        "ptf = \"{ptf}\"\nproduct = \"QS00SS1\"\nkind = \"{kind}\"\nprereqs = [{}]\n\
         cover = \"{ptf} sets the conformance level.\"\n\n\
         [[object]]\nlibrary = \"QS81\"\nname = \"LEVEL\"\ntype = \"*DTAARA\"\n",
        pre.join(", ")
    )
}

fn level(s: &mut Session) -> Result<String, String> {
    s.command("svc.dtaara get QS81/LEVEL", CMD)
}

/// Write `text` to `path` in the guest's IFS through the xfer area.
fn put(s: &mut Session, path: &str, name: &str, text: &str) -> Result<(), String> {
    crate::qsh::push_hex(s, name, text.as_bytes())?;
    want(&s.command(&format!("svc.ifs putx {path} {name}"), CMD)?, path, &["bytes"])
}

fn set_level(s: &mut Session, v: &str) -> Result<(), String> {
    s.command(&format!("chgdtaara DTAARA(QS81/LEVEL) VALUE('{v}')"), CMD)?;
    want(&level(s)?, "the level", &[v])
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 81: QSCRTPTF, LODPTF, APYPTF *TEMP, RMVPTF, a prerequisite,
    /// a delayed PTF applied by the IPL (C6004501), APYPTF *PERM.
    pub(crate) fn ptfs(&self, s: &mut Session) -> Result<String, String> {
        let key = std::fs::read_to_string(crate::target_dir().join("keys/dev.ed25519"))
            .map_err(|e| format!("the build's dev key: {e}"))?;
        put(s, "/tmp/s81.key", "S81KEY", key.trim())?;
        s.command("crtlib LIB(QS81)", CMD)?;
        s.command("crtdtaara DTAARA(QS81/LEVEL) TYPE(*CHAR) LEN(10) VALUE('level1')", CMD)?;
        // Each PTF is built from the object as it should become.
        for (ptf, kind, pre, v) in [
            ("QS08101", "immediate", &[][..], "level1"),
            ("QS08102", "delayed", &["QS08101"][..], "level2"),
        ] {
            set_level(s, v)?;
            let path = format!("/tmp/{ptf}.toml");
            put(s, &path, &format!("M{ptf}"), &manifest(ptf, kind, pre))?;
            let made = s.command(&format!("qscrtptf MANIFEST('{path}') KEYFILE('/tmp/s81.key')"), CMD)?;
            if made.contains("CPF") || made.contains("CPD") {
                return Err(format!("QSCRTPTF {ptf}: {made}"));
            }
        }
        set_level(s, "base")?;
        for ptf in ["QS08101", "QS08102"] {
            let loaded = s.command(
                &format!("lodptf LICPGM(QS00SS1) DEV(*SAVF) SELECT({ptf}) SAVF(QGPL/{ptf})"),
                CMD,
            )?;
            want(&loaded, "LODPTF", &["CPC3604", ptf])?;
        }
        want(
            &s.command("dspptf LICPGM(QS00SS1) SELECT(QS08101)", CMD)?,
            "DSPPTF after LODPTF",
            &["QS08101", "Loaded"],
        )?;
        let applied = s.command("apyptf LICPGM(QS00SS1) SELECT(QS08101) APY(*TEMP)", CMD)?;
        want(&applied, "APYPTF *TEMP", &["CPC3606", "Temporarily applied"])?;
        want(&level(s)?, "the level after APYPTF", &["level1"])?;
        let removed = s.command("rmvptf LICPGM(QS00SS1) SELECT(QS08101) RMV(*TEMP)", CMD)?;
        want(&removed, "RMVPTF *TEMP", &["CPC3608"])?;
        want(&level(s)?, "the level after RMVPTF", &["base"])?;
        // QS08102 needs QS08101 applied.
        let refused = s.command("apyptf LICPGM(QS00SS1) SELECT(QS08102) DELAYED(*YES)", CMD)?;
        if refused.contains("CPC3606") {
            return Err(format!("QS08102 applied without its prerequisite: {refused}"));
        }
        let again = s.command("apyptf LICPGM(QS00SS1) SELECT(QS08101) APY(*TEMP)", CMD)?;
        want(&again, "APYPTF QS08101 again", &["CPC3606"])?;
        let delayed = s.command("apyptf LICPGM(QS00SS1) SELECT(QS08102) DELAYED(*YES)", CMD)?;
        want(&delayed, "APYPTF DELAYED(*YES)", &["CPC3606", "Apply at next IPL"])?;
        want(&level(s)?, "the level before the IPL", &["level1"])?;
        // The IPL applies it.
        s.send("pwrdwn restart")?;
        s.expect("SRC C6004501", self.boot_timeout)?;
        s.expect(SRC_IPL_COMPLETE, self.boot_timeout)?;
        s.expect(crate::PROMPT, Duration::from_secs(60))?;
        want(&level(s)?, "the level after the IPL", &["level2"])?;
        want(
            &s.command("dspptf LICPGM(QS00SS1)", CMD)?,
            "DSPPTF after the IPL",
            &["QS08102", "Temporarily applied"],
        )?;
        let perm = s.command("apyptf LICPGM(QS00SS1) SELECT(QS08101) APY(*PERM)", CMD)?;
        want(&perm, "APYPTF *PERM", &["Permanently applied"])?;
        want(
            &s.command("dspptfcvr LICPGM(QS00SS1) SELECT(QS08101)", CMD)?,
            "DSPPTFCVR",
            &["sets the conformance level"],
        )?;
        // The object the PTF put there is signed; changed, it no longer
        // verifies.
        let itg = s.command("chkobjitg OBJ(QS81/*ALL) CHKSIG(*ALL)", CMD)?;
        want(&itg, "CHKOBJITG of the PTF's object", &["CPC39E0", "1 objects checked"])?;
        s.command("chgdtaara DTAARA(QS81/LEVEL) VALUE('tampered')", CMD)?;
        let itg = s.command("chkobjitg OBJ(QS81/*ALL)", CMD)?;
        want(&itg, "CHKOBJITG after a change", &["CPF39E1", "QS81/LEVEL", "signature does not match"])?;
        for f in ["/tmp/s81.key", "/tmp/QS08101.toml", "/tmp/QS08102.toml"] {
            s.command(&format!("svc.ifs rm {f}"), CMD).ok();
        }
        Ok("QSCRTPTF, LODPTF, APYPTF *TEMP and RMVPTF, a prerequisite refused, a delayed PTF applied by the IPL, APYPTF *PERM".into())
    }
}
