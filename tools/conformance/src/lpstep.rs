// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Phase 11 QEMU step 80, the licensed-program matrix: `QS00WDS` deleted
//! and restored around the RPG compiler, and a third-party product
//! packaged with `QSPKGPRD`, installed from its save file, keyed and
//! deleted.

use std::fmt::Write as _;

use ed25519_dalek::SigningKey;
use licpgm::license::key::{self, KeyFields};

use crate::qemu::session::Session;
use crate::steps::{CMD, Suite};

fn want(out: &str, what: &str, needles: &[&str]) -> Result<(), String> {
    if needles.iter().all(|n| out.contains(n)) {
        Ok(())
    } else {
        Err(format!("{what}: expected {needles:?}:\n{out}"))
    }
}

/// The vendor's signing key: a fixed seed, as a vendor's would be kept.
fn vendor() -> SigningKey {
    SigningKey::from_bytes(&[80; 32])
}

const PRODUCT: &str = "1QS8001";

/// The third-party product: one library with a data area, a keyed
/// feature with room for one user.
fn manifest() -> String {
    let mut hex = String::new();
    for b in vendor().verifying_key().to_bytes() {
        let _ = write!(hex, "{b:02x}");
    }
    format!(
        r#"[product]
id = "{PRODUCT}"
text = "Conformance product"
library = "QS80PRD"
license_key = "{hex}"

[[option]]
option = "*BASE"
text = "Conformance product base"
msgid = "CNF0001"
libraries = [{{ name = "QS80PRD" }}]
objects = [{{ name = "*ALL", type = "*DTAARA" }}]

[[license]]
feature = "5001"
compliance = "*KEYED"
usage_limit = 1
"#
    )
}

/// The system's serial number as the product functions see it.
fn serial(s: &mut Session) -> Result<String, String> {
    let out = s.command("svc.sysval QSRLNBR", CMD)?;
    let v = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("QSRLNBR:"))
        .unwrap_or("");
    let mut v: String = v.chars().filter(char::is_ascii_alphanumeric).collect();
    v.make_ascii_uppercase();
    Ok(if v.is_empty() { "QS40001".into() } else { v })
}

// The steps share one signature so `step` can take them as fn pointers.
#[allow(clippy::unused_self)]
impl Suite {
    /// Step 80: DLTLICPGM of the compilers makes CRTBNDRPG fail with
    /// CPF0C1F and RSTLICPGM brings them back; a vendor product goes
    /// through QSPKGPRD, RSTLICPGM, ADDLICKEY and DLTLICPGM.
    pub(crate) fn licensed_programs(&self, s: &mut Session) -> Result<String, String> {
        // QS00WDS around the compiler.
        let chk = s.command("chkprdopt PRDID(QS00WDS)", CMD)?;
        if chk.contains("CPF") {
            return Err(format!("CHKPRDOPT before: {chk}"));
        }
        s.command("crtsavf FILE(QGPL/S80WDS)", CMD)?;
        let saved = s.command("savlicpgm LICPGM(QS00WDS) DEV(*SAVF) OPTION(*ALL) SAVF(QGPL/S80WDS)", CMD)?;
        if saved.contains("CPF") {
            return Err(format!("SAVLICPGM: {saved}"));
        }
        let del = s.command("dltlicpgm LICPGM(QS00WDS)", CMD)?;
        want(&del, "DLTLICPGM QS00WDS", &["CPC3D82"])?;
        let rpg = "crtbndrpg PGM(QGPL/S80PGM) SRCFILE(QGPL/QRPGLESRC) SRCMBR(S80PGM)";
        let refused = s.command(rpg, CMD)?;
        want(&refused, "CRTBNDRPG without the compiler", &["CPF0C1F", "QS00WDS"])?;
        let rsf = s.command("dspsfwrsc", CMD)?;
        if rsf.contains("QS00WDS") {
            return Err(format!("DSPSFWRSC still lists QS00WDS:\n{rsf}"));
        }
        // *BASE first, then the compiler's option, as IBM's RSTLICPGM.
        let rst = s.command("rstlicpgm LICPGM(QS00WDS) DEV(*SAVF) SAVF(QGPL/S80WDS)", CMD)?;
        want(&rst, "RSTLICPGM QS00WDS", &["CPC3D81"])?;
        let rst = s.command("rstlicpgm LICPGM(QS00WDS) DEV(*SAVF) OPTION(31) SAVF(QGPL/S80WDS)", CMD)?;
        want(&rst, "RSTLICPGM QS00WDS option 31", &["CPC3D81"])?;
        let again = s.command(rpg, CMD)?;
        if again.contains("CPF0C1F") {
            return Err(format!("CRTBNDRPG after RSTLICPGM: {again}"));
        }
        want(&s.command("dspsfwrsc", CMD)?, "DSPSFWRSC after RSTLICPGM", &["QS00WDS"])?;

        // A vendor product.
        s.command("crtlib LIB(QS80PRD)", CMD)?;
        s.command("crtdtaara DTAARA(QS80PRD/INVDTA) TYPE(*CHAR) LEN(10) VALUE('vendor')", CMD)?;
        crate::qsh::push_hex(s, "S80MAN", manifest().as_bytes())?;
        let put = s.command("svc.ifs putx /tmp/s80.toml S80MAN", CMD)?;
        want(&put, "the manifest in the IFS", &["bytes"])?;
        s.command("crtsavf FILE(QGPL/S80PRD)", CMD)?;
        let pkg = s.command("qspkgprd MANIFEST('/tmp/s80.toml') SAVF(QGPL/S80PRD)", CMD)?;
        want(&pkg, "QSPKGPRD", &["CPC0C2A", PRODUCT])?;
        // Packaged is not installed (state 30): nothing to delete yet.
        let del = s.command(&format!("dltlicpgm LICPGM({PRODUCT})"), CMD)?;
        want(&del, "DLTLICPGM of a packaged product", &["CPF3D95"])?;
        s.command("dltdtaara DTAARA(QS80PRD/INVDTA)", CMD)?;
        let gone = s.command("svc.dtaara get QS80PRD/INVDTA", CMD)?;
        if gone.contains("vendor") {
            return Err(format!("the data area is still there: {gone}"));
        }
        let rst = s.command(&format!("rstlicpgm LICPGM({PRODUCT}) DEV(*SAVF) SAVF(QGPL/S80PRD)"), CMD)?;
        want(&rst, "RSTLICPGM of the vendor product", &["CPC3D81"])?;
        want(&s.command("svc.dtaara get QS80PRD/INVDTA", CMD)?, "the product's object", &["vendor"])?;
        want(&s.command("dspsfwrsc", CMD)?, "DSPSFWRSC lists the vendor product", &[PRODUCT])?;

        // A key for one user, signed by the vendor.
        let serial = serial(s)?;
        let fields = KeyFields {
            product: PRODUCT.parse().map_err(|e| format!("{e:?}"))?,
            term: "V1".parse().map_err(|e| format!("{e:?}"))?,
            feature: licpgm::LoadId::feature("5001").map_err(|e| format!("{e:?}"))?,
            serial: serial.clone(),
            processor_group: "*ANY".into(),
            usage_limit: Some(1),
            expiration: None,
            vendor_data: String::new(),
        };
        let k = key::generate(&vendor(), &fields).map_err(|e| e.to_string())?;
        let add = s.command(
            &format!(
                "addlickey PRDID({PRODUCT}) LICTRM(V1) FEATURE(5001) SERIAL({serial}) PRCGRP(*ANY) \
                 LICKEY({}) USGLMT(1)",
                k.groups().join(" ")
            ),
            CMD,
        )?;
        want(&add, "ADDLICKEY", &["CPC9E25"])?;
        // A key for another limit does not verify.
        let bad = s.command(
            &format!(
                "addlickey PRDID({PRODUCT}) LICTRM(V1) FEATURE(5001) SERIAL({serial}) PRCGRP(*ANY) \
                 LICKEY({}) USGLMT(5)",
                k.groups().join(" ")
            ),
            CMD,
        )?;
        if bad.contains("CPC9E25") {
            return Err(format!("a key for another usage limit was accepted: {bad}"));
        }
        let info = s.command(&format!("wrklicinf PRDID({PRODUCT})"), CMD)?;
        want(&info, "WRKLICINF", &[PRODUCT, "5001"])?;
        let del = s.command(&format!("dltlicpgm LICPGM({PRODUCT})"), CMD)?;
        want(&del, "DLTLICPGM after the key", &["CPC3D82"])?;
        let rsf = s.command("dspsfwrsc", CMD)?;
        if rsf.contains(PRODUCT) {
            return Err(format!("DSPSFWRSC still lists {PRODUCT}:\n{rsf}"));
        }
        for f in ["S80WDS", "S80PRD"] {
            s.command(&format!("svc.obj dlt *SAVF QGPL/{f}"), CMD).ok();
        }
        s.command("dltlib LIB(QS80PRD)", CMD).ok();
        s.command("svc.ifs rm /tmp/s80.toml", CMD).ok();
        Ok(format!(
            "QS00WDS deleted (CRTBNDRPG CPF0C1F) and restored; {PRODUCT} packaged, installed, keyed for one user and deleted"
        ))
    }
}
