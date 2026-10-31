// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The shipped startup program compiles against the shipped command
//! definitions, as the storage service compiles it at every IPL.

use cmd::analyze::{Env, MemSource, Setting};

#[test]
fn qstrup_compiles() {
    let source = include_str!("../../../lic/slic/src/stgsvc/shipped/QSTRUP.clle");
    let mut mem = MemSource::default();
    for (n, _) in sys_cpps::defs::DEFS {
        if let Ok(o) = sys_cpps::install::object(n, 0) {
            mem.put(o.def);
        }
    }
    let mut env = Env::new(&mut mem);
    env.setting = Setting {
        interactive: false,
        program: true,
        exec: false,
        module: false,
        syntax_only: false,
    };
    let opts = cl::Options {
        name: "QSTRUP".into(),
        srcfile: "QCLSRC".into(),
        srclib: "QSYS".into(),
        srcmbr: "QSTRUP".into(),
        text: "Startup program".into(),
        ..cl::Options::default()
    };
    let c = cl::compile(source, &opts, &mut env, &mut cl::NoFiles);
    let why: Vec<String> = c.diags.iter().map(ToString::to_string).collect();
    assert!(c.created, "{}", why.join("\n"));
    assert!(c.qir.contains("QSIPLA"), "the IPL attributes' data area is read");
}
