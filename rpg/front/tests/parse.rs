// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Whole members through the front end: a fixed-form report program,
//! a fully free program with groups and procedures, and mixed source.

use rpgfront::ast::{DefKind, Expr, Module};
use rpgfront::reader::Options;
use rpgfront::{Diag, MemIncludes, Member, front};

fn run(text: &str) -> (Module, Vec<Diag>) {
    let m = Member::from_text("QGPL", "QRPGLESRC", "T", text);
    let (_, module, diags) = front(m, &mut MemIncludes::default(), &Options::default());
    (module, diags)
}

#[test]
fn fixed_form_report_program() {
    let src = "     H DFTACTGRP(*NO) ACTGRP('QILE')
     FCUSTOMER  IP   E           K DISK
     FQSYSPRT   O    F  132        PRINTER OFLIND(*INOF)
     D Total           S             11P 2 INZ(0)
     D Names           S             10A   DIM(5) CTDATA PERRCD(1)
     D Rec             DS
     D  Id                     1      5S 0
     D  Nm                     6     25
     D Lng             C                   CONST('A long +
     D                                     literal')
     C   01
     CAN 02              ADD       BAL           TOTAL
     C     TOTAL         IFGT      1000
     C                   EVAL      Total = Total +
     C                               1
     C                   ENDIF
     C     KEY           CHAIN     CUSTOMER                           50
     CLR                 EXCEPT    TOT
     OQSYSPRT   H    1P                     2  3
     O                                           20 'CUSTOMER LIST'
     O          E            TOT         1
     O                       TOTAL         J     30
**CTDATA NAMES
ALPHA
BETA
";
    let (m, d) = run(src);
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(m.ctl.len(), 2);
    assert_eq!(m.files.len(), 2);
    assert_eq!(m.files[0].fixed.as_ref().unwrap().designation, "P");
    assert_eq!(m.files[1].kws[0].name, "OFLIND");
    assert_eq!(m.defs.len(), 4);
    assert_eq!(m.defs[2].kind, DefKind::Ds);
    assert_eq!(m.defs[2].children.len(), 2);
    assert_eq!(m.defs[2].children[0].fixed.as_ref().unwrap().to, "5");
    match &m.defs[3].value {
        Some(Expr::Lit(rpgfront::ast::Lit::Char(s), _)) => assert_eq!(s, "A long literal"),
        other => panic!("{other:?}"),
    }
    let codes: Vec<&str> = m.calcs.iter().map(|s| s.op.code.as_str()).collect();
    assert_eq!(codes, vec!["ADD", "IFGT", "EVAL", "ENDIF", "CHAIN", "EXCEPT"]);
    assert_eq!(m.calcs[0].cond.len(), 1);
    assert_eq!(m.calcs[0].cond[0].len(), 2);
    assert!(m.calcs[2].op.xf2.is_some());
    assert_eq!(m.calcs[4].op.resind[0], "50");
    assert_eq!(m.calcs[5].level, "LR");
    assert_eq!(m.outputs.len(), 2);
    assert_eq!(m.outputs[0].fields.len(), 1);
    assert_eq!(m.outputs[1].except, "TOT");
    assert_eq!(m.outputs[1].fields[0].edit, "J");
    assert_eq!(m.ctdata.len(), 1);
    assert_eq!(m.ctdata[0].records, vec!["ALPHA".to_string(), "BETA".to_string()]);
}

#[test]
fn fully_free_program() {
    let src = "**FREE
ctl-opt main(Main) dftactgrp(*no);
dcl-f QSYSPRT printer(132) usage(*output);
dcl-ds Line len(132) end-ds;
dcl-ds Cust qualified;
  id packed(7:0);
  dcl-subf name char(30);
end-ds;
dcl-pr Fmt varchar(40);
  amount packed(11:2) const;
end-pr;
dcl-c MAX 10;
dcl-proc Main;
  dcl-s i int(10);
  for i = 1 to MAX by 2;
    Line = Fmt(i) + ' done';
    write QSYSPRT Line;
  endfor;
  if %error or not %found(Cust);
    *inlr = *on;
  endif;
  chain(e) (1:'x') CUSTFILE Cust;
  Cust.id += 1;
  return;
end-proc;
dcl-proc Fmt export;
  dcl-pi *n varchar(40);
    amount packed(11:2) const;
  end-pi;
  return %char(amount);
end-proc;
";
    let (m, d) = run(src);
    assert!(d.is_empty(), "{d:?}");
    assert!(m.fully_free);
    assert_eq!(m.ctl.len(), 2);
    let kinds: Vec<DefKind> = m.defs.iter().map(|d| d.kind).collect();
    assert_eq!(kinds, vec![DefKind::Ds, DefKind::Ds, DefKind::Pr, DefKind::Const]);
    assert_eq!(m.defs[1].children.len(), 2);
    assert_eq!(m.defs[2].children.len(), 1);
    assert_eq!(m.procs.len(), 2);
    let main = &m.procs[0];
    assert_eq!(main.defs.len(), 1);
    let codes: Vec<&str> = main.calcs.iter().map(|s| s.op.code.as_str()).collect();
    assert_eq!(
        codes,
        vec!["FOR", "EVAL", "WRITE", "ENDFOR", "IF", "EVAL", "ENDIF", "CHAIN", "EVAL", "RETURN"]
    );
    assert_eq!(main.calcs[0].op.more.len(), 2);
    let chain = &main.calcs[7].op;
    assert_eq!(chain.ext, "E");
    assert!(matches!(chain.f1[0], Expr::List(..)));
    assert_eq!(chain.f2[0].name(), Some("CUSTFILE"));
    assert_eq!(chain.result[0].name(), Some("CUST"));
    let fmt = &m.procs[1];
    assert_eq!(fmt.pi.as_ref().unwrap().children.len(), 1);
    assert_eq!(fmt.kws[0].name, "EXPORT");
}

#[test]
fn mixed_source_and_errors() {
    let src = "     D Count           S             10I 0
       dcl-s Name char(10);
     C                   EVAL      Count = 1
       Name = 'abc';
       if Count = 1
       endif;
     C                   FROBNICATE
";
    let (m, d) = run(src);
    let ids: Vec<&str> = d.iter().map(|x| x.msgid).collect();
    assert!(ids.contains(&"RNF5013"), "{ids:?}");
    assert_eq!(m.defs.len(), 2);
    let codes: Vec<&str> = m.calcs.iter().map(|s| s.op.code.as_str()).collect();
    // The IF without its semicolon swallows ENDIF; the bad operation
    // is reported and still listed.
    assert_eq!(codes[..2], ["EVAL", "EVAL"]);
}
