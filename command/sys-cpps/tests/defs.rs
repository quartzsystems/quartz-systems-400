// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every shipped definition compiles, the inventory matches the
//! sources, processing program names are unique, and no prompt text or
//! message text repeats an IBM phrasing from the denylist.

use std::collections::HashSet;

use sys_cpps::{Site, all, defs, inventory};

#[test]
fn every_definition_compiles() {
    let mut failed = Vec::new();
    for (name, source) in defs::DEFS {
        if let Err(diags) = cmd::source::compile(name, source) {
            for d in diags {
                failed.push(format!("{name}: {} {}", d.msgid, d.detail));
            }
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
    assert!(defs::DEFS.len() >= 3);
}

#[test]
fn file_names_match_command_names() {
    for (name, source) in defs::DEFS {
        assert!(
            name.len() <= 10
                && name
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()),
            "{name}: a command name is up to ten upper-case characters"
        );
        assert!(source.contains("CMD"), "{name}: no CMD statement");
    }
}

#[test]
fn inventory_is_consistent() {
    let orphans = inventory::orphans();
    assert!(
        orphans.is_empty(),
        "inventory entries without a definition: {orphans:?}"
    );
    let mut cpps = HashSet::new();
    for e in all() {
        assert!(
            e.cpp.len() <= 10 && e.cpp.starts_with('Q'),
            "{}: CPP {}",
            e.name,
            e.cpp
        );
        assert!(
            cpps.insert(e.cpp.clone()),
            "{}: CPP {} is used twice",
            e.name,
            e.cpp
        );
        if e.site == Site::Stub {
            assert!(e.stub.is_some(), "{}: a stub names its message", e.name);
        }
    }
}

/// IBM phrasings no shipped text may repeat (the tripwire of the
/// acceptance list); one per line, case-insensitive, `#` comments.
const DENYLIST: &str = include_str!("../denylist.txt");

fn denied() -> Vec<String> {
    DENYLIST
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_ascii_lowercase)
        .collect()
}

#[test]
fn no_ibm_phrasing_in_definitions() {
    let denied = denied();
    let mut hits = Vec::new();
    for (name, source) in defs::DEFS {
        let lower = source.to_ascii_lowercase();
        for d in &denied {
            if lower.contains(d) {
                hits.push(format!("{name}: {d:?}"));
            }
        }
    }
    assert!(hits.is_empty(), "\n{}", hits.join("\n"));
}

#[test]
fn no_ibm_phrasing_in_messages() {
    let denied = denied();
    let mut hits = Vec::new();
    for (file, text) in [
        ("qcpfmsg.toml", msgf_source("qcpfmsg.toml")),
        ("qceemsg.toml", msgf_source("qceemsg.toml")),
        ("qsqlmsg.toml", msgf_source("qsqlmsg.toml")),
        ("qrpglemsg.toml", msgf_source("qrpglemsg.toml")),
        ("qrnxmsg.toml", msgf_source("qrnxmsg.toml")),
    ] {
        let lower = text.to_ascii_lowercase();
        for d in &denied {
            if lower.contains(d) {
                hits.push(format!("{file}: {d:?}"));
            }
        }
    }
    assert!(hits.is_empty(), "\n{}", hits.join("\n"));
}

fn msgf_source(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../runtime/msgf/msgs")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// `CHGIPLA`'s values reach the service whole: a `*SAME` cut to its
/// parameter's length (`*SAM`) made every `CHGIPLA` fail.
#[test]
fn chgipla_values_fit_the_block() {
    let mut src = cmd::analyze::MemSource::default();
    for (name, _) in defs::DEFS {
        src.put(sys_cpps::install::object(name, 0).unwrap().def);
    }
    let mut env = cmd::analyze::Env::new(&mut src);
    let r = cmd::analyze::analyze(&mut env, "CHGIPLA STRTCP(*YES)").unwrap();
    let built = cmd::block::build(&r.def, &r.parms).unwrap();
    let spec: Vec<bool> = r.parms.iter().map(|b| b.specified).collect();
    let parms = cmd::block::read(&r.def, &built.segments, &spec).unwrap();
    for p in &r.def.parms {
        let want = if p.kwd == "STRTCP" { "*YES" } else { "*SAME" };
        assert_eq!(parms.text(&p.kwd), want, "{}", p.kwd);
    }
}

/// Values that reach the parameter block unmapped but are longer than
/// the field that holds them.
fn overlong(cmd: &str, path: &str, p: &cmd::def::Parm, out: &mut Vec<String>) {
    use cmd::def::ParmType as T;
    let path = if p.kwd.is_empty() {
        path.to_string()
    } else {
        p.kwd.clone()
    };
    for (i, item) in p.items.iter().enumerate() {
        overlong(cmd, &format!("{path}.{}", i + 1), item, out);
    }
    let text = matches!(
        p.ty,
        T::Char | T::Name | T::Generic | T::PName | T::SName | T::CName
    );
    if !text {
        return;
    }
    let len = p.length().len as usize;
    let unmapped = p
        .spcval
        .iter()
        .chain(&p.sngval)
        .filter(|s| s.to.is_none())
        .map(|s| &s.from);
    // A list's default names one value per item.
    let dft = p.dft.iter().flat_map(|d| {
        if p.is_list() && !d.starts_with('\'') {
            d.split_whitespace().map(str::to_string).collect()
        } else {
            vec![d.clone()]
        }
    });
    let mapped: HashSet<String> = p
        .spcval
        .iter()
        .chain(&p.sngval)
        .filter(|s| s.to.is_some())
        .map(|s| s.from.to_uppercase())
        .collect();
    let mut seen = HashSet::new();
    for v in dft.chain(p.values.iter().cloned()).chain(unmapped.cloned()) {
        let t = v.trim_matches('\'');
        if mapped.contains(&v.to_uppercase()) {
            continue;
        }
        if t.chars().count() > len && seen.insert(t.to_string()) {
            out.push(format!("{cmd} {path} LEN({len}): {t}"));
        }
    }
}

/// No shipped definition holds a value its parameter block would cut.
#[test]
fn unmapped_values_fit_their_length() {
    let mut bad = Vec::new();
    for (name, _) in defs::DEFS {
        let def = sys_cpps::install::object(name, 0).unwrap().def;
        for p in &def.parms {
            overlong(name, "", p, &mut bad);
        }
    }
    assert!(bad.is_empty(), "{} values:\n{}", bad.len(), bad.join("\n"));
}

/// `CHGOWN` and `CHGPGP` take `SUBTREE(*NONE|*ALL)`: the analyzer
/// accepts it, the block carries the word the service maps, and the
/// flattened definition (`DescribeCommand`) shows it on the first
/// display with its values.
#[test]
fn chgown_and_chgpgp_take_subtree() {
    use cmd::prompt::flat;
    let mut src = cmd::analyze::MemSource::default();
    for (name, _) in defs::DEFS {
        src.put(sys_cpps::install::object(name, 0).unwrap().def);
    }
    let mut env = cmd::analyze::Env::new(&mut src);
    for (text, want) in [
        ("CHGOWN OBJ('/d') NEWOWN(ANN) SUBTREE(*ALL)", "*ALL"),
        ("CHGOWN OBJ('/d') NEWOWN(ANN)", "*NONE"),
        ("CHGPGP OBJ('/d') NEWPGP(DEV) SUBTREE(*ALL)", "*ALL"),
        ("CHGPGP OBJ('/d') NEWPGP(DEV) SUBTREE(*none)", "*NONE"),
    ] {
        let r = cmd::analyze::analyze(&mut env, text).unwrap_or_else(|e| panic!("{text}: {e}"));
        let built = cmd::block::build(&r.def, &r.parms).unwrap();
        let spec: Vec<bool> = r.parms.iter().map(|b| b.specified).collect();
        let parms = cmd::block::read(&r.def, &built.segments, &spec).unwrap();
        assert_eq!(parms.name("SUBTREE"), want, "{text}");
        let n = flat::nodes(&r.def)
            .into_iter()
            .find(|n| n.keyword == "SUBTREE")
            .expect("SUBTREE");
        assert_eq!(
            (n.default.as_deref(), n.show),
            (Some("*NONE"), flat::Show::Always),
            "{text}"
        );
        assert_eq!(n.values, ["*NONE", "*ALL"], "{text}");
    }
    assert!(
        cmd::analyze::analyze(&mut env, "CHGOWN OBJ('/d') NEWOWN(ANN) SUBTREE(*SOME)").is_err()
    );
}
