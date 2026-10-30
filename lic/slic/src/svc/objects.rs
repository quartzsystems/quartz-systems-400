// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Object commands of the service console: libraries, generic object
//! operations, the data types, and the create/delete bench. `QTEMP/`
//! names go to the temporary ASP.

use std::fmt::Write as _;
use std::time::Instant;

use qsname::Name;
use store::Store;
use store::obj::dtaara::{self, DataAreaType, DataAreaValue, Decimal};
use store::obj::dtaq::{self, Order, Select};
use store::obj::header::text_of;
use store::obj::library::Library;
use store::obj::{
    CreateParams, JobHandle, ObjectId, ops, type_by_code, type_by_name, types, usridx, usrspc,
};

use crate::ipl::system::{System, now_unix};

/// The console's job handle for locks.
const JOB: JobHandle = JobHandle(1);
/// Objects per transaction in the bench.
const BENCH_BATCH: u64 = 1000;
/// Libraries the bench spreads objects over.
const BENCH_LIBS: u64 = 10;
/// Checkpoint after this many batches, so a power pull mid-bench has
/// transactions to replay.
const CHECKPOINT_EVERY: u64 = 4;

/// The store and root library a library name lives in.
fn target<'a>(system: &'a mut System, lib: &str) -> Result<(&'a mut Store, ObjectId), String> {
    if lib.eq_ignore_ascii_case("QTEMP") {
        let t = system.temp.as_mut().ok_or("QTEMP is not available")?;
        return Ok((&mut t.store, t.qtemp));
    }
    let s = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = s.qsys.ok_or("object store not initialized")?;
    Ok((&mut s.store, qsys))
}

/// `LIB/NAME` split and validated.
fn qualified(s: &str) -> Result<(String, Name), String> {
    let (lib, name) = s
        .split_once('/')
        .ok_or_else(|| format!("{s}: use LIB/NAME"))?;
    if lib.eq_ignore_ascii_case("*CURLIB") {
        // The job's current library; QGPL for *CRTDFT.
        let cur = super::ile::current_libl()
            .and_then(|l| l.cur)
            .unwrap_or_else(|| "QGPL".into());
        let name = Name::parse(name).map_err(|e| e.to_string())?;
        return Ok((cur, name));
    }
    Name::parse(lib).map_err(|e| e.to_string())?;
    let name = Name::parse(name).map_err(|e| e.to_string())?;
    Ok((lib.to_ascii_uppercase(), name))
}

/// The library `lib` in `root`'s store: QTEMP is its own root.
fn library(txn: &mut store::Txn<'_>, root: ObjectId, lib: &str) -> Result<ObjectId, String> {
    if lib.eq_ignore_ascii_case("QTEMP") || lib.eq_ignore_ascii_case("QSYS") && root.asp != 0 {
        return Ok(root);
    }
    let name = Name::parse(lib).map_err(|e| e.to_string())?;
    ops::find_library(txn, root, name)
        .map(|f| f.id)
        .map_err(|e| e.to_string())
}

fn find(
    txn: &mut store::Txn<'_>,
    root: ObjectId,
    lib: &str,
    name: Name,
    type_code: u16,
) -> Result<ObjectId, String> {
    let lib_id = library(txn, root, lib)?;
    let l = Library::load(txn, lib_id).map_err(|e| e.to_string())?;
    l.lookup(txn, type_code, name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "{lib}/{name} {} not found",
                type_by_code(type_code).map_or("?", |t| t.name)
            )
        })
}

fn type_code(s: &str) -> Result<u16, String> {
    let s = if s.starts_with('*') {
        s.to_string()
    } else {
        format!("*{s}")
    };
    type_by_name(&s)
        .map(|t| t.code)
        .ok_or_else(|| format!("unknown type {s}"))
}

fn params(library: ObjectId, name: Name, type_code: u16, text: &str) -> CreateParams {
    CreateParams {
        library,
        name,
        type_code,
        attribute: String::new(),
        text: text.to_string(),
        owner: "QSECOFR".into(),
        system: "QS400".into(),
        now: now_unix(),
        by_system: false,
    }
}

/// `lib ls`: every library of ASP 1 and QTEMP, EBCDIC order.
pub fn lib_ls(system: &mut System) -> String {
    let mut out = String::from("Library     Objects  Text\n");
    let mut roots: Vec<&str> = vec!["QSYS"];
    if system.temp.is_some() {
        roots.push("QTEMP");
    }
    for root_name in roots {
        let (store, root) = match target(system, root_name) {
            Ok(t) => t,
            Err(e) => {
                let _ = writeln!(out, "{e}");
                continue;
            }
        };
        let mut txn = match store.begin() {
            Ok(t) => t,
            Err(e) => return format!("{e}\n"),
        };
        let libs = if root_name == "QTEMP" {
            vec![(Name::parse("QTEMP").expect("valid"), root)]
        } else {
            ops::libraries(&mut txn, root).unwrap_or_default()
        };
        for (name, id) in libs {
            match (ops::describe(&mut txn, id), Library::load(&mut txn, id)) {
                (Ok(h), Ok(l)) => {
                    let _ = writeln!(
                        out,
                        "{:<11} {:>7}  {}",
                        name,
                        l.attr.object_count,
                        text_of(&h.text)
                    );
                }
                _ => {
                    let _ = writeln!(out, "{name:<11} ?");
                }
            }
        }
        txn.abort();
    }
    out
}

/// `obj ...`.
pub fn obj(system: &mut System, args: &[&str]) -> String {
    let r = match args {
        ["ls", lib] => obj_ls(system, lib, None),
        ["ls", lib, t] => type_code(t).and_then(|t| obj_ls(system, lib, Some(t))),
        ["crt", t, qual] => obj_crt(system, t, qual, ""),
        ["crt", t, qual, text @ ..] => obj_crt(system, t, qual, &text.join(" ")),
        ["dlt", t, qual] => obj_dlt(system, t, qual),
        ["rnm", t, qual, new] => obj_rnm(system, t, qual, new),
        ["mov", t, qual, to] => obj_mov(system, t, qual, to),
        ["dsp", qual] => obj_dsp(system, qual),
        ["bench", n] => n.parse::<u64>().map_err(|e| e.to_string()).and_then(|n| bench(system, n)),
        _ => Err("Usage: obj ls LIB [TYPE] | crt TYPE LIB/NAME [TEXT] | dlt TYPE LIB/NAME | rnm TYPE LIB/NAME NEW | mov TYPE LIB/NAME TOLIB | dsp LIB/NAME | bench N".into()),
    };
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}

fn obj_ls(system: &mut System, lib: &str, t: Option<u16>) -> Result<String, String> {
    let (store, root) = target(system, lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let lib_id = library(&mut txn, root, lib)?;
    let entries = ops::list(&mut txn, lib_id, t).map_err(|e| e.to_string())?;
    let mut out = format!(
        "Object      Type      Attribute  Text ({} in {})\n",
        entries.len(),
        lib.to_ascii_uppercase()
    );
    for (k, id) in entries {
        let h = ops::describe(&mut txn, id).map_err(|e| e.to_string())?;
        let _ = writeln!(
            out,
            "{:<11} {:<9} {:<10} {}{}",
            k.name,
            type_by_code(k.type_code).map_or("?", |t| t.name),
            text_of(&h.attribute),
            text_of(&h.text),
            if h.damaged { " *DAMAGED" } else { "" }
        );
    }
    txn.abort();
    Ok(out)
}

/// `CRTDTAQ` with its attributes: the order, the entry length, whether
/// entries keep their sender.
pub fn crt_dtaq(system: &mut System, qual: &str, text: &str, order: Order, max_len: u32, sender: bool) -> Result<String, String> {
    let (lib, name) = qualified(qual)?;
    let (store, root) = target(system, &lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let lib_id = library(&mut txn, root, &lib)?;
    let p = params(lib_id, name, types::DTAQ, text);
    let id = dtaq::create(&mut txn, &p, types::DTAQ, order, max_len, sender).map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(format!("{lib}/{} *DTAQ created as {id}
", p.name))
}

fn obj_crt(system: &mut System, t: &str, qual: &str, text: &str) -> Result<String, String> {
    let code = type_code(t)?;
    let (lib, name) = qualified(qual)?;
    let (store, root) = target(system, &lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let lib_id = library(&mut txn, root, &lib)?;
    let p = params(lib_id, name, code, text);
    let id = match code {
        types::DTAARA => dtaara::create(
            &mut txn,
            &p,
            DataAreaType::Char { len: 2000 },
            &DataAreaValue::Char(String::new()),
        ),
        types::DTAQ => dtaq::create(&mut txn, &p, types::DTAQ, Order::Fifo, 1024, false),
        types::USRQ => dtaq::create(&mut txn, &p, types::USRQ, Order::Fifo, 1024, false),
        types::USRSPC => usrspc::create(&mut txn, &p, 4096, true, 0),
        types::USRIDX => usridx::create(&mut txn, &p, false, 10, 100),
        _ => ops::create(&mut txn, &p),
    }
    .map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(format!(
        "{lib}/{} {} created as {id}\n",
        p.name,
        type_by_code(code).map_or("?", |t| t.name)
    ))
}

fn obj_dlt(system: &mut System, t: &str, qual: &str) -> Result<String, String> {
    let code = type_code(t)?;
    let (lib, name) = qualified(qual)?;
    let (store, root) = target(system, &lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let id = find(&mut txn, root, &lib, name, code)?;
    if code == types::USRIDX {
        usridx::drop_entries(&mut txn, id).map_err(|e| e.to_string())?;
    }
    ops::delete(&mut txn, JOB, id).map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(format!("{lib}/{name} deleted\n"))
}

fn obj_rnm(system: &mut System, t: &str, qual: &str, new: &str) -> Result<String, String> {
    let code = type_code(t)?;
    let (lib, name) = qualified(qual)?;
    let new_name = Name::parse(new).map_err(|e| e.to_string())?;
    let (store, root) = target(system, &lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let id = find(&mut txn, root, &lib, name, code)?;
    ops::rename(&mut txn, id, new_name, now_unix()).map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(format!("{lib}/{name} renamed to {new_name}\n"))
}

fn obj_mov(system: &mut System, t: &str, qual: &str, to: &str) -> Result<String, String> {
    let code = type_code(t)?;
    let (lib, name) = qualified(qual)?;
    let (store, root) = target(system, &lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let id = find(&mut txn, root, &lib, name, code)?;
    let to_id = library(&mut txn, root, to)?;
    ops::move_to(&mut txn, id, to_id, now_unix()).map_err(|e| e.to_string())?;
    txn.commit().map_err(|e| e.to_string())?;
    Ok(format!(
        "{lib}/{name} moved to {}\n",
        to.to_ascii_uppercase()
    ))
}

fn obj_dsp(system: &mut System, qual: &str) -> Result<String, String> {
    let (lib, name) = qualified(qual)?;
    let (store, root) = target(system, &lib)?;
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    let lib_id = library(&mut txn, root, &lib)?;
    let mut out = String::new();
    for (k, id) in ops::list(&mut txn, lib_id, None).map_err(|e| e.to_string())? {
        if k.name != name {
            continue;
        }
        let h = ops::describe(&mut txn, id).map_err(|e| e.to_string())?;
        let extents = ops::all_extents(&mut txn, &h).map_err(|e| e.to_string())?;
        let _ = write!(
            out,
            "Object  . . . . . . :  {}\n\
             Library . . . . . . :  {}\n\
             Type  . . . . . . . :  {}\n\
             Attribute . . . . . :  {}\n\
             Text  . . . . . . . :  {}\n\
             Owner . . . . . . . :  {}\n\
             Creator . . . . . . :  {}\n\
             System  . . . . . . :  {}\n\
             Release . . . . . . :  {}\n\
             Created . . . . . . :  {}\n\
             Changed . . . . . . :  {}\n\
             Public authority  . :  {}\n\
             Size  . . . . . . . :  {} page(s) in {} extent(s)\n\
             Domain  . . . . . . :  {}\n\
             Damaged . . . . . . :  {}\n\
             Identifier  . . . . :  {}\n",
            h.name,
            lib.to_ascii_uppercase(),
            type_by_code(h.id.type_code).map_or("?", |t| t.name),
            text_of(&h.attribute),
            text_of(&h.text),
            text_of(&h.owner),
            text_of(&h.creator),
            text_of(&h.system),
            h.release,
            h.created,
            h.changed,
            h.public_authority.display(),
            h.size_pages,
            extents.len(),
            match h.domain {
                store::obj::header::Domain::System => "*SYSTEM",
                store::obj::header::Domain::User => "*USER",
            },
            if h.damaged { "*YES" } else { "*NO" },
            h.id
        );
    }
    txn.abort();
    if out.is_empty() {
        return Err(format!("{lib}/{name} not found"));
    }
    Ok(out)
}

/// `obj bench N`: N data areas over ten libraries, created then deleted.
#[allow(clippy::cast_precision_loss)] // rates for display
fn bench(system: &mut System, n: u64) -> Result<String, String> {
    if n == 0 {
        return Err("obj bench N with N > 0".into());
    }
    let (store, root) = target(system, "QSYS")?;
    let mut libs = Vec::new();
    {
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        for i in 0..BENCH_LIBS {
            let name = Name::parse(&format!("QBENCH{i}")).expect("valid");
            let id = match ops::find_library(&mut txn, root, name) {
                Ok(f) => f.id,
                Err(_) => ops::create(&mut txn, &params(root, name, types::LIB, "Bench library"))
                    .map_err(|e| e.to_string())?,
            };
            libs.push(id);
        }
        txn.commit().map_err(|e| e.to_string())?;
    }
    // Leftovers of an interrupted run go first, untimed.
    delete_bench_objects(store, &libs)?;
    let start = Instant::now();
    let mut created = 0;
    while created < n {
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        let end = (created + BENCH_BATCH).min(n);
        for i in created..end {
            let name = Name::parse(&format!("B{i:08}")).expect("valid");
            let lib = libs[usize::try_from(i % BENCH_LIBS).expect("small")];
            dtaara::create(
                &mut txn,
                &params(lib, name, types::DTAARA, "Bench"),
                DataAreaType::Char { len: 10 },
                &DataAreaValue::Char("x".into()),
            )
            .map_err(|e| format!("create {i}: {e}"))?;
        }
        txn.commit().map_err(|e| e.to_string())?;
        if (end / BENCH_BATCH).is_multiple_of(CHECKPOINT_EVERY) {
            store.checkpoint().map_err(|e| e.to_string())?;
        }
        created = end;
    }
    let create_secs = start.elapsed().as_secs_f64();
    let start = Instant::now();
    let deleted = delete_bench_objects(store, &libs)?;
    let delete_secs = start.elapsed().as_secs_f64();
    Ok(format!(
        "created {n} in {create_secs:.2}s ({:.0}/s); deleted {deleted} in {delete_secs:.2}s ({:.0}/s)\n",
        n as f64 / create_secs.max(1e-9),
        deleted as f64 / delete_secs.max(1e-9)
    ))
}

/// Delete every bench data area in `libs`, a batch per transaction.
fn delete_bench_objects(store: &mut Store, libs: &[ObjectId]) -> Result<u64, String> {
    let mut deleted = 0u64;
    for lib in libs {
        loop {
            let mut txn = store.begin().map_err(|e| e.to_string())?;
            let batch: Vec<ObjectId> = ops::list(&mut txn, *lib, Some(types::DTAARA))
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|(k, _)| k.name.to_string().starts_with('B'))
                .map(|(_, id)| id)
                .take(usize::try_from(BENCH_BATCH).expect("small"))
                .collect();
            if batch.is_empty() {
                txn.abort();
                break;
            }
            for id in &batch {
                ops::delete(&mut txn, JOB, *id).map_err(|e| e.to_string())?;
            }
            txn.commit().map_err(|e| e.to_string())?;
            deleted += batch.len() as u64;
            if (deleted / BENCH_BATCH).is_multiple_of(CHECKPOINT_EVERY) {
                store.checkpoint().map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(deleted)
}

/// `dtaara get|set LIB/NAME [value]`.
pub fn dtaara_cmd(system: &mut System, args: &[&str]) -> String {
    let r = (|| -> Result<String, String> {
        let (op, qual, rest) = match args {
            [op, qual, rest @ ..] => (*op, *qual, rest),
            _ => return Err("Usage: dtaara get|set LIB/NAME [value]".into()),
        };
        let (lib, name) = qualified(qual)?;
        let (store, root) = target(system, &lib)?;
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        let id = find(&mut txn, root, &lib, name, types::DTAARA)?;
        let (kind, locked) = dtaara::describe(&mut txn, id).map_err(|e| e.to_string())?;
        match op {
            "get" => {
                let v = dtaara::read(&mut txn, id).map_err(|e| e.to_string())?;
                txn.abort();
                Ok(format!(
                    "{lib}/{name} {kind:?}{}: {}\n",
                    if locked { " (locked)" } else { "" },
                    show_value(&v, kind)
                ))
            }
            "set" => {
                let text = rest.join(" ");
                let v = match kind {
                    DataAreaType::Char { .. } => DataAreaValue::Char(text),
                    DataAreaType::Dec { digits, decimals } => DataAreaValue::Dec(
                        Decimal::parse(&text, digits, decimals)
                            .map_err(|e| e.to_string())?
                            .scaled,
                    ),
                    DataAreaType::Lgl => DataAreaValue::Lgl(matches!(text.as_str(), "1" | "'1'")),
                };
                dtaara::write(&mut txn, id, &v).map_err(|e| e.to_string())?;
                txn.commit().map_err(|e| e.to_string())?;
                Ok(format!("{lib}/{name} set\n"))
            }
            _ => Err("Usage: dtaara get|set LIB/NAME [value]".into()),
        }
    })();
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}

fn show_value(v: &DataAreaValue, kind: DataAreaType) -> String {
    match (v, kind) {
        (DataAreaValue::Char(s), _) => format!("'{}'", s.trim_end()),
        (DataAreaValue::Dec(d), DataAreaType::Dec { decimals, .. }) => Decimal {
            scaled: *d,
            decimals,
        }
        .to_string(),
        (DataAreaValue::Dec(d), _) => d.to_string(),
        (DataAreaValue::Lgl(b), _) => {
            if *b {
                "'1'".into()
            } else {
                "'0'".into()
            }
        }
    }
}

/// `dtaq snd|rcv LIB/NAME [data]`.
pub fn dtaq_cmd(system: &mut System, args: &[&str]) -> String {
    let r = (|| -> Result<String, String> {
        let (op, qual, rest) = match args {
            [op, qual, rest @ ..] => (*op, *qual, rest),
            _ => return Err("Usage: dtaq snd|rcv LIB/NAME [data]".into()),
        };
        let (lib, name) = qualified(qual)?;
        let (store, root) = target(system, &lib)?;
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        let id = find(&mut txn, root, &lib, name, types::DTAQ)?;
        match op {
            "snd" => {
                let data = rest.join(" ");
                let seq = dtaq::send(&mut txn, id, &[], data.as_bytes(), None)
                    .map_err(|e| e.to_string())?;
                txn.commit().map_err(|e| e.to_string())?;
                Ok(format!("{lib}/{name} entry {seq} sent\n"))
            }
            "rcv" => {
                let e = dtaq::receive(&mut txn, id, &Select::Next).map_err(|e| e.to_string())?;
                txn.commit().map_err(|e| e.to_string())?;
                Ok(match e {
                    Some(e) => format!(
                        "{lib}/{name} entry {}: {}\n",
                        e.seq,
                        String::from_utf8_lossy(&e.data)
                    ),
                    None => format!("{lib}/{name} is empty\n"),
                })
            }
            _ => Err("Usage: dtaq snd|rcv LIB/NAME [data]".into()),
        }
    })();
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}

/// `usrspc rd LIB/NAME OFFSET LEN | wr LIB/NAME OFFSET TEXT`.
pub fn usrspc_cmd(system: &mut System, args: &[&str]) -> String {
    let r = (|| -> Result<String, String> {
        let (op, qual, off, rest) = match args {
            [op, qual, off, rest @ ..] => (*op, *qual, *off, rest),
            _ => {
                return Err(
                    "Usage: usrspc rd LIB/NAME OFFSET LEN | wr LIB/NAME OFFSET TEXT".into(),
                );
            }
        };
        let offset: u64 = off.parse().map_err(|_| format!("{off}: not an offset"))?;
        let (lib, name) = qualified(qual)?;
        let (store, root) = target(system, &lib)?;
        let mut txn = store.begin().map_err(|e| e.to_string())?;
        let id = find(&mut txn, root, &lib, name, types::USRSPC)?;
        match op {
            "rd" => {
                let len: usize = rest
                    .first()
                    .and_then(|l| l.parse().ok())
                    .unwrap_or(64)
                    .min(4096);
                let bytes = usrspc::read(&mut txn, id, offset, len).map_err(|e| e.to_string())?;
                txn.abort();
                let mut out = String::new();
                for (i, chunk) in bytes.chunks(16).enumerate() {
                    let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
                    let text: String = chunk
                        .iter()
                        .map(|&b| {
                            if (0x20..0x7f).contains(&b) {
                                b as char
                            } else {
                                '.'
                            }
                        })
                        .collect();
                    let _ = writeln!(
                        out,
                        "{:08x}  {:<48} {text}",
                        offset + (i * 16) as u64,
                        hex.join(" ")
                    );
                }
                Ok(out)
            }
            "wr" => {
                let data = rest.join(" ");
                usrspc::write(&mut txn, id, offset, data.as_bytes()).map_err(|e| e.to_string())?;
                txn.commit().map_err(|e| e.to_string())?;
                Ok(format!(
                    "{lib}/{name}: {} byte(s) written at {offset}\n",
                    data.len()
                ))
            }
            _ => Err("Usage: usrspc rd LIB/NAME OFFSET LEN | wr LIB/NAME OFFSET TEXT".into()),
        }
    })();
    match r {
        Ok(s) => s,
        Err(e) => format!("{e}\n"),
    }
}
