// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapdiff -b baseDN -sh host -ch host [options]`: compares a subtree
//! on two servers, the supplier (`-s` options) and the consumer (`-c`
//! options), and prints the LDIF that makes the consumer's match (to
//! `-L file` instead); `-F` applies it to the consumer. `-S` compares
//! the two schemas (`cn=schema`) instead. `-C n` stops after `n`
//! differences. No replication is involved.

use std::collections::BTreeMap;

use ldap::client::Client;
use ldap::ldif::{self, AttrValue, Body, ModSpec, Record, Value};
use ldap::msg::{Attribute, ModOp, Modification, Scope, SearchRequest};
use ldap::{Dn, Filter};
use qsh::sys::UtilCtx;

use super::{Args, PARAM, REFUSED, Tool, close, connect, parse, status_of};
use crate::common::io::open_write;

const NAME: &str = "ldapdiff";

const USAGE: &str = "\
usage: ldapdiff -b baseDN -sh host -ch host [options]; ldapdiff -S -sh host -ch host [options]
  -sh host -sp port -sD dn -sw password -sZ -sK keydb -sP password -sN label
              the supplier; -ch -cp -cD -cw -cZ -cK -cP -cN the consumer
  -b base     the subtree; -S compare the schemas instead
  -F          apply the changes to the consumer
  -L file     write the LDIF there (else standard output)
  -C n        stop after n differences; -v verbose
";

const SPEC: &[&str] = &[
    "sh:", "sp:", "sD:", "sw:", "sZ", "sK:", "sP:", "sN:", "ch:", "cp:", "cD:", "cw:", "cZ", "cK:",
    "cP:", "cN:", "b:", "S", "F", "L:", "C:", "a", "j", "x", "O", "v",
];

/// One side's connection options as a tool's (`-sh` as `-h`, ...).
fn side(a: &Args, p: &str) -> Args {
    let mut out = Args::default();
    for (o, v) in &a.opts {
        if let Some(rest) = o.strip_prefix(p)
            && !rest.is_empty()
        {
            out.opts.push((rest.to_string(), v.clone()));
        }
    }
    if a.has("v") {
        out.opts.push(("v".into(), None));
    }
    out
}

fn norm(dn: &str) -> String {
    Dn::parse(dn).map_or_else(
        |_| dn.to_ascii_lowercase(),
        |d| d.to_string().to_ascii_lowercase(),
    )
}

type Entries = BTreeMap<String, (String, Vec<Attribute>)>;

fn fetch(t: &Tool<'_, '_>, c: &mut Client, base: &str, schema: bool) -> Result<Entries, i32> {
    let (scope, attrs) = if schema {
        (
            Scope::Base,
            [
                "attributeTypes",
                "objectClasses",
                "ldapSyntaxes",
                "matchingRules",
                "IBMAttributeTypes",
            ]
            .iter()
            .map(ToString::to_string)
            .collect(),
        )
    } else {
        (Scope::Sub, vec!["*".to_string()])
    };
    let mut req = SearchRequest::new(base.to_string(), scope, Filter::any());
    req.attrs = attrs;
    let r = c
        .search(&req, &[])
        .map_err(|e| t.fail(status_of(&e), &format!("search: {e}")))?;
    if !r.result.is_success() {
        return Err(t.fail(super::code(&r.result), &super::text(&r.result)));
    }
    Ok(r.entries
        .into_iter()
        .map(|e| (norm(&e.dn), (e.dn, e.attrs)))
        .collect())
}

fn sorted(v: &[Vec<u8>]) -> Vec<Vec<u8>> {
    let mut s = v.to_vec();
    s.sort();
    s
}

/// The changes that make `cons`'s entry `sup`'s.
fn changes(sup: &[Attribute], cons: &[Attribute]) -> Vec<Modification> {
    let find =
        |l: &[Attribute], ty: &str| l.iter().find(|a| a.ty.eq_ignore_ascii_case(ty)).cloned();
    let mut out = Vec::new();
    for s in sup {
        match find(cons, &s.ty) {
            Some(c) if sorted(&c.vals) == sorted(&s.vals) => {}
            _ => out.push(Modification {
                op: ModOp::Replace,
                attr: s.clone(),
            }),
        }
    }
    for c in cons {
        if find(sup, &c.ty).is_none() {
            out.push(Modification {
                op: ModOp::Delete,
                attr: Attribute {
                    ty: c.ty.clone(),
                    vals: Vec::new(),
                },
            });
        }
    }
    out
}

fn record(dn: &str, body: Body) -> Record {
    Record {
        line: 0,
        dn: dn.to_string(),
        controls: Vec::new(),
        body,
    }
}

fn lines(attrs: &[Attribute]) -> Vec<AttrValue> {
    attrs
        .iter()
        .flat_map(|a| {
            a.vals.iter().map(|v| AttrValue {
                ty: a.ty.clone(),
                value: Value::Data(v.clone()),
            })
        })
        .collect()
}

/// Run `ldapdiff`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let t = Tool { cx, name: NAME };
    let a = match parse(&t.cx.argv[1..], SPEC) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    for o in ["a", "j", "x", "O"] {
        if a.has(o) {
            return t.fail(REFUSED, &format!("-{o} is not supported."));
        }
    }
    let schema = a.has("S");
    let base = match (a.value("b"), schema) {
        (_, true) => "cn=schema".to_string(),
        (Some(b), false) => b.to_string(),
        (None, false) => return t.usage("A base DN (-b) is needed, or -S.", USAGE),
    };
    if !a.has("sh") || !a.has("ch") {
        return t.usage(
            "The supplier (-sh) and the consumer (-ch) are needed.",
            USAGE,
        );
    }
    let limit = match a.value("C").map(str::parse::<usize>) {
        None => usize::MAX,
        Some(Ok(n)) if n > 0 => n,
        Some(_) => return t.usage("-C needs a positive number.", USAGE),
    };
    let mut sup = match connect(&t, &side(&a, "s")) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let mut cons = match connect(&t, &side(&a, "c")) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let (se, ce) = match (
        fetch(&t, &mut sup, &base, schema),
        fetch(&t, &mut cons, &base, schema),
    ) {
        (Ok(s), Ok(c)) => (s, c),
        (Err(e), _) | (_, Err(e)) => return e,
    };
    close(sup);
    let mut recs = Vec::new();
    // Adds parents first, deletes children first.
    let mut adds: Vec<&(String, Vec<Attribute>)> = se
        .iter()
        .filter(|(k, _)| !ce.contains_key(*k))
        .map(|(_, v)| v)
        .collect();
    adds.sort_by_key(|(dn, _)| Dn::parse(dn).map_or(0, |d| d.len()));
    for (dn, attrs) in adds {
        recs.push((record(dn, Body::Add(lines(attrs))), None));
    }
    for (k, (dn, sattrs)) in &se {
        if let Some((_, cattrs)) = ce.get(k) {
            let ch = changes(sattrs, cattrs);
            if !ch.is_empty() {
                let specs = ch
                    .iter()
                    .map(|m| ModSpec {
                        op: m.op,
                        ty: m.attr.ty.clone(),
                        values: m.attr.vals.iter().map(|v| Value::Data(v.clone())).collect(),
                    })
                    .collect();
                recs.push((record(dn, Body::Modify(specs)), Some(ch)));
            }
        }
    }
    let mut dels: Vec<&String> = ce
        .iter()
        .filter(|(k, _)| !se.contains_key(*k))
        .map(|(_, (dn, _))| dn)
        .collect();
    dels.sort_by_key(|dn| std::cmp::Reverse(Dn::parse(dn).map_or(0, |d| d.len())));
    if !schema {
        for dn in dels {
            recs.push((record(dn, Body::Delete), None));
        }
    }
    recs.truncate(limit);
    let text = ldif::write(&ldif::Ldif {
        version: Some(1),
        charset: None,
        records: recs.iter().map(|(r, _)| r.clone()).collect(),
    });
    match a.value("L") {
        Some(f) => match open_write(t.cx, f, false) {
            Ok(fd) => {
                let _ = qsh::sys::stream::write_all(&*fd, text.as_bytes());
            }
            Err(e) => return t.fail(PARAM, &format!("{f}: {}", e.errno.text())),
        },
        None => t.say(&text),
    }
    t.verbose(&a, &format!("{} differences", recs.len()));
    let mut status = 0;
    if a.has("F") {
        for (r, ch) in recs {
            let res = match (&r.body, ch) {
                (Body::Add(l), _) => {
                    let at = ldif::group(l, |_| Err::<Vec<u8>, ()>(())).unwrap_or_default();
                    cons.add(&r.dn, at, &[])
                }
                (Body::Delete, _) => cons.delete(&r.dn, &[]),
                (_, Some(ch)) => cons.modify(&r.dn, ch, &[]),
                _ => continue,
            };
            match res {
                Ok(x) if x.result.is_success() => {}
                Ok(x) => {
                    status = t.fail(
                        super::code(&x.result),
                        &format!("{}: {}", r.dn, super::text(&x.result)),
                    );
                }
                Err(e) => status = t.fail(status_of(&e), &format!("{}: {e}", r.dn)),
            }
        }
    }
    close(cons);
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_changes_between_two_entries() {
        let s = vec![
            Attribute::new("cn", &["a"]),
            Attribute::new("sn", &["x", "y"]),
        ];
        let c = vec![
            Attribute::new("CN", &["a"]),
            Attribute::new("sn", &["y"]),
            Attribute::new("mail", &["m"]),
        ];
        let ch = changes(&s, &c);
        assert_eq!(ch.len(), 2);
        assert_eq!((ch[0].op, ch[0].attr.ty.as_str()), (ModOp::Replace, "sn"));
        assert_eq!((ch[1].op, ch[1].attr.ty.as_str()), (ModOp::Delete, "mail"));
        assert_eq!(norm("CN=A, DC=B"), "cn=a,dc=b");
    }
}
