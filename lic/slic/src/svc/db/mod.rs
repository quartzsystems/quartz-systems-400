// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Database commands of the service console: files, members, copies,
//! displays, journals, commitment control, overrides, SQL, the scripted
//! record-level driver and the bench. Dev-side text moves through the
//! `xfer` area: named line buffers pushed and pulled over the serial line.

mod bench;
mod files;
mod journal;
pub(crate) mod prtf;
mod rla;
mod sqlcmd;

use std::collections::BTreeMap;
use std::fmt::Write as _;

use qsname::Name;
use sql::SqlContext;
use sql::ast::{Naming, ObjectName};
use store::Store;
use store::obj::ops;
use store::obj::{JobHandle, ObjectId};
use store::txn::Txn;

use crate::ipl::system::{System, now_unix};

/// The console's job handle.
pub const JOB: JobHandle = JobHandle(1);

/// Help for the database commands.
pub const HELP: &str = "\
xfer put NAME N      store the next N lines as NAME; xfer get NAME | ls | rm NAME
crtpf LIB/FILE SRCFILE(LIB/SRC) [SRCMBR(M)] | RCDLEN(n)  [MAXMBRS(n) REUSEDLT(*YES) TEXT('..')]
crtlf LIB/FILE SRCFILE(LIB/SRC) [SRCMBR(M)] | crtsrcpf LIB/FILE [RCDLEN(n)]
dltf LIB/FILE | addpfm LIB/FILE MBR [TEXT('..') SRCTYPE(T)] | rmvm LIB/FILE MBR
clrpfm LIB/FILE [MBR(M)] | rgzpfm LIB/FILE [MBR(M)] [KEYFILE(*FILE|LIB/LF)]
cpyf FROMFILE(LIB/F) TOFILE(LIB/F) [FROMMBR TOMBR MBROPT(*ADD|*REPLACE|*UPDADD) CRTFILE(*YES)
     FMTOPT(*MAP|*DROP|*MAP *DROP|*NOCHK|*CVTSRC) FROMRCD TORCD NBRRCDS ERRLVL(n|*NOMAX)]
cpytoimpf FROMFILE(LIB/F) [TOSTMF(NAME) FROMMBR DTAFMT(*DLM|*FIXED) RCDDLM(*LF) RMVBLANK(*TRAILING)
          STRDLM(*NONE) FLDDLM(;) DATFMT(*USA) TIMFMT(*ISO) NULLS(*YES)]
cpyfrmimpf FROMSTMF(NAME) TOFILE(LIB/F) [TOMBR same options ERRLVL(n)]
dspfd LIB/FILE [TYPE(*ALL|*BASATR|*ATR|*ACCPTH|*MBRLIST|*RCDFMT|*SELECT|*JOIN)]
dspffd LIB/FILE | dspdbr LIB/FILE [MBR(M)]
dsppfm LIB/FILE [MBR(M) FROMRCD(n) NBRRCDS(n) OUTPUT(*HEX) DELETED(*YES)]
srcput LIB/FILE MBR [SRCTYPE(T) FROMSTMF(NAME)] | srcget LIB/FILE MBR [TOSTMF(NAME)]
crtjrnrcv LIB/RCV [THRESHOLD(kb)] | crtjrn LIB/JRN JRNRCV(LIB/RCV) [MNGRCV(*USER)]
chgjrn LIB/JRN [JRNRCV(*GEN|LIB/RCV)] | dltjrn LIB/JRN | dltjrnrcv LIB/RCV | dspjrnrcva LIB/RCV
strjrnpf LIB/FILE JRN(LIB/JRN) [IMAGES(*BOTH) OMTJRNE(*OPNCLO)] | endjrnpf LIB/FILE
dspjrn LIB/JRN [FILE(LIB/F) JRNCDE(R) ENTTYP(PT) FROMENT(n) TOENT(n) JOB(n)]
apyjrnchg|rmvjrnchg LIB/JRN [FILE(LIB/F) FROMENT(n) TOENT(n) CMTBDY(*YES)]
strcmtctl [LCKLVL(*CHG|*CS|*ALL) DFTJRN(LIB/JRN)] | commit | rollbck | endcmtctl
ovrdbf FILE(F) [TOFILE(LIB/F) MBR(M) SHARE(*YES) LVLCHK(*NO) INHWRT(*YES) POSITION(*START|*END)]
dltovr FILE(F|*ALL)
crtprtf LIB/FILE [SRCFILE(LIB/SRC) SRCMBR(M)] [PAGESIZE(l w) LPI() CPI() OVRFLW() CTLCHAR(*FCFC) OUTQ() HOLD() SCHEDULE() ...]
chgprtf LIB/FILE keywords | ovrprtf FILE(F) [TOFILE(LIB/F) OVRSCOPE(*JOB)] keywords
sql [STATEMENT;]     SQL mode: statements end with ';', quit leaves | sqlrun NAME [COMMIT(*CHG) ERRLVL(n) NAMING(*SQL)]
naming *SYS|*SQL | curlib LIB
rla [NAME]           run an RLA script from xfer, or type lines until 'end'
dbbench N            insert, read and update N rows of a 5-field PF with 2 LFs
";

/// What the console is in the middle of.
#[derive(Debug, Default)]
pub enum Mode {
    /// Commands.
    #[default]
    Normal,
    /// Storing lines into the xfer area.
    Capture {
        /// Buffer name.
        name: String,
        /// Lines still to come.
        remaining: usize,
        /// Lines so far.
        lines: Vec<String>,
    },
    /// SQL statements, `;` terminated.
    Sql {
        /// Text so far.
        buffer: String,
    },
    /// RLA script lines until `end`.
    Rla {
        /// Lines so far.
        lines: Vec<String>,
    },
}

/// The console's database state.
#[derive(Default)]
pub struct State {
    /// What is being typed.
    pub mode: Mode,
    /// Named line buffers.
    pub xfer: BTreeMap<String, String>,
    /// The SQL context (session, cursors, overrides), once storage is up.
    pub sql: Option<SqlContext>,
    /// `OVRPRTF`s in effect, oldest first.
    pub prt_overrides: Vec<crate::stgsvc::prtf::PrtOverride>,
}

/// The prompt the mode wants.
pub fn prompt(system: &System) -> &'static str {
    match system.svc.mode {
        Mode::Normal => super::PROMPT,
        Mode::Capture { .. } => "",
        Mode::Sql { .. } => "sql> ",
        Mode::Rla { .. } => "rla> ",
    }
}

/// Lines a mode consumes before the dispatcher sees them.
pub fn intercept(system: &mut System, line: &str) -> Option<String> {
    match std::mem::take(&mut system.svc.mode) {
        Mode::Normal => None,
        Mode::Capture {
            name,
            remaining,
            mut lines,
        } => {
            lines.push(line.to_string());
            if remaining > 1 {
                system.svc.mode = Mode::Capture {
                    name,
                    remaining: remaining - 1,
                    lines,
                };
                return Some(String::new());
            }
            let n = lines.len();
            let mut text = lines.join("\n");
            text.push('\n');
            system.svc.xfer.insert(name.clone(), text);
            Some(format!("{name}: {n} lines stored\n"))
        }
        Mode::Sql { mut buffer } => {
            let word = line.trim().to_ascii_lowercase();
            if word == "quit" || word == "exit" {
                return Some(String::new());
            }
            buffer.push_str(line);
            buffer.push('\n');
            if buffer.trim_end().ends_with(';') {
                let out = sqlcmd::run_text(system, &buffer);
                system.svc.mode = Mode::Sql {
                    buffer: String::new(),
                };
                return Some(out);
            }
            system.svc.mode = Mode::Sql { buffer };
            Some(String::new())
        }
        Mode::Rla { mut lines } => {
            if line.trim().eq_ignore_ascii_case("end") {
                return Some(rla::run(system, &lines));
            }
            lines.push(line.to_string());
            system.svc.mode = Mode::Rla { lines };
            Some(String::new())
        }
    }
}

/// Dispatch a database command; `None` when `cmd` is not one.
pub fn command(system: &mut System, cmd: &str, args: &[&str], line: &str) -> Option<String> {
    let p = Params::parse(line);
    let r = match cmd {
        "xfer" => xfer(system, args),
        "crtpf" => files::crtpf(system, &p),
        "crtsrcpf" => files::crtsrcpf(system, &p),
        "crtlf" => files::crtlf(system, &p),
        "dltf" => {
            let qual = p.pos_or(0, "FILE").unwrap_or_default();
            prtf::dltf(system, &qual).unwrap_or_else(|| files::dltf(system, &p))
        }
        "crtprtf" => prtf::crtprtf(system, &p),
        "chgprtf" => prtf::chgprtf(system, &p),
        "ovrprtf" => prtf::ovrprtf(system, line),
        "addpfm" => files::addpfm(system, &p),
        "rmvm" => files::rmvm(system, &p),
        "clrpfm" => files::clrpfm(system, &p),
        "rgzpfm" => files::rgzpfm(system, &p),
        "cpyf" => files::cpyf(system, &p),
        "cpytoimpf" => files::cpytoimpf(system, &p),
        "cpyfrmimpf" => files::cpyfrmimpf(system, &p),
        "dspfd" => {
            let qual = p.pos_or(0, "FILE").unwrap_or_default();
            prtf::dspfd(system, &qual).unwrap_or_else(|| files::dspfd(system, &p))
        }
        "dspffd" => files::dspffd(system, &p),
        "dsppfm" => files::dsppfm(system, &p),
        "dspdbr" => files::dspdbr(system, &p),
        "srcput" => files::srcput(system, &p),
        "srcget" => files::srcget(system, &p),
        "crtjrnrcv" => journal::crtjrnrcv(system, &p),
        "crtjrn" => journal::crtjrn(system, &p),
        "chgjrn" => journal::chgjrn(system, &p),
        "dltjrn" => journal::dltjrn(system, &p),
        "dltjrnrcv" => journal::dltjrnrcv(system, &p),
        "dspjrnrcva" => journal::dspjrnrcva(system, &p),
        "strjrnpf" => journal::strjrnpf(system, &p),
        "endjrnpf" => journal::endjrnpf(system, &p),
        "dspjrn" => journal::dspjrn(system, &p),
        "apyjrnchg" => journal::apply(system, &p, true),
        "rmvjrnchg" => journal::apply(system, &p, false),
        "strcmtctl" => journal::strcmtctl(system, &p),
        "commit" => journal::commit(system),
        "rollbck" => journal::rollbck(system),
        "endcmtctl" => journal::endcmtctl(system),
        "ovrdbf" => journal::ovrdbf(system, &p),
        "dltovr" => journal::dltovr(system, &p),
        "sql" => return Some(sqlcmd::sql(system, line)),
        "sqlrun" => sqlcmd::sqlrun(system, &p),
        "naming" => sqlcmd::naming(system, args),
        "curlib" => sqlcmd::curlib(system, args),
        "rla" => return Some(rla::command(system, args)),
        "dbbench" => bench::dbbench(system, args),
        _ => return None,
    };
    Some(match r {
        Ok(s) if s.ends_with('\n') || s.is_empty() => s,
        Ok(s) => format!("{s}\n"),
        Err(e) => format!("{e}\n"),
    })
}

/// `xfer put NAME N | get NAME | ls | rm NAME`.
fn xfer(system: &mut System, args: &[&str]) -> Result<String, String> {
    match args {
        ["put", name, n] => {
            let remaining: usize = n
                .parse()
                .map_err(|_| format!("{n}: a line count is needed"))?;
            if remaining == 0 {
                system.svc.xfer.insert((*name).to_string(), String::new());
                return Ok(format!("{name}: 0 lines stored"));
            }
            system.svc.mode = Mode::Capture {
                name: (*name).to_string(),
                remaining,
                lines: Vec::new(),
            };
            Ok(String::new())
        }
        ["get", name] => {
            let text = system
                .svc
                .xfer
                .get(*name)
                .ok_or_else(|| format!("{name}: no such buffer"))?;
            let n = text.lines().count();
            Ok(format!("--- {name} {n} lines\n{text}--- end {name}"))
        }
        ["ls"] => {
            let mut out = String::new();
            for (k, v) in &system.svc.xfer {
                let _ = writeln!(
                    out,
                    "{k:<20} {:>6} lines {:>8} bytes",
                    v.lines().count(),
                    v.len()
                );
            }
            if out.is_empty() {
                out.push_str("(empty)\n");
            }
            Ok(out)
        }
        ["rm", name] => system
            .svc
            .xfer
            .remove(*name)
            .map(|_| format!("{name} removed"))
            .ok_or_else(|| format!("{name}: no such buffer")),
        _ => Err("Usage: xfer put NAME N | get NAME | ls | rm NAME".into()),
    }
}

pub use wm::cmd::Params;

/// `LIB/NAME` or `NAME` as an object name.
pub fn object_name(s: &str) -> Result<ObjectName, String> {
    let parse = |n: &str| Name::parse(n).map_err(|e| format!("{n}: {e}"));
    if let Some((lib, name)) = s.split_once('/') {
        parse(lib)?;
        parse(name)?;
        Ok(ObjectName::qualified(lib, name))
    } else {
        parse(s)?;
        Ok(ObjectName::new(s))
    }
}

/// A member name parameter.
pub fn member_spec(p: &Params, name: &str) -> Result<qsname::MemberSpec, String> {
    match p.upper(name).as_deref() {
        None | Some("*FIRST" | "") => Ok(qsname::MemberSpec::First),
        Some("*LAST") => Ok(qsname::MemberSpec::Last),
        Some("*ALL") => Ok(qsname::MemberSpec::All),
        Some(m) => Name::parse(m)
            .map(qsname::MemberSpec::Name)
            .map_err(|e| format!("{name}({m}): {e}")),
    }
}

/// The store and the SQL context, built on first use; names resolve
/// through the job's library list as it is now.
pub fn db(system: &mut System) -> Result<(&mut Store, &mut SqlContext), String> {
    let storage = system.storage.as_mut().ok_or("object store not open")?;
    let qsys = storage.qsys.ok_or("object store not initialized")?;
    let names = {
        let mut txn = storage.store.begin().map_err(|e| e.to_string())?;
        let n = super::ile::name_context(&mut txn, qsys);
        txn.abort();
        n
    };
    match system.svc.sql.as_mut() {
        Some(ctx) => ctx.names = names,
        None => {
            system.svc.sql = Some(SqlContext::with_locks(
                qsys,
                JOB,
                names,
                system.locks.clone(),
            ));
        }
    }
    Ok((&mut storage.store, system.svc.sql.as_mut().expect("set")))
}

/// Run `f` in a transaction: committed on success, dropped on error.
pub fn with_txn<T>(
    store: &mut Store,
    f: impl FnOnce(&mut Txn<'_>) -> Result<T, String>,
) -> Result<T, String> {
    let mut txn = store.begin().map_err(|e| e.to_string())?;
    match f(&mut txn) {
        Ok(v) => {
            txn.commit().map_err(|e| e.to_string())?;
            Ok(v)
        }
        Err(e) => {
            txn.abort();
            Err(e)
        }
    }
}

/// A file by `LIB/NAME` or `NAME` through the library list.
pub fn file_id(txn: &mut Txn<'_>, ctx: &SqlContext, qual: &str) -> Result<ObjectId, String> {
    let name = object_name(qual)?;
    ctx.resolve_file(txn, &name).map_err(|e| e.to_string())
}

/// An object of `type_code` by `LIB/NAME` or `NAME`.
pub fn typed_id(
    txn: &mut Txn<'_>,
    ctx: &SqlContext,
    qual: &str,
    type_code: u16,
) -> Result<ObjectId, String> {
    let name = object_name(qual)?;
    let lib = ctx.lib_spec(&name).map_err(|e| e.to_string())?;
    let n = SqlContext::sys_name(&name.name).map_err(|e| e.to_string())?;
    ops::resolve(txn, ctx.qsys, &ctx.names, lib, n, type_code)
        .map(|f| f.id)
        .map_err(|e| format!("{qual}: {e}"))
}

/// A database error with its message ID.
pub fn db_err(e: &db::DbError) -> String {
    match e.msgid() {
        Some(id) => format!("{id} {e}"),
        None => e.to_string(),
    }
}

/// The time for created and changed stamps.
pub fn now() -> u64 {
    now_unix()
}

/// The SQL naming mode in force.
pub fn naming(system: &System) -> Naming {
    system.svc.sql.as_ref().map_or(Naming::Sys, |c| c.naming)
}
