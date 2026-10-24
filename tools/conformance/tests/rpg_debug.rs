// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! An RPG program under the debugger: compiled with a debug view, a
//! breakpoint on a source line stops it, and `EVAL` shows fields,
//! array elements and data structure subfields by their RPG names, in
//! the main procedure, a subroutine and a subprocedure.

use std::cell::RefCell;
use std::rc::Rc;

use conformance::rpg::{self, build};
use ile::{ActGrpAttr, PgmKind};
use mi::debug::{Resume, Stop};
use mi::{Job, MemSource};
use rpg_oracle::case::Case;

/// Each stop's statement and the values `EVAL` showed there.
type Stops = Vec<(u32, Vec<String>)>;

const SOURCE: &str = "**FREE
//; cmd: CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*SOURCE)
ctl-opt dftactgrp(*no);
dcl-s count int(10) inz(3);
dcl-s name char(8) inz('QUARTZ');
dcl-s amt packed(7:2) inz(12.5);
dcl-s list char(2) dim(3) inz('ab');
dcl-ds rec qualified;
  id int(10) inz(42);
  label char(4) inz('rec');
end-ds;
count += 1;
exsr bump;
amt = twice(amt);
*inlr = *on;
begsr bump;
  count += 10;
endsr;
dcl-proc twice;
  dcl-pi *n packed(7:2);
    v packed(7:2) value;
  end-pi;
  dcl-s local char(3) inz('loc');
  return v * 2;
end-proc;
";

#[test]
fn eval_shows_rpg_names_at_breakpoints() {
    let case = Case::parse("DBGRPG", SOURCE);
    let c = rpg::compile(&case, None);
    assert!(c.created, "{}", rpg::notes(&c));
    assert!(c.qir.contains("debug.var \"REC.ID\""), "subfields are named");
    let m = build::module_object(&c.qir, "DBGRPG").expect("a module");
    assert!(m.debug_code.is_some(), "a debug view asks for the debug variant");
    let p = build::Objects::default()
        .bind("DBGRPG", PgmKind::Pgm, vec![m], &[], &[], ActGrpAttr::Caller)
        .expect("a program");
    let mut src = MemSource::default();
    src.add("QGPL", p);
    let mut job = Job::boxed(Box::new(src));
    qrpgrt::install(&mut job);
    let line = |text: &str| u32::try_from(SOURCE.lines().position(|l| l.trim() == text).expect("a line") + 1).unwrap();
    let (at_bump, at_amt, at_ret) = (line("count += 10;"), line("amt = twice(amt);"), line("return v * 2;"));
    let seen: Rc<RefCell<Stops>> = Rc::default();
    let s = seen.clone();
    job.set_debug_hook(Box::new(move |j: &mut Job, stop: &Stop| {
        let names = ["COUNT", "NAME", "AMT", "LIST", "LIST(2)", "REC", "REC.LABEL", "V", "LOCAL"];
        let vals = names.iter().map(|n| format!("{n}={}", j.eval(n).unwrap_or_else(|_| "-".into()))).collect();
        s.borrow_mut().push((stop.stmt, vals));
        Resume::Go
    }));
    job.strdbg("DBGRPG", "QGPL").expect("under debug");
    for l in [at_bump, at_amt, at_ret] {
        job.addbkp("DBGRPG", l);
    }
    job.call_program("DBGRPG", "QGPL", &[]).expect("it runs");
    let seen = seen.borrow();
    let at = |l: u32| &seen.iter().find(|(s, _)| *s == l).unwrap_or_else(|| panic!("no stop at {l}: {seen:?}")).1;
    let bump = at(at_bump);
    assert!(bump.contains(&"COUNT=4".to_string()), "{bump:?}");
    assert!(bump.contains(&"NAME='QUARTZ  '".to_string()), "{bump:?}");
    assert!(bump.contains(&"LIST(2)='ab'".to_string()), "{bump:?}");
    assert!(bump.contains(&"LIST=LIST(1) = 'ab'\nLIST(2) = 'ab'\nLIST(3) = 'ab'".to_string()), "{bump:?}");
    assert!(bump.contains(&"REC=REC.ID = 42\nREC.LABEL = 'rec '".to_string()), "{bump:?}");
    let amt = at(at_amt);
    assert!(amt.contains(&"COUNT=14".to_string()), "{amt:?}");
    assert!(amt.contains(&"AMT=12.50".to_string()), "{amt:?}");
    let ret = at(at_ret);
    assert!(ret.contains(&"V=12.50".to_string()), "{ret:?}");
    assert!(ret.contains(&"LOCAL='loc'".to_string()), "{ret:?}");
    assert!(ret.contains(&"COUNT=14".to_string()), "globals are seen in a subprocedure: {ret:?}");
}
