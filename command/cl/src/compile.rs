// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiler's driver: source to the IR text and the listing. The
//! listing follows the shape of IBM's (a prologue, the source with
//! statement numbers and nesting, a cross-reference of variables and
//! labels, the message summary, the end line); its words are ours.

use cmd::analyze::Env;

use crate::ast::Kind;
use crate::diag::{Diag, Severity};
use crate::sem::{Files, Tables};
use crate::{lex, lower, parse, sem};

/// What `CRTCLMOD`, `CRTBNDCL` and `CRTCLPGM` pass on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// The module or program name.
    pub name: String,
    /// `ACTGRP`.
    pub actgrp: String,
    /// `DFTACTGRP(*YES)`.
    pub dftactgrp: bool,
    /// `DBGVIEW`.
    pub dbgview: String,
    /// `LOG`.
    pub log: String,
    /// `OPTION(*XREF)`.
    pub xref: bool,
    /// `OPTION(*GEN)`.
    pub generate: bool,
    /// `OPTION(*SECLVL)`.
    pub seclvl: bool,
    /// `ALWRTVSRC(*YES)`: keep the source in the module.
    pub alwrtvsrc: bool,
    /// `TEXT`.
    pub text: String,
    /// The source file, library and member, for the listing.
    pub srcfile: String,
    /// The source library.
    pub srclib: String,
    /// The source member.
    pub srcmbr: String,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            name: "MAIN".into(),
            actgrp: "*CALLER".into(),
            dftactgrp: false,
            dbgview: "*STMT".into(),
            log: "*JOB".into(),
            xref: true,
            generate: true,
            seclvl: false,
            alwrtvsrc: true,
            text: String::new(),
            srcfile: "QCLSRC".into(),
            srclib: "*LIBL".into(),
            srcmbr: String::new(),
        }
    }
}

/// What a compilation produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compiled {
    /// The IR module text; empty when the program is not created.
    pub qir: String,
    /// The listing, line by line.
    pub listing: Vec<String>,
    /// Every diagnostic.
    pub diags: Vec<Diag>,
    /// The tables (the cross-reference).
    pub tables: Tables,
    /// Whether the object is to be created.
    pub created: bool,
}

/// Compile `source`.
pub fn compile(source: &str, opts: &Options, env: &mut Env<'_>, files: &mut dyn Files) -> Compiled {
    let (raws, mut diags) = lex::statements(source);
    let program = parse::program(&raws, &mut diags);
    let tables = sem::tables(&program, files, &mut diags);
    let mut qir = String::new();
    if !program.stmts.iter().any(|s| matches!(s.kind, Kind::Pgm(_))) {
        diags.push(Diag::warn(
            1,
            crate::diag::ids::PGM_FIRST,
            "no PGM statement; the program takes no parameters",
        ));
    }
    let gopts = lower::Options {
        name: opts.name.clone(),
        actgrp: opts.actgrp.clone(),
        dftactgrp: opts.dftactgrp,
        dbgview: opts.dbgview.clone(),
        log: opts.log.clone(),
    };
    let (text, gen_diags) = lower::module(&program, &tables, env, &gopts);
    diags.extend(gen_diags);
    diags.sort_by_key(|d| (d.line, d.msgid.clone()));
    let created = diags.iter().all(|d| d.severity < Severity::Error) && opts.generate;
    if created {
        qir = text;
    }
    let listing = listing(source, &program, &tables, &diags, opts, created);
    Compiled {
        qir,
        listing,
        diags,
        tables,
        created,
    }
}

/// The listing.
fn listing(
    source: &str,
    p: &crate::ast::Program,
    t: &Tables,
    diags: &[Diag],
    o: &Options,
    created: bool,
) -> Vec<String> {
    let mut out = Vec::new();
    let w = |s: String| s;
    out.push(w(format!(
        "{:<40}{}",
        "QS/400 ILE CL compiler",
        release::full()
    )));
    out.push(String::new());
    out.push(w(format!(
        "Program  . . . . . . . . . . . . :   {}",
        o.name
    )));
    out.push(w(format!(
        "Source file  . . . . . . . . . . :   {}/{}",
        o.srclib, o.srcfile
    )));
    out.push(w(format!(
        "Source member  . . . . . . . . . :   {}",
        o.srcmbr
    )));
    out.push(w(format!(
        "Text . . . . . . . . . . . . . . :   {}",
        o.text
    )));
    out.push(w(format!(
        "Options  . . . . . . . . . . . . :   {}{}{}",
        if o.xref { "*XREF " } else { "*NOXREF " },
        if o.generate { "*GEN " } else { "*NOGEN " },
        if o.seclvl { "*SECLVL" } else { "*NOSECLVL" }
    )));
    out.push(w(format!(
        "Debugging view . . . . . . . . . :   {}",
        o.dbgview
    )));
    out.push(w(format!(
        "Activation group . . . . . . . . :   {}",
        if o.dftactgrp { "*DFTACTGRP" } else { &o.actgrp }
    )));
    out.push(String::new());
    out.push("Statement  Nesting  Source".into());
    let mut depth: usize = 0;
    let mut by_line: std::collections::HashMap<usize, &crate::ast::Stmt> =
        std::collections::HashMap::new();
    for s in &p.stmts {
        by_line.insert(s.line, s);
    }
    for (i, line) in source.lines().enumerate() {
        let n = i + 1;
        let (stmt, nest) = match by_line.get(&n) {
            Some(s) => {
                let closing = matches!(s.kind, Kind::EndDo | Kind::EndSelect | Kind::EndSubr(_));
                if closing {
                    depth = depth.saturating_sub(1);
                }
                let shown = depth;
                let opening = matches!(s.kind, Kind::Do | Kind::Select | Kind::Subr(_))
                    || matches!(
                        s.kind,
                        Kind::DoWhile(_) | Kind::DoUntil(_) | Kind::DoFor(..)
                    )
                    || matches!(&s.kind, Kind::If(_, Some(t)) | Kind::When(_, Some(t)) | Kind::Otherwise(Some(t)) | Kind::Else(Some(t)) if matches!(t.kind, Kind::Do));
                if opening {
                    depth += 1;
                }
                (format!("{:>9}", s.n), format!("{shown:>7}"))
            }
            None => ("         ".into(), "       ".into()),
        };
        out.push(w(format!("{stmt}  {nest}  {line}")));
        for d in diags.iter().filter(|d| d.line == n) {
            out.push(w(format!("{:>9}  {:>7}  * {d}", "", "")));
        }
    }
    if o.xref {
        out.push(String::new());
        out.push("Cross-reference".into());
        out.push("Variable    Type    Length  Declared  Referenced".into());
        let mut names: Vec<&String> = t.vars.keys().collect();
        names.sort();
        for name in names {
            let v = &t.vars[name];
            let refs: Vec<String> = p
                .stmts
                .iter()
                .filter(|s| {
                    s.line != v.line && s.text.to_ascii_uppercase().contains(&format!("&{name}"))
                })
                .map(|s| s.n.to_string())
                .collect();
            let ty = match v.ty {
                crate::ast::VarType::Char => "*CHAR",
                crate::ast::VarType::Dec => "*DEC",
                crate::ast::VarType::Lgl => "*LGL",
                crate::ast::VarType::Int => "*INT",
                crate::ast::VarType::Uint => "*UINT",
                crate::ast::VarType::Ptr => "*PTR",
            };
            let len = if v.ty == crate::ast::VarType::Dec {
                format!("{} {}", v.len, v.dec)
            } else {
                v.len.to_string()
            };
            out.push(w(format!(
                "&{name:<10} {ty:<7} {len:<7} {:>8}  {}",
                v.line,
                refs.join(" ")
            )));
        }
        if !t.labels.is_empty() {
            out.push(String::new());
            out.push("Label       Defined   Referenced".into());
            let mut labels: Vec<(&String, &usize)> = t.labels.iter().collect();
            labels.sort();
            for (l, n) in labels {
                let refs: Vec<String> = p
                    .stmts
                    .iter()
                    .filter(|s| matches!(&s.kind, Kind::Goto(g) if g == l) || matches!(&s.kind, Kind::Leave(Some(g)) | Kind::Iterate(Some(g)) if g == l))
                    .map(|s| s.n.to_string())
                    .collect();
                out.push(w(format!("{l:<11} {n:>7}   {}", refs.join(" "))));
            }
        }
    }
    out.push(String::new());
    out.push("Message summary".into());
    let count = |s: Severity| diags.iter().filter(|d| d.severity == s).count();
    out.push(w(format!(
        "  Informational {}   Warning {}   Error {}   Terminal {}",
        count(Severity::Info),
        count(Severity::Warning),
        count(Severity::Error),
        count(Severity::Terminal)
    )));
    for d in diags {
        out.push(w(format!("  {d}  (statement at line {})", d.line)));
    }
    out.push(String::new());
    if created {
        out.push(w(format!("Program {} created.", o.name)));
    } else {
        out.push(w(format!(
            "Program {} not created: the errors above.",
            o.name
        )));
    }
    out.push("End of compilation.".into());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmd::analyze::MemSource;

    fn run(src: &str) -> Compiled {
        let mut s = MemSource::default();
        for (name, _) in sys_cpps::defs::DEFS {
            s.put(sys_cpps::install::object(name, 0).unwrap().def);
        }
        let mut env = Env::new(&mut s);
        let opts = Options {
            name: "T".into(),
            ..Options::default()
        };
        compile(src, &opts, &mut env, &mut crate::sem::NoFiles)
    }

    #[test]
    fn small_program() {
        let src = "\
             PGM        PARM(&LIB)
             DCL        VAR(&LIB) TYPE(*CHAR) LEN(10)
             DCL        VAR(&N) TYPE(*DEC) LEN(5 0) VALUE(1)
             DCL        VAR(&MSG) TYPE(*CHAR) LEN(50)
             CHGVAR     VAR(&N) VALUE(&N + 1)
             IF         COND(&N *GT 1) THEN(DO)
               CHGVAR     VAR(&MSG) VALUE('big' *BCAT %CHAR(&N))
             ENDDO
             ELSE       CMD(CHGVAR VAR(&MSG) VALUE('small'))
             DOWHILE    COND(&N *LT 10)
               CHGVAR     VAR(&N) VALUE(&N * 2)
               IF         COND(&N *EQ 8) THEN(LEAVE)
             ENDDO
             CRTLIB     LIB(&LIB) TEXT('made by T')
             MONMSG     MSGID(CPF2111) EXEC(GOTO DONE)
             SNDPGMMSG  MSG('done') TOPGMQ(*EXT)
 DONE:       RETURN
             ENDPGM
";
        let c = run(src);
        for d in &c.diags {
            eprintln!("{d}");
        }
        if std::env::var("CL_DUMP").is_ok() {
            eprintln!("{}", c.qir);
        }
        assert!(c.created, "{:?}", c.diags);
        let m = qir::parse(&c.qir).unwrap_or_else(|e| panic!("{e}\n{}", c.qir));
        qir::verify(&m).unwrap_or_else(|e| panic!("{e}\n{}", c.qir));
        assert!(c.listing.iter().any(|l| l.contains("Program T created.")));
        assert!(c.listing.iter().any(|l| l.contains("&LIB")));
    }

    #[test]
    fn errors_stop_creation() {
        let c =
            run("             PGM\n             CHGVAR VAR(&X) VALUE(1)\n             ENDPGM\n");
        assert!(!c.created);
        assert!(
            c.diags
                .iter()
                .any(|d| d.msgid == crate::diag::ids::UNDECLARED)
        );
        assert!(c.listing.iter().any(|l| l.contains("not created")));
    }
}
