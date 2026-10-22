// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every shipped command has somewhere to go: a CL statement the
//! compiler takes, a job-site handler, a stub message, a direct
//! implementation in the service, a bridge word the service's console
//! tables know, or a place on the unsupported list.

use sys_cpps::inventory::{BRIDGE_WORDS, CL_STATEMENTS, DIRECT, JOB_SITE, UNSUPPORTED};
use sys_cpps::{Site, all};

#[test]
fn every_command_has_a_site() {
    let mut missing = Vec::new();
    for e in all() {
        let name = e.name.as_str();
        let ok = match e.site {
            Site::Stub => e.stub.is_some(),
            Site::Job => JOB_SITE.contains(&name),
            Site::Service => {
                CL_STATEMENTS.contains(&name)
                    || DIRECT.contains(&name)
                    || BRIDGE_WORDS.contains(&e.word.as_str())
                    || UNSUPPORTED.contains(&name)
            }
        };
        if !ok {
            missing.push(format!("{name} ({:?}, word {})", e.site, e.word));
        }
    }
    assert!(
        missing.is_empty(),
        "commands without a place to go:\n  {}",
        missing.join("\n  ")
    );
}

#[test]
fn the_unsupported_list_is_honest() {
    for name in UNSUPPORTED {
        let e = sys_cpps::entry(name).unwrap_or_else(|| panic!("{name}: no definition"));
        assert!(
            !DIRECT.contains(name)
                && !JOB_SITE.contains(name)
                && !CL_STATEMENTS.contains(name)
                && !BRIDGE_WORDS.contains(&e.word.as_str())
                && e.stub.is_none(),
            "{name} is listed as unsupported but has a place to go"
        );
    }
}

#[test]
fn the_lists_name_real_commands() {
    for name in DIRECT.iter().chain(JOB_SITE).chain(CL_STATEMENTS) {
        assert!(
            sys_cpps::entry(name).is_some(),
            "{name} is listed but has no definition"
        );
    }
}
