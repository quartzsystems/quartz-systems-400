// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! SEU's prompt of a CL source line (`Prompter::for_source_line`): the
//! `Label` field above the parameters and `Comment` below them, the
//! values kept apart from the command string. Compared with pub400's
//! `seu-prompt-cl` on the title, the instruction and the label row;
//! the parameter rows follow the prompter's own layout.

use std::path::Path;

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::{Prompter, Step};

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

fn golden_rows(name: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/verified/pub400")
        .join(format!("{name}.txt"));
    std::fs::read_to_string(&p)
        .unwrap_or_else(|e| panic!("{}: {e}", p.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn label_and_comment_around_the_parameters() {
    let mut src = source();
    let r = {
        let mut env = Env::new(&mut src);
        env.setting.program = true;
        analyze(&mut env, "DSPLIB").unwrap()
    };
    let mut p = Prompter::for_source_line(&r, "");
    let s = p.screen();
    let want = golden_rows("seu-prompt-cl");
    for row in [1usize, 3, 5] {
        assert_eq!(s.rows[row - 1].trim_end(), want[row - 1].trim_end(), "row {row}");
    }
    let comment_row = want.iter().position(|l| l.trim_start().starts_with("Comment")).unwrap();
    let ours = s.rows.iter().position(|l| l.trim_start().starts_with("Comment")).unwrap();
    assert_eq!(s.rows[ours].trim_end(), want[comment_row].trim_end());
    // Typed into: the label and comment come back apart.
    let mut fields: Vec<String> = s.fields.iter().map(|f| f.value.clone()).collect();
    fields[0] = "again".into();
    let last = fields.len() - 1;
    fields[last] = "look at it".into();
    let mut env = Env::new(&mut src);
    env.setting.program = true;
    let step = p.key(0, &fields, &mut env);
    assert!(matches!(step, Step::Done(ref t) if t.starts_with("DSPLIB")), "{step:?}");
    assert_eq!(p.source_parts(), Some(("AGAIN".into(), "look at it".into())));
}
