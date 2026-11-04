// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! DDM files at the console: `DSPFD` and `DLTF` of a `*FILE` of
//! attribute `DDMF` (`CRTDDMF` and `CHGDDMF` run in the job).

use std::fmt::Write as _;

use ddm::objects::DdmFile;
use store::obj::{JobHandle, ObjectId, ops, types};
use store::txn::Txn;

use super::{db, file_id, with_txn};
use crate::ipl::system::System;

/// Whether `id` is a DDM file.
pub fn is_ddmf(txn: &mut Txn<'_>, id: ObjectId) -> bool {
    ops::read_header(txn, id).is_ok_and(|(_, h)| {
        h.id.type_code == types::FILE && ccsid::decode_trimmed(&h.attribute) == mi::io::DDM_ATTRIBUTE
    })
}

/// `DLTF` of a DDM file; `None` when `qual` is not one.
pub fn dltf(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let found = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_ddmf(txn, id) {
            return Ok(false);
        }
        ops::delete(txn, JobHandle(0), id).map_err(|e| e.to_string())?;
        Ok(true)
    });
    match found {
        Ok(false) => None,
        Ok(true) => Some(Ok(format!("File {qual} deleted."))),
        Err(e) => Some(Err(e)),
    }
}

/// A row of the description as IBM prints it: dot leaders on the even
/// columns, the colon in column 48, the keyword, the value.
fn row(out: &mut String, indent: usize, label: &str, kwd: &str, value: &str) {
    let mut l = format!("{}{label} ", " ".repeat(indent));
    while l.len() < 48 {
        l.push_str(if l.len() % 2 == 0 { "." } else { " " });
    }
    let _ = writeln!(out, "{l}: {kwd:<10} {value}");
}

fn quoted(s: &str) -> String {
    if s.starts_with('\'') || s.starts_with('*') { s.to_string() } else { format!("'{s}'") }
}

/// The description IBM's `DSPFD` shows of a DDM file: the header and
/// the DDM file attributes.
pub fn describe(lib: &str, f: &DdmFile, created: &str) -> String {
    let mut out = String::from(" File Description Header\n");
    row(&mut out, 4, "File", "FILE", &f.file);
    row(&mut out, 4, "Library", "", lib);
    row(&mut out, 4, "Type of file", "", "Device");
    row(&mut out, 4, "Device type", "", "DDM");
    row(&mut out, 4, "Auxiliary storage pool ID", "", "00001");
    out.push_str(" DDM File Attributes\n");
    row(&mut out, 4, "Creation date", "", created);
    let rmt = f.remote_name().map_or_else(|| f.rmtfile.clone(), |(l, n)| if l == "*LIBL" { n } else { format!("{l}/{n}") });
    row(&mut out, 4, "Remote file", "RMTFILE", &quoted(&rmt));
    if f.rmtloc == "*RDB" {
        row(&mut out, 4, "Remote location", "RMTLOCNAME", "*RDB");
        row(&mut out, 4, "Relational database", "RDB", &f.rdb);
    } else {
        let _ = writeln!(out, "    Remote location                               RMTLOCNAME");
        row(&mut out, 6, "Name or address", "", &quoted(&f.rmtloc));
        row(&mut out, 6, "Type", "", &f.loc_type);
    }
    row(&mut out, 4, "Text 'description'", "TEXT", &f.text);
    if f.loc_type == "*IP" || f.rmtloc == "*RDB" {
        row(&mut out, 4, "TCP/IP port", "PORT", &format!("{:05}", f.port));
    } else {
        row(&mut out, 4, "APPC device description", "DEV", &f.dev);
        row(&mut out, 4, "Local location", "LCLLOCNAME", &f.lcllocname);
        row(&mut out, 4, "Mode", "MODE", &f.mode);
        row(&mut out, 4, "Remote network identifier", "RMTNETID", &f.rmtnetid);
    }
    row(&mut out, 4, "Access method", "ACCMTH", &f.accmth);
    let yn = |b: bool| if b { "*YES" } else { "*NO" };
    row(&mut out, 4, "Share open data path", "SHARE", yn(f.share));
    row(&mut out, 4, "Protected conversation", "PTCCNV", yn(f.ptccnv));
    row(&mut out, 4, "Record format level check", "LVLCHK", &f.lvlchk);
    out
}

/// `DSPFD` of a DDM file; `None` when `qual` is not one.
pub fn dspfd(system: &mut System, qual: &str) -> Option<Result<String, String>> {
    let (store, ctx) = db(system).ok()?;
    let r = with_txn(store, |txn| {
        let id = file_id(txn, ctx, qual)?;
        if !is_ddmf(txn, id) {
            return Ok(None);
        }
        let (_, h) = ops::read_header(txn, id).map_err(|e| e.to_string())?;
        let lib = ops::read_header(txn, h.library).map(|(_, l)| l.name.to_string()).unwrap_or_default();
        let payload = wm::obj::read(txn, id).map_err(|e| e.to_string())?;
        let f = DdmFile::decode(&String::from_utf8_lossy(&payload)).ok_or_else(|| format!("CPF3204 {qual} is damaged"))?;
        let c = sec::cmd::civil(h.created);
        let created = format!("{}/{}/{}", &c[5..7], &c[8..10], &c[2..4]);
        Ok(Some(describe(lib.trim(), &f, &created)))
    });
    r.transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_oracle_rows() {
        let mut f = DdmFile::new("QSTEST", "QSDDMT", "QSTEST/QSDDMR", "127.0.0.1", "*IP");
        f.text = "probe".into();
        let d = describe("QSTEST", &f, "11/03/26");
        // As the primary shows them (tools/conformance/golden/phase12/oracle/).
        assert!(d.contains("    Remote file . . . . . . . . . . . . . . . . : RMTFILE    'QSTEST/QSDDMR'"), "{d}");
        assert!(d.contains("    TCP/IP port . . . . . . . . . . . . . . . . : PORT       00446"), "{d}");
        assert!(d.contains("    Record format level check . . . . . . . . . : LVLCHK     *RMTFILE"), "{d}");
        assert!(d.contains("      Type  . . . . . . . . . . . . . . . . . . :            *IP"), "{d}");
    }
}
