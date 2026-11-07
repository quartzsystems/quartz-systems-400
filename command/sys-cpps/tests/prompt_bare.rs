// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A command prompted with nothing filled in (`?SNDMSG`, a menu
//! option's `CMD ?SNDMSG`): its required parameters are the prompt's to
//! ask for, not an error before it.

use cmd::analyze::{Env, MemSource, analyze};
use cmd::prompt::Prompter;

fn source() -> MemSource {
    let mut s = MemSource::default();
    for (name, _) in sys_cpps::defs::DEFS {
        let o = sys_cpps::install::object(name, 0).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        s.put(o.def);
    }
    s
}

#[test]
fn required_parameters_wait_for_the_prompt() {
    let mut src = source();
    let mut env = Env::new(&mut src);
    env.setting.interactive = true;
    for text in ["?SNDMSG", "?SNDBRKMSG", "?CRTLIB", "?SBMJOB"] {
        let r = analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"));
        assert!(r.prompt, "{text}");
        let screen = Prompter::new(&r).screen().text();
        assert!(
            screen.contains(&format!("({})", &text[1..])),
            "{text}:\n{screen}"
        );
    }
}
