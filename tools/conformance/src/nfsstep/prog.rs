// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The small IR programs the NFS steps call on a guest: no command takes
//! a byte-range lock or shows an `errno`, so a program does, through the
//! IFS APIs (`open`, `fcntl`, `read`, `pwrite`) a job's RPG would use.
//! Each is made for its path (the texts below), pushed over the console,
//! assembled with `qir asm` and bound with `CRTPGM`; it reports with
//! `DSPLY` lines and marker files beside a local path, which the steps
//! read with `svc.ifs stat`.
//!
//! - [`hold`]: a write lock on bytes `start` to `start + len - 1`, a
//!   marker `M.ok` (or `M.bad` and the `errno`), then a wait, one
//!   `DLYJOB DLY(1)` at a time, until the release file exists; then 8
//!   bytes written at offset 4096 through the same descriptor (`M.wrote`)
//!   and the close. Submitted as a batch job, it is the job holding the
//!   lock.
//! - [`try_lock`]: the same lock tried once: `NFSLK-LOCKED` (and
//!   unlocked) or `NFSLK-REFUSED` and the `errno`.
//! - [`read`]: the first bytes read and, when given, compared:
//!   `NFSRD-SAME`/`NFSRD-DIFF`, or `NFSRD-FAILED` and the `errno`, with a
//!   marker `M.eintr`, `M.etimedout` or `M.err` (`M.ok` when it read).
//! - [`write`]: bytes written at the start of a file, truncated first:
//!   `NFSWR-OK` or `NFSWR-FAILED` and the `errno`.

use std::fmt::Write as _;

use crate::qemu::session::Session;

/// `EAGAIN`: a lock another owner holds.
pub(crate) const EAGAIN: u32 = 3406;
/// `EINTR`: an `intr` mount's call ended by `ENDJOB`.
pub(crate) const EINTR: u32 = 3407;
/// `ETIMEDOUT`: a `soft` mount gave up.
pub(crate) const ETIMEDOUT: u32 = 3447;

/// The most rounds of `DLYJOB DLY(1)` a holder waits.
const HOLD_ROUNDS: u32 = 1200;

/// The prototypes the programs use (IBM's C signatures, as the IFS API
/// corpus declares them) and two helpers.
const PROTOS: &str = "\
extern pgm QCMDEXC($cmd: char(1) *VARSIZE, $len: packed(15,5)) pgm(QCMDEXC) lib(QSYS)
extern proc open(byval $p: ptr, byval $o: i32, byval $m: u32 *NOPASS, byval $c: u32 *NOPASS, byval $t: u32 *NOPASS) -> i32
extern proc close(byval $fd: i32) -> i32
extern proc read(byval $fd: i32, byval $b: ptr, byval $n: u32) -> i32
extern proc write(byval $fd: i32, byval $b: ptr, byval $n: u32) -> i32
extern proc pwrite(byval $fd: i32, byval $b: ptr, byval $n: u32, byval $o: i32) -> i32
extern proc fcntl(byval $fd: i32, byval $c: i32, byval $l: ptr) -> i32
extern proc creat(byval $p: ptr, byval $m: u32) -> i32
extern proc access(byval $p: ptr, byval $m: i32) -> i32
extern proc memcmp(byval $a: ptr, byval $b: ptr, byval $n: u32) -> i32
extern proc __errno() -> ptr
; The errno of the last call.
proc ERRNO() -> i32 {
L0:
  %ep = callb __errno()
  %e = load [%ep]:i32
  ret %e
}
; An empty marker file.
proc MARK(byval $p: ptr) {
L0:
  %q = load $p
  %fd = callb creat(%q, 438)
  %c = callb close(%fd)
  ret
}
";

/// `O_RDONLY`, `O_WRONLY`, `O_RDWR`, `O_CREAT`, `O_TRUNC` (IBM's values).
const O_RDONLY: u32 = 1;
const O_WRONLY: u32 = 2;
const O_RDWR: u32 = 4;
const O_TRUNC: u32 = 64;
/// `fcntl`'s `F_SETLK`.
const F_SETLK: u32 = 4;

/// Bytes as a hexadecimal literal's digits.
fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, c| {
        let _ = write!(s, "{c:02X}");
        s
    })
}

/// A `struct flock` (48 bytes): the type, `SEEK_SET`, start, length.
fn flock(kind: u16, start: u32, len: u32) -> String {
    let mut b = Vec::with_capacity(48);
    b.extend_from_slice(&kind.to_be_bytes());
    b.extend_from_slice(&0u16.to_be_bytes());
    b.extend_from_slice(&start.to_be_bytes());
    b.extend_from_slice(&len.to_be_bytes());
    b.resize(48, 0);
    format!("x\"{}\"", hex(&b))
}

/// The module line, the prototypes and the statics' header.
fn head(name: &str) -> String {
    format!("; made by the NFS steps (conformance::nfsstep::prog)\nmodule {name}\n")
}

/// A text as the program's source: the prototypes after the module line
/// and the `z"..."` paths as EBCDIC (the job's CCSID) with a NUL.
fn finish(text: &str) -> String {
    crate::ifsapi::expand(text, PROTOS)
}

/// A batch job's lock holder.
pub(crate) fn hold(
    name: &str,
    path: &str,
    start: u32,
    len: u32,
    release: &str,
    marker: &str,
) -> String {
    let wl = flock(2, start, len);
    finish(&format!(
        "{head}static $p: bytes(*) inz(z\"{path}\")
static $rel: bytes(*) inz(z\"{release}\")
static $mok: bytes(*) inz(z\"{marker}.ok\")
static $mbad: bytes(*) inz(z\"{marker}.bad\")
static $mwr: bytes(*) inz(z\"{marker}.wrote\")
static $wl: bytes(48) inz({wl})
static $dly: char(13) inz(\"DLYJOB DLY(1)\")
static $tag: bytes(8) inz(x\"{tag}\")
static $n: i32 inz(0)
proc {name}() entry {{
L0:
  %p = addr $p
  %fd = callb open(%p, {O_RDWR}, 0)
  %bad = cmp.lt %fd, 0
  brif %bad, FAIL, LOCK
LOCK:
  %pw = addr $wl
  %k = callb fcntl(%fd, {F_SETLK}, %pw)
  %kb = cmp.lt %k, 0
  brif %kb, FAILC, HELD
HELD:
  dsply \"NFSLK-HELD\"
  %mo = addr $mok
  callb MARK(%mo)
  br WAIT
WAIT:
  %r = addr $rel
  %a = callb access(%r, 0)
  %there = cmp.eq %a, 0
  brif %there, DONE, MORE
MORE:
  %i = load $n
  %i2 = add %i, 1
  store $n, %i2
  %over = cmp.gt %i2, {HOLD_ROUNDS}
  brif %over, DONE, SLEEP
SLEEP:
  call QCMDEXC($dly, 13)
  br WAIT
DONE:
  %t = addr $tag
  %w = callb pwrite(%fd, %t, 8, 4096)
  %wok = cmp.eq %w, 8
  brif %wok, WROTE, CLOSE
WROTE:
  %mw = addr $mwr
  callb MARK(%mw)
  br CLOSE
CLOSE:
  %c = callb close(%fd)
  dsply \"NFSLK-DONE\"
  ret
FAILC:
  %e = callb ERRNO()
  %c2 = callb close(%fd)
  br REPORT
FAIL:
  %e = callb ERRNO()
  br REPORT
REPORT:
  dsply \"NFSLK-FAILED\"
  dsply %e
  %mb = addr $mbad
  callb MARK(%mb)
  ret
}}
",
        head = head(name),
        tag = hex(b"AFTER-RS"),
    ))
}

/// One try at the lock, from the console.
pub(crate) fn try_lock(name: &str, path: &str, start: u32, len: u32) -> String {
    let wl = flock(2, start, len);
    let ul = flock(3, start, len);
    finish(&format!(
        "{head}static $p: bytes(*) inz(z\"{path}\")
static $wl: bytes(48) inz({wl})
static $ul: bytes(48) inz({ul})
proc {name}() entry {{
L0:
  %p = addr $p
  %fd = callb open(%p, {O_RDWR}, 0)
  %bad = cmp.lt %fd, 0
  brif %bad, NOOPEN, LOCK
LOCK:
  %pw = addr $wl
  %k = callb fcntl(%fd, {F_SETLK}, %pw)
  %kb = cmp.lt %k, 0
  brif %kb, REFUSED, LOCKED
LOCKED:
  dsply \"NFSLK-LOCKED\"
  %pu = addr $ul
  %u = callb fcntl(%fd, {F_SETLK}, %pu)
  br CLOSE
REFUSED:
  %e = callb ERRNO()
  dsply \"NFSLK-REFUSED\"
  dsply %e
  br CLOSE
CLOSE:
  %c = callb close(%fd)
  ret
NOOPEN:
  %e2 = callb ERRNO()
  dsply \"NFSLK-NOOPEN\"
  dsply %e2
  ret
}}
",
        head = head(name),
    ))
}

/// A read of the first bytes, compared with `expect` when given.
pub(crate) fn read(name: &str, path: &str, expect: Option<&[u8]>, marker: &str) -> String {
    let (decl, compare) = match expect {
        Some(b) => (
            format!(
                "static $want: bytes({n}) inz(x\"{h}\")\n",
                n = b.len(),
                h = hex(b)
            ),
            format!(
                "  %w = addr $want
  %d = callb memcmp(%b, %w, {n})
  %same = cmp.eq %d, 0
  brif %same, SAME, DIFF
SAME:
  dsply \"NFSRD-SAME\"
  br DONE
DIFF:
  dsply \"NFSRD-DIFF\"
  br DONE
",
                n = b.len()
            ),
        ),
        None => (String::new(), "  br DONE\n".to_string()),
    };
    finish(&format!(
        "{head}static $p: bytes(*) inz(z\"{path}\")
static $mok: bytes(*) inz(z\"{marker}.ok\")
static $mintr: bytes(*) inz(z\"{marker}.eintr\")
static $mtime: bytes(*) inz(z\"{marker}.etimedout\")
static $merr: bytes(*) inz(z\"{marker}.err\")
static $buf: bytes(256)
{decl}proc {name}() entry {{
L0:
  %p = addr $p
  %fd = callb open(%p, {O_RDONLY}, 0)
  %bad = cmp.lt %fd, 0
  brif %bad, FAIL, READ
READ:
  %b = addr $buf
  %n = callb read(%fd, %b, 256)
  %nb = cmp.lt %n, 0
  brif %nb, FAILC, GOT
GOT:
  dsply \"NFSRD-READ\"
  dsply %n
{compare}DONE:
  %c = callb close(%fd)
  %mo = addr $mok
  callb MARK(%mo)
  ret
FAILC:
  %e = callb ERRNO()
  %c2 = callb close(%fd)
  br REPORT
FAIL:
  %e = callb ERRNO()
  br REPORT
REPORT:
  dsply \"NFSRD-FAILED\"
  dsply %e
  %is4 = cmp.eq %e, {EINTR}
  brif %is4, INTR, NOTINTR
INTR:
  %mi = addr $mintr
  callb MARK(%mi)
  ret
NOTINTR:
  %is7 = cmp.eq %e, {ETIMEDOUT}
  brif %is7, TIMED, OTHER
TIMED:
  %mt = addr $mtime
  callb MARK(%mt)
  ret
OTHER:
  %me = addr $merr
  callb MARK(%me)
  ret
}}
",
        head = head(name),
    ))
}

/// `data` written over a file, truncated first.
pub(crate) fn write(name: &str, path: &str, data: &[u8]) -> String {
    finish(&format!(
        "{head}static $p: bytes(*) inz(z\"{path}\")
static $d: bytes({n}) inz(x\"{h}\")
proc {name}() entry {{
L0:
  %p = addr $p
  %fd = callb open(%p, {flags}, 0)
  %bad = cmp.lt %fd, 0
  brif %bad, FAIL, PUT
PUT:
  %d = addr $d
  %w = callb write(%fd, %d, {n})
  %c = callb close(%fd)
  %ok = cmp.eq %w, {n}
  brif %ok, DONE, FAIL
DONE:
  dsply \"NFSWR-OK\"
  ret
FAIL:
  %e = callb ERRNO()
  dsply \"NFSWR-FAILED\"
  dsply %e
  ret
}}
",
        head = head(name),
        n = data.len(),
        h = hex(data),
        flags = O_WRONLY | O_TRUNC,
    ))
}

/// Make program `QGPL/name` from `text` on a guest (any earlier one gone).
pub(crate) fn build(s: &mut Session, name: &str, text: &str) -> Result<(), String> {
    let _ = s.command(&format!("DLTPGM PGM(QGPL/{name})"), super::CMD);
    let _ = s.command(&format!("DLTMOD MODULE(QGPL/{name})"), super::CMD);
    crate::phase5::build(s, name, text)?;
    let _ = s.command(&format!("svc.xfer rm {name}"), super::CMD);
    Ok(())
}

/// What a program's `DSPLY` lines said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Said {
    /// `NFSLK-LOCKED`, `NFSRD-SAME`, `NFSWR-OK`, `NFSLK-HELD` ...: the tag.
    Ok(String),
    /// A failure tag and the `errno` after it.
    Failed(String, u32),
    /// None of the tags.
    Nothing,
}

/// The first tag in a call's output, with the number after a failure's.
pub(crate) fn said(out: &str) -> Said {
    let lines: Vec<&str> = out.lines().map(str::trim).collect();
    for (i, l) in lines.iter().enumerate() {
        let tag = l.split_whitespace().find(|w| {
            w.starts_with("NFSLK-") || w.starts_with("NFSRD-") || w.starts_with("NFSWR-")
        });
        let Some(tag) = tag else { continue };
        if matches!(
            tag,
            "NFSLK-REFUSED" | "NFSLK-NOOPEN" | "NFSLK-FAILED" | "NFSRD-FAILED" | "NFSWR-FAILED"
        ) {
            let n = lines[i + 1..]
                .iter()
                .find_map(|l| {
                    l.split_whitespace()
                        .rev()
                        .find_map(|w| w.parse::<u32>().ok())
                })
                .unwrap_or(0);
            return Said::Failed(tag.to_string(), n);
        }
        if tag == "NFSRD-READ" {
            // The comparison, when there is one, says more.
            if let Some(t) = lines[i + 1..].iter().find_map(|l| {
                l.split_whitespace()
                    .find(|w| *w == "NFSRD-SAME" || *w == "NFSRD-DIFF")
            }) {
                return Said::Ok(t.to_string());
            }
        }
        return Said::Ok(tag.to_string());
    }
    Said::Nothing
}

/// Call `QGPL/name` at the console; what it said.
pub(crate) fn call(
    s: &mut Session,
    name: &str,
    limit: std::time::Duration,
) -> Result<(Said, String), String> {
    let out = s.command(&format!("call QGPL/{name}"), limit)?;
    Ok((said(&out), out))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use mi::exec::Program;
    use sysapi::MemProvider;
    use sysapi::ifs::memory::MemIfs;

    use super::*;

    /// Runs a program as `QSTEST` over `sys`; its console lines.
    fn run(sys: &MemIfs, text: &str) -> String {
        let mut p = Program::from_text(text).unwrap_or_else(|e| panic!("{e}\n{text}"));
        sysapi::install(p.job(), Rc::new(RefCell::new(MemProvider::default())));
        sysapi::ifs::configure(p.job(), Rc::new(sys.client("QSTEST", 1, 37)), 37);
        let r = p.call_entry();
        let mut out = p.job().console.join("\n");
        if let Err(e) = r {
            let _ = write!(out, "\nended: {e}");
        }
        out
    }

    const DIR: &str = "/home/QSTEST";

    #[test]
    fn the_programs_run() {
        let sys = MemIfs::new();
        let f = format!("{DIR}/lk.dat");
        // The write opens without O_CREAT (a member exists first): not there.
        let data = b"NFSQSYSTEXT and more";
        let out = run(&sys, &write("NFSW", &f, data));
        assert!(
            matches!(said(&out), Said::Failed(ref t, _) if t == "NFSWR-FAILED"),
            "{out}"
        );
        // Made, then written, read back and compared.
        let make = finish(&format!(
            "{}static $p: bytes(*) inz(z\"{f}\")\nproc MK() entry {{\nL0:\n  %p = addr $p\n  callb MARK(%p)\n  ret\n}}\n",
            head("MK")
        ));
        let _ = run(&sys, &make);
        let out = run(&sys, &write("NFSW", &f, data));
        assert_eq!(said(&out), Said::Ok("NFSWR-OK".into()), "{out}");
        let out = run(
            &sys,
            &read("NFSR", &f, Some(b"NFSQSYSTEXT"), &format!("{DIR}/r")),
        );
        assert_eq!(said(&out), Said::Ok("NFSRD-SAME".into()), "{out}");
        let out = run(
            &sys,
            &read("NFSR", &f, Some(b"SOMETHING"), &format!("{DIR}/r")),
        );
        assert_eq!(said(&out), Said::Ok("NFSRD-DIFF".into()), "{out}");
        let out = run(
            &sys,
            &read("NFSR", &format!("{DIR}/none"), None, &format!("{DIR}/r")),
        );
        assert!(
            matches!(said(&out), Said::Failed(ref t, n) if t == "NFSRD-FAILED" && n > 3000),
            "{out}"
        );
        // The lock tried once: free, so taken and given back.
        let out = run(&sys, &try_lock("NFST", &f, 0, 100));
        assert_eq!(said(&out), Said::Ok("NFSLK-LOCKED".into()), "{out}");
        // The holder with its release already there: it locks, marks,
        // writes at 4096 and ends without a DLYJOB.
        let m = format!("{DIR}/hold");
        let out = run(&sys, &hold("NFSH", &f, 0, 100, &f, &m));
        assert!(
            out.contains("NFSLK-HELD") && out.contains("NFSLK-DONE"),
            "{out}"
        );
        for suffix in ["ok", "wrote"] {
            let p = format!("{m}.{suffix}");
            let out = run(&sys, &read("NFSR", &p, None, &format!("{DIR}/r")));
            assert_eq!(said(&out), Said::Ok("NFSRD-READ".into()), "{p}: {out}");
        }
        // A holder of a file not there says so.
        let out = run(&sys, &hold("NFSH", &format!("{DIR}/none"), 0, 1, &f, &m));
        assert!(
            matches!(said(&out), Said::Failed(ref t, _) if t == "NFSLK-FAILED"),
            "{out}"
        );
    }

    #[test]
    fn reads_what_programs_said() {
        assert_eq!(
            said("call QGPL/X\nNFSLK-LOCKED\n"),
            Said::Ok("NFSLK-LOCKED".into())
        );
        assert_eq!(
            said("NFSLK-REFUSED\n3406\n"),
            Said::Failed("NFSLK-REFUSED".into(), EAGAIN)
        );
        assert_eq!(
            said("DSPLY NFSRD-FAILED\nDSPLY 3447\n"),
            Said::Failed("NFSRD-FAILED".into(), ETIMEDOUT)
        );
        assert_eq!(
            said("NFSRD-READ\n80\nNFSRD-SAME\n"),
            Said::Ok("NFSRD-SAME".into())
        );
        assert_eq!(said("nothing"), Said::Nothing);
        assert!(flock(2, 0, 100).starts_with("x\"00020000000000000000006400"));
    }
}
