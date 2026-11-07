// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Step 92: the `Ifs` service (version 5) through the real connection
//! job, as `QAPTEST`: a directory made, text written in pieces from
//! UTF-8 into a CCSID 37 file and into a mixed 939 one, read back in
//! small converted chunks that join into the text, listed, copied (its
//! CCSID kept, `Exists` without `replace`), renamed; a directory
//! `*PUBLIC *EXCLUDE` the console made is `NotAuthorized`; `/QSYS.LIB`
//! listed; a directory not empty is `Error`; everything removed.

use qap_proto::ops::ifs::{
    CreateDirectory, Entries, Entry, EntryKind, FileData, FromTo, ListDirectory, PathRef, ReadFile,
    WriteFile, WriteMode, Written,
};
use qap_proto::{Envelope, Op, Status};

use super::client::{Conn, brief, msgid};

/// The step's directory, under `/tmp`.
pub const DIR: &str = "/tmp/qap92";
/// A directory the console makes `*PUBLIC *EXCLUDE`.
pub const EXCLUDED: &str = "/qap92x";
/// Text CCSID 37 holds.
const TEXT: &str = "Grüße aus Zürich: [1 + 1] = 2, ¡olé!\nZweite Zeile, äöü ÄÖÜ ß.\n";
/// Mixed text for CCSID 939.
const JAPANESE: &str = "QS/400 は日本語を変換します。\nｶﾀｶﾅ ABC 東京都　千代田区\n";

fn path(name: &str) -> String {
    format!("{DIR}/{name}")
}

/// A reply's status, which must be `want`.
fn status(r: &Envelope, want: Status, what: &str) -> Result<(), String> {
    if r.status == want {
        Ok(())
    } else {
        Err(format!("{what}: {} where {want:?} was due", brief(r)))
    }
}

fn mkdir(c: &mut Conn, p: &str, parents: bool) -> Result<Envelope, String> {
    c.call(
        Op::CreateDirectory,
        &CreateDirectory {
            path: p.into(),
            parents,
        },
    )
}

fn delete(c: &mut Conn, p: &str) -> Result<Envelope, String> {
    c.call(Op::Delete, &PathRef { path: p.into() })
}

fn info(c: &mut Conn, p: &str) -> Result<Envelope, String> {
    c.call(Op::GetInfo, &PathRef { path: p.into() })
}

fn list(c: &mut Conn, p: &str, pattern: &str) -> Result<Envelope, String> {
    c.call(
        Op::ListDirectory,
        &ListDirectory {
            path: p.into(),
            pattern: pattern.into(),
            start: 0,
            max: 0,
        },
    )
}

fn from_to(c: &mut Conn, op: Op, from: &str, to: &str, replace: bool) -> Result<Envelope, String> {
    c.call(
        op,
        &FromTo {
            from: from.into(),
            to: to.into(),
            replace,
        },
    )
}

/// `text` written from UTF-8 into a new file of `ccsid`, a few
/// characters a request: the file's size after the last.
fn write_pieces(c: &mut Conn, p: &str, ccsid: u16, text: &str) -> Result<u64, String> {
    let chars: Vec<char> = text.chars().collect();
    let mut size = 0;
    for (i, piece) in chars.chunks(3).enumerate() {
        let data: String = piece.iter().collect();
        let w: Written = c
            .ok(
                Op::WriteFile,
                &WriteFile {
                    path: p.into(),
                    mode: if i == 0 {
                        WriteMode::Create
                    } else {
                        WriteMode::Append
                    },
                    offset: 0,
                    ccsid,
                    data: data.into_bytes(),
                    data_ccsid: Some(1208),
                },
            )
            .map_err(|e| format!("WriteFile {p} piece {i}: {e}"))?;
        size = w.size;
    }
    Ok(size)
}

/// `p` read from 0 in reads of `step` bytes from each `next_offset`;
/// the data joined and how many reads it took.
fn read_on(
    c: &mut Conn,
    p: &str,
    step: u64,
    as_ccsid: Option<u16>,
) -> Result<(Vec<u8>, usize), String> {
    let (mut out, mut at, mut n) = (Vec::new(), 0u64, 0);
    loop {
        let d: FileData = c.ok(
            Op::ReadFile,
            &ReadFile {
                path: p.into(),
                offset: at,
                length: step,
                as_ccsid,
            },
        )?;
        n += 1;
        if d.next_offset <= at && d.next_offset < d.size {
            return Err(format!("ReadFile {p} made no progress at {at}"));
        }
        out.extend_from_slice(&d.data);
        at = d.next_offset;
        if at >= d.size {
            return Ok((out, n));
        }
    }
}

/// A file's entry, which must be a file of `ccsid` and `size` bytes.
fn file_entry(c: &mut Conn, p: &str, ccsid: u16, size: usize) -> Result<(), String> {
    let e: Entry = c.ok(Op::GetInfo, &PathRef { path: p.into() })?;
    if e.kind != EntryKind::File
        || e.ccsid != ccsid
        || e.size != size as u64
        || e.owner != super::USER
    {
        return Err(format!(
            "GetInfo {p}: {e:?}, not a file of CCSID {ccsid}, {size} bytes, owned by {}",
            super::USER
        ));
    }
    Ok(())
}

/// What a previous run left goes.
fn clean(c: &mut Conn) -> Result<(), String> {
    for p in ["sub/r37.txt", "sub", "t37.txt", "c37.txt", "j939.txt"] {
        let _ = delete(c, &path(p))?;
    }
    let _ = delete(c, DIR)?;
    Ok(())
}

/// The step's body, signed on as `USER`.
#[allow(clippy::too_many_lines)]
pub fn run(c: &mut Conn) -> Result<String, String> {
    clean(c)?;
    status(
        &mkdir(c, &path("sub"), true)?,
        Status::Ok,
        "CreateDirectory with parents",
    )?;
    let again = mkdir(c, DIR, false)?;
    if again.status != Status::Exists || msgid(&again) != "CPFA0A0" {
        return Err(format!(
            "CreateDirectory of an existing directory: {}",
            brief(&again)
        ));
    }
    // UTF-8 written in pieces into a CCSID 37 file.
    let t37 = path("t37.txt");
    let stored = ccsid::conv::from_str(37, TEXT).map_err(|e| e.to_string())?;
    let size = write_pieces(c, &t37, 37, TEXT)?;
    if size != stored.len() as u64 {
        return Err(format!(
            "WriteFile sizes: {size}, the text is {} bytes in CCSID 37",
            stored.len()
        ));
    }
    file_entry(c, &t37, 37, stored.len())?;
    let (raw, _) = read_on(c, &t37, 0, None)?;
    if raw != stored {
        return Err(format!(
            "{t37} as stored: {raw:02X?}, not the text in CCSID 37"
        ));
    }
    let (utf8, reads) = read_on(c, &t37, 5, Some(1208))?;
    if utf8 != TEXT.as_bytes() {
        return Err(format!(
            "{t37} read as 1208 by 5: {:?}",
            String::from_utf8_lossy(&utf8)
        ));
    }
    // Mixed: Japanese written in pieces into CCSID 939, read back by 3
    // bytes, boundaries inside its pairs.
    let j939 = path("j939.txt");
    let mixed = ccsid::conv::from_str(939, JAPANESE).map_err(|e| e.to_string())?;
    write_pieces(c, &j939, 939, JAPANESE)?;
    file_entry(c, &j939, 939, mixed.len())?;
    let (raw, _) = read_on(c, &j939, 0, None)?;
    if raw != mixed {
        return Err(format!(
            "{j939} in pieces: {raw:02X?}, one write makes {mixed:02X?}"
        ));
    }
    let (jp, jreads) = read_on(c, &j939, 3, Some(1208))?;
    if jp != JAPANESE.as_bytes() {
        return Err(format!(
            "{j939} read as 1208 by 3: {:?}",
            String::from_utf8_lossy(&jp)
        ));
    }
    // Listed.
    let l: Entries = list(c, DIR, "*")?.body_as().map_err(|e| e.to_string())?;
    let names: Vec<&str> = l.entries.iter().map(|e| e.name.as_str()).collect();
    if l.total != 3 || names != ["j939.txt", "sub", "t37.txt"] {
        return Err(format!("ListDirectory {DIR}: {} {names:?}", l.total));
    }
    // Copied, its CCSID kept; again without replace is Exists.
    let c37 = path("c37.txt");
    status(
        &from_to(c, Op::Copy, &t37, &c37, false)?,
        Status::Ok,
        "Copy",
    )?;
    file_entry(c, &c37, 37, stored.len())?;
    let twice = from_to(c, Op::Copy, &t37, &c37, false)?;
    if twice.status != Status::Exists {
        return Err(format!(
            "Copy onto a file without replace: {}",
            brief(&twice)
        ));
    }
    status(
        &from_to(c, Op::Copy, &j939, &c37, true)?,
        Status::Ok,
        "Copy with replace",
    )?;
    file_entry(c, &c37, 939, mixed.len())?;
    // Renamed into the subdirectory.
    let r37 = path("sub/r37.txt");
    status(
        &from_to(c, Op::Rename, &c37, &r37, false)?,
        Status::Ok,
        "Rename",
    )?;
    status(
        &info(c, &c37)?,
        Status::NotFound,
        "GetInfo of the renamed file",
    )?;
    file_entry(c, &r37, 939, mixed.len())?;
    // The console's directory, *PUBLIC *EXCLUDE.
    let no = list(c, EXCLUDED, "*")?;
    if no.status != Status::NotAuthorized || msgid(&no) != "CPFA09C" {
        return Err(format!("ListDirectory {EXCLUDED}: {}", brief(&no)));
    }
    let no_mk = mkdir(c, &format!("{EXCLUDED}/x"), false)?;
    status(
        &no_mk,
        Status::NotAuthorized,
        "CreateDirectory in the excluded directory",
    )?;
    // /QSYS.LIB.
    let q: Entries = list(c, "/QSYS.LIB", "QGPL.LIB")?
        .body_as()
        .map_err(|e| e.to_string())?;
    if q.total != 1 || q.entries.first().map(|e| e.kind) != Some(EntryKind::Directory) {
        return Err(format!("ListDirectory /QSYS.LIB QGPL.LIB: {q:?}"));
    }
    // A directory not empty.
    let full = delete(c, &path("sub"))?;
    if full.status != Status::Error || msgid(&full) != "CPFA0AC" {
        return Err(format!("Delete of a directory not empty: {}", brief(&full)));
    }
    // Everything goes.
    for p in [
        r37.clone(),
        path("sub"),
        t37.clone(),
        j939.clone(),
        DIR.to_string(),
    ] {
        status(&delete(c, &p)?, Status::Ok, &format!("Delete {p}"))?;
    }
    status(
        &info(c, DIR)?,
        Status::NotFound,
        "GetInfo of the deleted directory",
    )?;
    Ok(format!(
        "{} bytes of UTF-8 written in pieces into CCSID 37 and read back in {reads} converted reads, Japanese into 939 in pieces as one write makes it and back in {jreads} reads of 3 bytes; listed, copied (CCSID kept, then Exists), renamed; {EXCLUDED} NotAuthorized (CPFA09C); /QSYS.LIB listed; a full directory Error (CPFA0AC); all deleted",
        TEXT.len()
    ))
}
