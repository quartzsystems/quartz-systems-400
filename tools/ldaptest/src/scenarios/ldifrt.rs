// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDIF import and export: N generated entries written as LDIF, read back,
//! added; the subtree searched, written as LDIF, read back and compared.

use std::collections::{BTreeMap, HashSet};

use ldap3::{Scope, SearchEntry};

use super::{Ctx, Lines, Scenario};
use crate::error::check;
use crate::ldif::{self, Entry};
use crate::target::Suite;

/// This suite.
pub const LIST: [Scenario; 1] =
    [Scenario { name: "ldif-round-trip", suite: Suite::Ldif, oracle: false, run: ldif_round_trip }];

/// An LDIF entry of a search entry.
pub fn from_search(e: &SearchEntry) -> Entry {
    let mut out = Entry::new(e.dn.clone());
    for (k, vals) in &e.attrs {
        for v in vals {
            out.add(k, v.as_bytes().to_vec());
        }
    }
    for (k, vals) in &e.bin_attrs {
        for v in vals {
            out.add(k, v.clone());
        }
    }
    out
}

fn ldif_round_trip(c: &Ctx) -> Lines {
    let mut sb = c.sandbox("ldif")?;
    let n = c.t.entries;
    let generated = ldif::generate(&sb.dn, n);
    let text = ldif::write(&generated);
    let parsed = ldif::read(&text)?;
    check(parsed.len() == n, || format!("{} entries read of {n}", parsed.len()))?;
    let started = std::time::Instant::now();
    for e in &parsed {
        let attrs: Vec<(Vec<u8>, HashSet<Vec<u8>>)> =
            e.attrs.iter().map(|(k, v)| (k.as_bytes().to_vec(), v.iter().cloned().collect())).collect();
        let r = sb.admin.add_bytes(&e.dn, attrs)?;
        crate::client::expect_rc(&format!("import {}", e.dn), &r, 0)?;
    }
    let import = started.elapsed();
    let f = sb.admin.find(&sb.dn, Scope::OneLevel, "(objectClass=inetOrgPerson)", &["*"])?;
    crate::client::expect_rc("export", &f.res, 0)?;
    let exported: Vec<Entry> = f.entries.iter().map(from_search).collect();
    let back = ldif::read(&ldif::write(&exported))?;
    let by_dn = |es: &[Entry]| -> BTreeMap<String, BTreeMap<String, Vec<Vec<u8>>>> {
        es.iter().map(|e| (e.dn.to_ascii_lowercase(), e.normal())).collect()
    };
    let want = by_dn(&generated);
    let got = by_dn(&back);
    check(want.len() == got.len(), || format!("{} exported of {}", got.len(), want.len()))?;
    let mut diffs = Vec::new();
    for (dn, attrs) in &want {
        match got.get(dn) {
            None => diffs.push(format!("{dn}: missing")),
            Some(g) => {
                for (k, v) in attrs {
                    let mut gv = g.get(k).cloned().unwrap_or_default();
                    let mut wv = v.clone();
                    if k == "objectclass" {
                        // Values compare case-insensitively.
                        for x in &mut gv {
                            x.make_ascii_lowercase();
                        }
                        for x in &mut wv {
                            x.make_ascii_lowercase();
                        }
                        gv.sort();
                        wv.sort();
                    }
                    if gv != wv {
                        diffs.push(format!("{dn} {k}"));
                    }
                }
            }
        }
    }
    check(diffs.is_empty(), || format!("{} differences, first {}", diffs.len(), diffs[0]))?;
    let b64 = text.lines().filter(|l| l.contains(":: ")).count();
    Ok(vec![
        format!("{n} entries imported in {:.0} ms, exported and compared", import.as_secs_f64() * 1e3),
        format!("{b64} base64 lines, LDIF {} bytes", text.len()),
    ])
}
