// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Filters evaluated with the schema's matching rules (RFC 4511 §4.5.1.7),
//! three-valued: `Some(true)`, `Some(false)`, `None` for Undefined.
//!
//! - An attribute matches through its subtypes (`(name=x)` meets `cn`).
//! - An unknown attribute, a missing rule or an attribute the requester
//!   may not search (`s`) is Undefined.
//! - `~=` is QS/400's approximate rule on the equality-normalized values:
//!   equal, or a common prefix of at least four characters and three
//!   quarters of the shorter, or an edit distance of at most a quarter of
//!   the shorter (IBM's matches `(cn~=ibmpolicy)` to `IBMpolicies`).
//! - `(&)` and `(|)` match nothing, as IBM's server answers (RFC 4526 is
//!   not advertised).
//! - An attribute defined without a rule takes the first rule of its
//!   syntax (IBM's `createTimestamp` orders without one).
//! - Extensible match runs the named rule (QS/400's addition: IBM's
//!   matches nothing); an ordering rule matches values below the
//!   assertion, a substrings rule takes `*` in the assertion.

use std::cmp::Ordering;

use ldap::dn::Dn;
use ldap::filter::{Extensible, Filter, Substrings};
use ldap::msg::Attribute;

use crate::schema::check::split_description;
use crate::schema::rule::{self, Kind, Rule};
use crate::schema::{AttributeType, Schema};

/// What a filter is evaluated against.
pub struct Target<'a> {
    /// The entry's DN.
    pub dn: &'a Dn,
    /// Its attributes, the computed operational ones the filter names
    /// included.
    pub attrs: &'a [Attribute],
    /// Whether the requester may search an attribute.
    pub can_search: &'a dyn Fn(&AttributeType) -> bool,
}

fn and3(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(false), _) | (_, Some(false)) => Some(false),
        (Some(true), Some(true)) => Some(true),
        _ => None,
    }
}

fn or3(a: Option<bool>, b: Option<bool>) -> Option<bool> {
    match (a, b) {
        (Some(true), _) | (_, Some(true)) => Some(true),
        (Some(false), Some(false)) => Some(false),
        _ => None,
    }
}

/// The values of `at` and its subtypes in `attrs`.
fn values<'a>(schema: &Schema, attrs: &'a [Attribute], at: &AttributeType) -> Vec<&'a [u8]> {
    attrs
        .iter()
        .filter(|a| {
            schema
                .attribute(split_description(&a.ty).0)
                .is_some_and(|t| schema.is_subtype_of(t, at.oid()))
        })
        .flat_map(|a| a.vals.iter().map(Vec::as_slice))
        .collect()
}

/// Evaluate `f` against `t`.
#[must_use]
pub fn eval(schema: &Schema, f: &Filter, t: &Target<'_>) -> Option<bool> {
    match f {
        Filter::And(fs) => {
            if fs.is_empty() {
                return Some(false);
            }
            fs.iter()
                .fold(Some(true), |acc, g| and3(acc, eval(schema, g, t)))
        }
        Filter::Or(fs) => fs
            .iter()
            .fold(Some(false), |acc, g| or3(acc, eval(schema, g, t))),
        Filter::Not(g) => eval(schema, g, t).map(|b| !b),
        Filter::Present(a) => {
            let at = schema.attribute(split_description(a).0)?;
            if at.oid() == "2.5.4.0" {
                return Some(true);
            }
            if !(t.can_search)(at) {
                return None;
            }
            Some(!values(schema, t.attrs, at).is_empty())
        }
        Filter::Equality(a, v) => with_rule(schema, a, t, Kind::Equality, |at| at.equality, |r, x| {
            r.equals(schema, x, v)
        }),
        Filter::Approx(a, v) => with_rule(schema, a, t, Kind::Equality, |at| at.equality, |r, x| {
            approx(schema, r, x, v)
        }),
        Filter::GreaterOrEqual(a, v) => with_rule(schema, a, t, Kind::Ordering, |at| at.ordering, |r, x| {
            r.compare(schema, x, v).map(|o| o != Ordering::Less)
        }),
        Filter::LessOrEqual(a, v) => with_rule(schema, a, t, Kind::Ordering, |at| at.ordering, |r, x| {
            r.compare(schema, x, v).map(|o| o != Ordering::Greater)
        }),
        Filter::Substrings(a, s) => with_rule(schema, a, t, Kind::Substring, |at| at.substr, |r, x| {
            r.substrings(x, s)
        }),
        Filter::Extensible(e) => extensible(schema, e, t),
    }
}

/// The first rule of `kind` for the attribute's syntax, for an attribute
/// defined without one (IBM's `createTimestamp` has none, yet orders).
pub(crate) fn by_syntax(at: &AttributeType, kind: Kind) -> Option<&'static Rule> {
    let syn = at.syntax.as_deref()?;
    rule::RULES.iter().find(|r| r.kind == kind && r.syntax == syn)
}

/// Any value of attribute `a` matching under the rule `pick` gives.
fn with_rule(
    schema: &Schema,
    a: &str,
    t: &Target<'_>,
    kind: Kind,
    pick: impl Fn(&AttributeType) -> Option<&'static Rule>,
    test: impl Fn(&Rule, &[u8]) -> Option<bool>,
) -> Option<bool> {
    let at = schema.attribute(split_description(a).0)?;
    if !(t.can_search)(at) {
        return None;
    }
    let r = pick(at).or_else(|| by_syntax(at, kind))?;
    values(schema, t.attrs, at)
        .into_iter()
        .fold(Some(false), |acc, v| or3(acc, test(r, v)))
}

fn edit_distance(a: &[char], b: &[char]) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = (prev[j] + usize::from(ca != cb))
                .min(prev[j + 1] + 1)
                .min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

/// QS/400's approximate match (see the module's notes).
fn approx(schema: &Schema, r: &Rule, value: &[u8], assertion: &[u8]) -> Option<bool> {
    let v = r.normalize(schema, value).ok()?;
    let a = r.normalize_assertion(schema, assertion).ok()?;
    if v == a {
        return Some(true);
    }
    let v: Vec<char> = String::from_utf8_lossy(&v).chars().collect();
    let a: Vec<char> = String::from_utf8_lossy(&a).chars().collect();
    let short = v.len().min(a.len());
    if short == 0 {
        return Some(false);
    }
    let prefix = v.iter().zip(&a).take_while(|(x, y)| x == y).count();
    if prefix >= 4 && prefix * 4 >= short * 3 {
        return Some(true);
    }
    Some(edit_distance(&v, &a) * 4 <= short)
}

/// A substrings assertion written with `*` (an extensible match's value
/// under a substrings rule).
fn split_stars(v: &[u8]) -> Substrings {
    let parts: Vec<&[u8]> = v.split(|&b| b == b'*').collect();
    let n = parts.len();
    let keep = |p: &[u8]| (!p.is_empty()).then(|| p.to_vec());
    Substrings {
        initial: keep(parts[0]),
        any: if n > 2 {
            parts[1..n - 1]
                .iter()
                .filter(|p| !p.is_empty())
                .map(|p| p.to_vec())
                .collect()
        } else {
            Vec::new()
        },
        fin: if n > 1 { keep(parts[n - 1]) } else { None },
    }
}

fn apply(schema: &Schema, r: &Rule, value: &[u8], assertion: &[u8]) -> Option<bool> {
    match r.kind {
        Kind::Equality => r.equals(schema, value, assertion),
        Kind::Ordering => r.compare(schema, value, assertion).map(|o| o == Ordering::Less),
        Kind::Substring => r.substrings(value, &split_stars(assertion)),
    }
}

fn extensible(schema: &Schema, e: &Extensible, t: &Target<'_>) -> Option<bool> {
    let named = match &e.rule {
        Some(n) => Some(rule::find(n)?),
        None => None,
    };
    let mut acc = Some(false);
    match &e.attr {
        Some(a) => {
            let at = schema.attribute(split_description(a).0)?;
            if !(t.can_search)(at) {
                return None;
            }
            let r = named.or(at.equality)?;
            for v in values(schema, t.attrs, at) {
                acc = or3(acc, apply(schema, r, v, &e.value));
            }
        }
        None => {
            let r = named?;
            for a in t.attrs {
                let Some(at) = schema.attribute(split_description(&a.ty).0) else {
                    continue;
                };
                let fits = at.syntax.as_deref() == Some(r.syntax)
                    || [at.equality, at.ordering, at.substr]
                        .iter()
                        .flatten()
                        .any(|x| x.oid == r.oid);
                if !fits || !(t.can_search)(at) {
                    continue;
                }
                for v in &a.vals {
                    acc = or3(acc, apply(schema, r, v, &e.value));
                }
            }
        }
    }
    if e.dn_attributes {
        for rdn in &t.dn.0 {
            for ava in &rdn.0 {
                let Some(at) = schema.attribute(&ava.ty) else {
                    continue;
                };
                if let Some(a) = &e.attr
                    && !schema.attribute(a).is_some_and(|f| schema.is_subtype_of(at, f.oid()))
                {
                    continue;
                }
                let Some(r) = named.or(at.equality) else {
                    continue;
                };
                acc = or3(acc, apply(schema, r, &ava.value, &e.value));
            }
        }
    }
    acc
}

/// Every attribute description a filter names.
pub fn names<'a>(f: &'a Filter, out: &mut Vec<&'a str>) {
    match f {
        Filter::And(fs) | Filter::Or(fs) => fs.iter().for_each(|g| names(g, out)),
        Filter::Not(g) => names(g, out),
        Filter::Equality(a, _)
        | Filter::Approx(a, _)
        | Filter::GreaterOrEqual(a, _)
        | Filter::LessOrEqual(a, _)
        | Filter::Substrings(a, _)
        | Filter::Present(a) => out.push(a),
        Filter::Extensible(e) => {
            if let Some(a) = &e.attr {
                out.push(a);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: &str, attrs: &[(&str, &[&str])]) -> Option<bool> {
        let s = Schema::shipped().unwrap();
        let attrs: Vec<Attribute> = attrs.iter().map(|(t, v)| Attribute::new(t, v)).collect();
        let dn = Dn::parse("cn=IBMpolicies,o=Acme").unwrap();
        let all = |_: &AttributeType| true;
        eval(
            &s,
            &Filter::parse(f).unwrap(),
            &Target {
                dn: &dn,
                attrs: &attrs,
                can_search: &all,
            },
        )
    }

    #[test]
    fn evaluates() {
        let e: &[(&str, &[&str])] = &[
            ("objectClass", &["top", "container"]),
            ("cn", &["IBMpolicies"]),
            ("createTimestamp", &["20260914115805.748334Z"]),
        ];
        assert_eq!(run("(cn=ibmpolicies)", e), Some(true));
        assert_eq!(run("(name=IBMPOLICIES)", e), Some(true));
        assert_eq!(run("(cn~=ibmpolicy)", e), Some(true));
        assert_eq!(run("(cn~=localhost)", e), Some(false));
        assert_eq!(run("(cn=p*d*y)", e), Some(false));
        assert_eq!(run("(cn=*pol*)", e), Some(true));
        assert_eq!(run("(cn>=m)", e), Some(false));
        assert_eq!(run("(cn<=m)", e), Some(true));
        assert_eq!(run("(nosuchattribute=x)", e), None);
        assert_eq!(run("(!(nosuchattribute=x))", e), None);
        assert_eq!(run("(&)", e), Some(false));
        assert_eq!(run("(|)", e), Some(false));
        assert_eq!(run("(createTimestamp>=19700101000000Z)", e), Some(true));
        assert_eq!(run("(cn:caseExactMatch:=IBMpolicies)", e), Some(true));
        assert_eq!(run("(cn:caseExactMatch:=ibmpolicies)", e), Some(false));
        assert_eq!(run("(:dn:2.5.13.2:=acme)", e), Some(true));
        assert_eq!(run("(o:dn:=acme)", e), Some(true));
        assert_eq!(run("(cn:caseIgnoreSubstringsMatch:=ibm\\2as)", e), Some(true));
        assert_eq!(run("(objectClass=*)", e), Some(true));
        assert_eq!(run("(description=*)", e), Some(false));
    }
}
