// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `Rfile -r [-bKQ] [-c command] file` and `Rfile -w [-abQ] [-c
//! command] file`: read a record file to standard output, one record a
//! line with trailing blanks removed (`-K` keeps them), or write
//! standard input to one, replacing its records (`-a` appends). `-b`
//! copies bytes unchanged. The file is a path (a `.FILE` directory
//! means its first member) or with `-Q` a qualified name
//! `[library/]file[(member)]`, the library `*LIBL` or `*CURLIB` or
//! left out to search the library list. `-c` runs a CL command first
//! (an override, say).

use qsh::sys::{FileType, LibKind, UtilCtx};

use super::msgs;
use crate::common::args::getopt;
use crate::common::io::{Lines, Out, copy, open_read, open_write, stdin};
use crate::common::{complain, complain_sys, usage};

const NAME: &str = "Rfile";

/// `[library/]file[(member)]` split into its parts, upper-cased.
pub fn qualified(s: &str) -> Option<(Option<String>, String, Option<String>)> {
    let (obj, mbr) = match s.split_once('(') {
        Some((o, m)) => (o, Some(m.strip_suffix(')')?.to_ascii_uppercase())),
        None => (s, None),
    };
    let (lib, file) = match obj.split_once('/') {
        Some((l, f)) => (Some(l.to_ascii_uppercase()), f),
        None => (None, obj),
    };
    if file.is_empty() || file.contains('/') || mbr.as_deref() == Some("") {
        return None;
    }
    Some((lib, file.to_ascii_uppercase(), mbr))
}

fn first_member(cx: &UtilCtx<'_>, file_dir: &str) -> Result<String, String> {
    let mut names = cx
        .sys
        .read_dir(file_dir)
        .map_err(|e| e.errno.text().to_string())?;
    names.sort();
    names
        .into_iter()
        .find(|n| n.to_ascii_uppercase().ends_with(".MBR"))
        .map(|n| format!("{file_dir}/{n}"))
        .ok_or_else(|| "The file has no members.".to_string())
}

fn member_path(cx: &UtilCtx<'_>, dir: &str, mbr: Option<&str>) -> Result<String, String> {
    match mbr {
        None | Some("*FIRST") => first_member(cx, dir),
        Some(m) => Ok(format!("{dir}/{m}.MBR")),
    }
}

/// The path of the member the operand names.
fn resolve(cx: &UtilCtx<'_>, op: &str, q: bool) -> Result<String, String> {
    if !q {
        let abs = cx.path(op);
        let is_file_dir = abs.to_ascii_uppercase().ends_with(".FILE")
            && cx.sys.stat(&abs).is_ok_and(|s| s.ftype == FileType::Dir);
        return if is_file_dir {
            first_member(cx, &abs)
        } else {
            Ok(abs)
        };
    }
    let Some((lib, file, mbr)) = qualified(op) else {
        return Err("The qualified name is not valid.".to_string());
    };
    let libs: Vec<String> = match lib.as_deref() {
        Some("*LIBL") | None => cx
            .sys
            .library_list()
            .map_err(|e| e.errno.text().to_string())?
            .into_iter()
            .map(|e| e.name)
            .collect(),
        Some("*CURLIB") => cx
            .sys
            .library_list()
            .map_err(|e| e.errno.text().to_string())?
            .into_iter()
            .filter(|e| e.kind == LibKind::Current)
            .map(|e| e.name)
            .collect(),
        Some(l) => vec![l.to_string()],
    };
    for l in &libs {
        let dir = format!("/QSYS.LIB/{l}.LIB/{file}.FILE");
        if cx.sys.stat(&dir).is_ok() {
            return member_path(cx, &dir, mbr.as_deref());
        }
    }
    Err(format!("The file {file} is not found."))
}

/// Run `Rfile`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let p = match getopt(&cx.argv[1..], "rwabKQqc:") {
        Ok(p) => p,
        Err(e) => return usage(cx, NAME, &e.to_string()),
    };
    if p.has('r') == p.has('w') {
        return usage(cx, NAME, "Give -r or -w.");
    }
    let [op] = p.operands.as_slice() else {
        return usage(cx, NAME, "One file is taken.");
    };
    if let Some(c) = p.value('c')
        && !msgs::run_quiet(cx, NAME, c)
    {
        return 1;
    }
    let path = match resolve(cx, op, p.has('Q')) {
        Ok(p) => p,
        Err(e) => {
            complain(cx, NAME, &format!("{op}: {e}"));
            return 1;
        }
    };
    if p.has('w') {
        let fd = match open_write(cx, &path, p.has('a')) {
            Ok(f) => f,
            Err(e) => {
                complain_sys(cx, NAME, op, e);
                return 1;
            }
        };
        let src = stdin(cx);
        if p.has('b') {
            if let Err(e) = copy(&src, &fd) {
                complain(cx, NAME, &format!("{op}: {}", crate::common::io::errno_text(&e)));
                return 1;
            }
            return 0;
        }
        let mut out = Out::on(Some(fd));
        let mut lines = Lines::new(src);
        while let Ok(Some((l, _))) = lines.next_text() {
            out.write(&l);
            out.text("\n");
        }
        return out.finish(cx, NAME, 0);
    }
    let fd = match open_read(cx, &path) {
        Ok(f) => f,
        Err(e) => {
            complain_sys(cx, NAME, op, e);
            return 1;
        }
    };
    if p.has('b') {
        return match cx.fd(1) {
            Some(o) => i32::from(copy(&fd, o).is_err()),
            None => 1,
        };
    }
    let mut out = Out::stdout(cx);
    let mut lines = Lines::new(fd);
    while let Ok(Some((mut l, _))) = lines.next_text() {
        if !p.has('K') {
            while l.last() == Some(&b' ') {
                l.pop();
            }
        }
        out.write(&l);
        out.text("\n");
    }
    out.finish(cx, NAME, 0)
}

#[cfg(test)]
mod tests {
    use super::qualified;

    #[test]
    fn names() {
        assert_eq!(
            qualified("qgpl/qclsrc(hello)"),
            Some((Some("QGPL".into()), "QCLSRC".into(), Some("HELLO".into())))
        );
        assert_eq!(qualified("QCLSRC"), Some((None, "QCLSRC".into(), None)));
        assert_eq!(qualified("a/b/c"), None);
        assert_eq!(qualified("f(m"), None);
        assert_eq!(qualified("f()"), None);
    }
}
