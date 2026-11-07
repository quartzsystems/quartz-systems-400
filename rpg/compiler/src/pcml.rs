// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `PGMINFO(*PCML)`: the Program Call Markup Language document of a
//! compiled program or module, as `INFOSTMF` receives it, in the form
//! IBM i 7.5's compiler writes (`tools/conformance/oracle/rpg/pcml/`):
//! a program describes its entry parameters, a module each exported
//! procedure; the structures they use go first; a comment before each
//! element gives the source line it comes from.

use std::fmt::Write as _;

use rpgfront::ast::{DefKind, Proc};

use crate::compile::Compiled;
use crate::syms::{Parm, Table};
use crate::types::Ty;

/// What the document is of.
pub struct Subject<'a> {
    /// The object name.
    pub name: &'a str,
    /// Its library.
    pub lib: &'a str,
    /// A module (`CRTRPGMOD`): its exported procedures; else a program.
    pub module: bool,
    /// Where the source came from, for the comment.
    pub source: &'a str,
    /// When, `yyyy-mm-dd-hh.mm.ss`.
    pub created: &'a str,
}

/// One parameter as PCML describes it.
struct Item {
    name: String,
    ty: Ty,
    dim: u32,
    value: bool,
    input: bool,
}

/// A `<program>`: its name, entry point, return value and line.
struct Program {
    name: String,
    entry: Option<String>,
    integer: bool,
    line: u32,
    parms: Vec<Item>,
}

/// The PCML document, or why there is none: a parameter PCML cannot
/// describe (a pointer, an object, a procedure pointer).
///
/// # Errors
///
/// The name of the parameter PCML has no type for.
pub fn document(c: &Compiled, s: &Subject<'_>) -> Result<String, String> {
    let t = &c.table;
    let programs: Vec<Program> = if s.module {
        c.module
            .procs
            .iter()
            .filter(|p| exported(p))
            .map(|p| {
                let proto = t
                    .protos
                    .iter()
                    .find(|x| x.name.eq_ignore_ascii_case(&p.name));
                Program {
                    name: p.name.to_ascii_uppercase(),
                    entry: Some(p.name.to_ascii_uppercase()),
                    integer: proto.and_then(|x| x.ret.clone()) == Some(Ty::Int(10)),
                    line: p.at.line,
                    parms: proto
                        .map(|x| x.parms.iter().map(item).collect())
                        .unwrap_or_default(),
                }
            })
            .collect()
    } else {
        let parms = t
            .entry
            .iter()
            .filter_map(|id| t.syms.get(*id))
            .map(|s| Item {
                name: s.name.clone(),
                ty: s.ty.clone(),
                dim: s.dim,
                value: false,
                input: false,
            })
            .collect();
        let line = c
            .module
            .defs
            .iter()
            .find(|d| d.kind == DefKind::Pi)
            .map_or(1, |d| d.at.line);
        vec![Program {
            name: s.name.to_ascii_uppercase(),
            entry: None,
            integer: false,
            line,
            parms,
        }]
    };
    let mut structs: Vec<usize> = Vec::new();
    for p in programs.iter().flat_map(|p| &p.parms) {
        check(&p.name, &p.ty)?;
        collect(t, &p.ty, &mut structs)?;
    }
    let mut out = String::from("<pcml version=\"7.0\">\n");
    let _ = writeln!(
        out,
        "   <!-- RPG {}: {}  -->",
        if s.module { "module" } else { "program" },
        s.name
    );
    let _ = writeln!(out, "   <!-- created: {} -->", s.created);
    let _ = writeln!(out, "   <!-- source: {} -->", s.source);
    for i in &structs {
        let d = &t.ds[*i];
        let (name, line) = t
            .syms
            .get(d.sym)
            .map_or(("DS", 0), |x| (x.name.as_str(), x.def_line));
        let _ = writeln!(out, "   <!-- {line} -->");
        let _ = writeln!(
            out,
            "   <struct name=\"{}\">",
            esc(&name.to_ascii_uppercase())
        );
        for f in &d.subfields {
            if let Some(sf) = t.syms.get(*f) {
                let _ = writeln!(
                    out,
                    "      {}",
                    data(t, &sf.name, &sf.ty, sf.dim, None, "inherit")
                );
            }
        }
        out.push_str("   </struct>\n");
    }
    for p in &programs {
        let _ = writeln!(out, "   <!-- {} -->", p.line);
        let mut head = format!("<program name=\"{}\"", esc(&p.name));
        match &p.entry {
            Some(e) => {
                let _ = write!(head, " entrypoint=\"{}\"", esc(e));
            }
            None => {
                let _ = write!(head, " path=\"/QSYS.LIB/{}.LIB/{}.PGM\"", s.lib, s.name);
            }
        }
        if p.integer {
            head.push_str(" returnvalue=\"integer\"");
        }
        let _ = writeln!(out, "   {head}>");
        for x in &p.parms {
            let usage = if x.input { "input" } else { "inputoutput" };
            let by = x.value.then_some("value");
            let _ = writeln!(out, "      {}", data(t, &x.name, &x.ty, x.dim, by, usage));
        }
        out.push_str("   </program>\n");
    }
    out.push_str("</pcml>\n");
    Ok(out)
}

fn exported(p: &Proc) -> bool {
    p.kws.iter().any(|k| k.name == "EXPORT")
}

fn item(p: &Parm) -> Item {
    Item {
        name: p.name.clone(),
        ty: p.ty.clone(),
        dim: p.dim,
        value: p.value,
        input: p.value || p.konst,
    }
}

/// A type PCML can describe; pointers and objects it cannot.
fn check(name: &str, ty: &Ty) -> Result<(), String> {
    match ty {
        Ty::Ptr | Ty::ProcPtr | Ty::Object | Ty::Unknown | Ty::Dur(_) => {
            Err(name.to_ascii_uppercase())
        }
        _ => Ok(()),
    }
}

/// The structures a type uses, inner ones first.
fn collect(t: &Table, ty: &Ty, out: &mut Vec<usize>) -> Result<(), String> {
    if let Ty::Ds(i) = *ty {
        if out.contains(&i) {
            return Ok(());
        }
        if let Some(d) = t.ds.get(i) {
            for f in &d.subfields {
                if let Some(s) = t.syms.get(*f) {
                    check(&s.name, &s.ty)?;
                    collect(t, &s.ty, out)?;
                }
            }
        }
        out.push(i);
    }
    Ok(())
}

/// One `<data>` element.
fn data(t: &Table, name: &str, ty: &Ty, dim: u32, passby: Option<&str>, usage: &str) -> String {
    let mut a = format!("<data name=\"{}\" ", esc(&name.to_ascii_uppercase()));
    let _ = match ty {
        Ty::Char(n) => write!(a, "type=\"char\" length=\"{n}\""),
        Ty::VarChar(n, _) => write!(a, "type=\"varchar\" length=\"{n}\""),
        Ty::Ucs2(n) | Ty::Graph(n) => {
            write!(a, "type=\"char\" length=\"{n}\" chartype=\"twobyte\"")
        }
        Ty::VarUcs2(n, _) | Ty::VarGraph(n, _) => {
            write!(a, "type=\"varchar\" length=\"{n}\" chartype=\"twobyte\"")
        }
        Ty::Ind => write!(a, "type=\"char\" length=\"1\""),
        Ty::Packed(d, s) => write!(a, "type=\"packed\" length=\"{d}\" precision=\"{s}\""),
        Ty::Zoned(d, s) => write!(a, "type=\"zoned\" length=\"{d}\" precision=\"{s}\""),
        Ty::Bin(d, _) => {
            let (len, prec) = if *d <= 4 { (2, 15) } else { (4, 31) };
            write!(a, "type=\"int\" length=\"{len}\" precision=\"{prec}\"")
        }
        Ty::Int(d) => {
            let (len, bits) = int_size(*d);
            write!(
                a,
                "type=\"int\" length=\"{len}\" precision=\"{}\"",
                bits - 1
            )
        }
        Ty::Uns(d) => {
            let (len, bits) = int_size(*d);
            write!(a, "type=\"int\" length=\"{len}\" precision=\"{bits}\"")
        }
        Ty::Float(n) => write!(a, "type=\"float\" length=\"{n}\""),
        Ty::Date(..) | Ty::Time(..) | Ty::Timestamp(_) => {
            write!(a, "type=\"char\" length=\"{}\"", t.size(ty))
        }
        Ty::Ds(i) => {
            let s =
                t.ds.get(*i)
                    .and_then(|d| t.syms.get(d.sym))
                    .map_or("DS", |x| x.name.as_str());
            write!(
                a,
                "type=\"struct\" struct=\"{}\"",
                esc(&s.to_ascii_uppercase())
            )
        }
        Ty::Ptr | Ty::ProcPtr | Ty::Object | Ty::Unknown | Ty::Dur(_) => Ok(()),
    };
    if dim > 0 {
        let _ = write!(a, " count=\"{dim}\"");
    }
    if let Some(p) = passby {
        let _ = write!(a, " passby=\"{p}\"");
    }
    let _ = write!(a, " usage=\"{usage}\" />");
    a
}

/// An integer's bytes and bits from its digits (3, 5, 10, 20).
fn int_size(digits: u8) -> (u8, u8) {
    match digits {
        0..=3 => (1, 8),
        4..=5 => (2, 16),
        6..=10 => (4, 32),
        _ => (8, 64),
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sources the captures under `tools/conformance/oracle/rpg/pcml/`
    /// came from.
    const PROGRAM: &str = "**FREE\ndcl-ds cust qualified template;\n  id packed(7:0);\n  name char(20);\nend-ds;\n\
        dcl-pi *n;\n  who char(10);\n  amt packed(9:2);\n  n int(10);\n  u uns(5);\n  z zoned(5:1);\n  v varchar(30);\n\
        \x20 arr char(3) dim(4);\nend-pi;\n*inlr = *on;\n";
    const MODULE: &str = "**FREE\nctl-opt nomain;\ndcl-ds cust qualified template;\n  id packed(7:0);\n  name char(20);\nend-ds;\n\
        dcl-proc add export;\n  dcl-pi *n int(10);\n    a int(10) value;\n    b int(10) const;\n    c likeds(cust);\n\
        \x20   f float(8);\n    i ind;\n  end-pi;\n  return a + b;\nend-proc;\ndcl-proc hidden;\nend-proc;\n";

    fn compiled(src: &str, name: &str, command: crate::Command) -> Compiled {
        let m = rpgfront::Member::from_text("{user}1", "QSPCMLSRC", name, src);
        let o = crate::Options {
            command,
            name: name.into(),
            lib: "{user}1".into(),
            src: ("{user}1".into(), "QSPCMLSRC".into(), name.into()),
            ..crate::Options::default()
        };
        crate::compile(m, &mut rpgfront::NoIncludes, &mut crate::NoExtFiles, &o)
    }

    /// Ours against IBM's, every line but the time.
    fn same(ours: &str, capture: &str) {
        let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/conformance/oracle/rpg/pcml")
            .join(capture);
        let theirs = std::fs::read_to_string(&file).unwrap().replace('\r', "");
        let keep = |l: &&str| !l.contains("<!-- created:");
        let a: Vec<&str> = ours.lines().filter(keep).collect();
        let b: Vec<&str> = theirs.lines().filter(keep).collect();
        assert_eq!(a, b, "{ours}");
    }

    #[test]
    fn a_program_as_ibm_writes_it() {
        let c = compiled(PROGRAM, "PCMLT", crate::Command::CrtBndRpg);
        assert!(c.created, "{:?}", c.diags);
        let s = Subject {
            name: "PCMLT",
            lib: "{user}1",
            module: false,
            source: "{user}1/QSPCMLSRC(PCMLT)",
            created: "x",
        };
        same(&document(&c, &s).unwrap(), "pcmlt.pcml");
    }

    #[test]
    fn a_module_as_ibm_writes_it() {
        let c = compiled(MODULE, "PCMLM", crate::Command::CrtRpgMod);
        assert!(c.created, "{:?}", c.diags);
        let s = Subject {
            name: "PCMLM",
            lib: "{user}1",
            module: true,
            source: "{user}1/QSPCMLSRC(PCMLM)",
            created: "x",
        };
        same(&document(&c, &s).unwrap(), "pcmlm.pcml");
    }

    #[test]
    fn a_pointer_has_no_pcml() {
        let c = compiled(
            "**FREE\ndcl-pi *n;\n  p pointer;\nend-pi;\n*inlr = *on;\n",
            "T",
            crate::Command::CrtBndRpg,
        );
        let s = Subject {
            name: "T",
            lib: "QGPL",
            module: false,
            source: "",
            created: "",
        };
        assert_eq!(document(&c, &s).unwrap_err(), "P");
    }
}
