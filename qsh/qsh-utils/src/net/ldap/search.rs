// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ldapsearch [options] filter [attributes...]`: entries as `dn` then
//! `attribute=value` lines (`-F` another separator), or LDIF with `-L`.
//! The base is `-b`, else `$LDAP_BASEDN`. `-q` pages, `-T` waits between
//! pages, `-o` sorts on the server, `-c` asks for a persistent search,
//! `-f` runs the filter once per line of a file (`%s` in the filter is
//! the line).

use ldap::Filter;
use ldap::control::{
    Control, ControlValue, EntryChange, PagedResults, PersistentSearch, SortKey, SortRequest,
    change_type,
};
use ldap::ldif::{self, AttrValue, Body, Record, Value};
use ldap::msg::{Attribute, DerefAliases, ProtocolOp, Scope, SearchRequest};
use qsh::sys::UtilCtx;

use super::{Args, CONNECT, REFUSED, Tool, close, connect, controls, input, parse, status_of};
use crate::common::io::open_write;

const NAME: &str = "ldapsearch";

const USAGE: &str = "\
usage: ldapsearch [options] filter [attributes...]
  -b base     the base entry (else $LDAP_BASEDN)
  -s scope    base, one or sub (sub)
  -a deref    never, always, search or find
  -z n        the most entries; -l n the most seconds
  -A          attribute names only
  -L          print LDIF; -F sep separates names and values otherwise
  -B          print values that are not text as they are
  -t          write each value to a file and print its name
  -q n        ask for pages of n entries; -T s waits s seconds between them
  -o attr     sort on the server by attr (-attr reverses); repeatable
  -c types    persistent search: any, or add,delete,modify,moddn
  -f file     a filter per line (%s in the filter is the line)
  -n          show the search, do not run it; -v verbose; -e version
connection: -h host -p port -D dn -w password|? -V 2|3 -Z (LDAPS) -Y (StartTLS)
  -K keydb -P password -N label -m EXTERNAL -y proxydn -M -R -O hops -C charset
";

fn spec() -> Vec<&'static str> {
    let mut s = CONNECT.to_vec();
    s.extend([
        "a:", "A", "b:", "B", "c:", "e", "f:", "F:", "i:", "j:", "J:", "l:", "L", "o:", "q:", "s:",
        "t", "T:", "z:",
    ]);
    s
}

fn scope(s: &str) -> Option<Scope> {
    match s.to_ascii_lowercase().as_str() {
        "base" => Some(Scope::Base),
        "one" | "onelevel" => Some(Scope::One),
        "sub" | "subtree" => Some(Scope::Sub),
        _ => None,
    }
}

fn deref(s: &str) -> Option<DerefAliases> {
    match s.to_ascii_lowercase().as_str() {
        "never" => Some(DerefAliases::Never),
        "always" => Some(DerefAliases::Always),
        "search" => Some(DerefAliases::InSearching),
        "find" => Some(DerefAliases::FindingBase),
        _ => None,
    }
}

/// Whether a value prints as text.
#[must_use]
pub fn printable(v: &[u8]) -> bool {
    std::str::from_utf8(v).is_ok_and(|s| s.chars().all(|c| !c.is_control() || c == '\t'))
}

/// How entries are printed.
struct Out {
    ldif: bool,
    sep: String,
    raw: bool,
    files: bool,
    types_only: bool,
    count: usize,
}

impl Out {
    fn entry(&mut self, t: &Tool<'_, '_>, dn: &str, attrs: &[Attribute]) {
        let mut s = String::new();
        if self.count > 0 {
            s.push('\n');
        }
        self.count += 1;
        if self.ldif {
            let lines: Vec<AttrValue> = attrs
                .iter()
                .flat_map(|a| {
                    let vals: Vec<Vec<u8>> = if self.types_only {
                        vec![Vec::new()]
                    } else {
                        a.vals.clone()
                    };
                    vals.into_iter().map(move |v| AttrValue {
                        ty: a.ty.clone(),
                        value: Value::Data(v),
                    })
                })
                .map(|mut l| {
                    if self.files
                        && let Value::Data(d) = &l.value
                        && let Some(p) = self.file(t, &l.ty, d)
                    {
                        l.value = Value::Url(format!("file://{p}"));
                    }
                    l
                })
                .collect();
            let r = Record {
                line: 0,
                dn: dn.to_string(),
                controls: Vec::new(),
                body: Body::Content(lines),
            };
            s.push_str(&ldif::write_one(&r));
            s.push('\n');
        } else {
            s.push_str(dn);
            s.push('\n');
            for a in attrs {
                if self.types_only {
                    s.push_str(&a.ty);
                    s.push('\n');
                    continue;
                }
                for v in &a.vals {
                    let shown = if self.files {
                        self.file(t, &a.ty, v).unwrap_or_default()
                    } else if self.raw || printable(v) {
                        String::from_utf8_lossy(v).into_owned()
                    } else {
                        format!("(a binary value of {} bytes)", v.len())
                    };
                    s.push_str(&format!("{}{}{shown}\n", a.ty, self.sep));
                }
            }
        }
        t.say(&s);
    }

    /// `-t`: the value written to a file; its path.
    fn file(&self, t: &Tool<'_, '_>, ty: &str, v: &[u8]) -> Option<String> {
        let dir =
            t.cx.getenv("TMPDIR")
                .unwrap_or("/tmp")
                .trim_end_matches('/')
                .to_string();
        let path = format!("{dir}/ldapsearch-{ty}-{}-{}", self.count, v.len());
        let fd = open_write(t.cx, &path, false).ok()?;
        qsh::sys::stream::write_all(&*fd, v).ok()?;
        Some(path)
    }

    fn reference(&self, t: &Tool<'_, '_>, urls: &[String]) {
        for u in urls {
            if self.ldif {
                t.say(&format!("# reference: {u}\n"));
            } else {
                t.say(&format!("reference{}{u}\n", self.sep));
            }
        }
    }
}

fn change_types(s: &str) -> Option<(i32, bool, bool)> {
    let mut parts = s.split(':');
    let mut bits = 0;
    for w in parts.next()?.split(',') {
        bits |= match w.trim().to_ascii_lowercase().as_str() {
            "any" | "all" => change_type::ALL,
            "add" => change_type::ADD,
            "delete" => change_type::DELETE,
            "modify" => change_type::MODIFY,
            "moddn" | "modrdn" => change_type::MODDN,
            _ => return None,
        };
    }
    let flag = |p: Option<&str>, d: bool| match p.map(str::to_ascii_lowercase).as_deref() {
        None => Some(d),
        Some("true" | "yes" | "1") => Some(true),
        Some("false" | "no" | "0") => Some(false),
        Some(_) => None,
    };
    let only = flag(parts.next(), true)?;
    let ecs = flag(parts.next(), true)?;
    Some((bits, only, ecs))
}

/// Run `ldapsearch`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let t = Tool { cx, name: NAME };
    let a = match parse(&t.cx.argv[1..], &spec()) {
        Ok(a) => a,
        Err(e) => return t.usage(&e, USAGE),
    };
    if a.has("e") {
        t.say(&format!("{NAME}: QS/400 LDAP tools, {}\n", release::full()));
        return 0;
    }
    for o in ["j", "J"] {
        if a.has(o) {
            return t.fail(
                REFUSED,
                &format!("-{o} (IBM's limit on attribute values) is not supported."),
            );
        }
    }
    let mut ops = a.operands.clone();
    let from_file = a.has("f") || a.has("i");
    let pattern = if ops.is_empty() {
        if from_file {
            "%s".to_string()
        } else {
            return t.usage("A filter is needed.", USAGE);
        }
    } else {
        ops.remove(0)
    };
    let attrs = ops;
    let base = a
        .value("b")
        .map(str::to_string)
        .or_else(|| t.cx.getenv("LDAP_BASEDN").map(str::to_string))
        .unwrap_or_default();
    let Some(sc) = scope(a.value("s").unwrap_or("sub")) else {
        return t.usage(
            &format!("The scope {} is not valid.", a.value("s").unwrap_or("")),
            USAGE,
        );
    };
    let Some(dr) = deref(a.value("a").unwrap_or("never")) else {
        return t.usage("The alias dereferencing is not valid.", USAGE);
    };
    let num = |o: &str| -> Result<i32, i32> {
        a.value(o).map_or(Ok(0), |v| {
            v.parse::<i32>()
                .ok()
                .filter(|n| *n >= 0)
                .ok_or_else(|| t.usage(&format!("-{o} {v} is not a number."), USAGE))
        })
    };
    let (size, time, page, wait) = match (num("z"), num("l"), num("q"), num("T")) {
        (Ok(z), Ok(l), Ok(q), Ok(w)) => (z, l, q, w),
        (Err(e), ..) | (_, Err(e), ..) | (_, _, Err(e), _) | (.., Err(e)) => return e,
    };
    let filters: Vec<String> = if from_file {
        let b = match input(&t, &a) {
            Ok(b) => b,
            Err(e) => return e,
        };
        String::from_utf8_lossy(&b)
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| pattern.replace("%s", l.trim()))
            .collect()
    } else {
        vec![pattern]
    };
    let mut parsed = Vec::new();
    for f in &filters {
        match Filter::parse(f) {
            Ok(p) => parsed.push(p),
            Err(e) => return t.fail(0x57, &format!("The filter {f} is not valid: {e}.")),
        }
    }
    let mut ctrls = controls(&a);
    let keys: Vec<SortKey> = a
        .values("o")
        .iter()
        .map(|k| {
            let (reverse, k) = k.strip_prefix('-').map_or((false, *k), |r| (true, r));
            let (attr, rule) = k
                .split_once(':')
                .map_or((k, None), |(x, r)| (x, Some(r.to_string())));
            SortKey {
                attr: attr.to_string(),
                rule,
                reverse,
            }
        })
        .collect();
    if !keys.is_empty() {
        ctrls.push(SortRequest(keys).to_control(true));
    }
    let psearch = match a.value("c").map(change_types) {
        None => None,
        Some(Some(p)) => Some(p),
        Some(None) => return t.usage("The change types of -c are not valid.", USAGE),
    };
    if a.has("n") {
        for f in &filters {
            t.say(&format!(
                "{NAME}: would search {base:?}, scope {sc:?}, filter {f}\n"
            ));
        }
        return 0;
    }
    let mut c = match connect(&t, &a) {
        Ok(c) => c,
        Err(e) => return e,
    };
    let mut out = Out {
        ldif: a.has("L"),
        sep: a.value("F").unwrap_or("=").to_string(),
        raw: a.has("B"),
        files: a.has("t"),
        types_only: a.has("A"),
        count: 0,
    };
    let mut status = 0;
    for f in parsed {
        let mut req = SearchRequest::new(base.clone(), sc, f);
        req.deref = dr;
        req.size_limit = size;
        req.time_limit = time;
        req.types_only = a.has("A");
        req.attrs.clone_from(&attrs);
        t.verbose(&a, &format!("searching {:?} with {}", req.base, req.filter));
        status = if let Some((bits, only, ecs)) = psearch {
            persistent(&t, &mut c, req, &ctrls, (bits, only, ecs), &mut out)
        } else if page > 0 {
            paged(&t, &mut c, &req, &ctrls, page, wait, &mut out)
        } else {
            match c.search(&req, &ctrls) {
                Ok(r) => {
                    for e in &r.entries {
                        out.entry(&t, &e.dn, &e.attrs);
                    }
                    for refs in &r.references {
                        out.reference(&t, refs);
                    }
                    finish(&t, &r.result)
                }
                Err(e) => t.fail(status_of(&e), &format!("search: {e}")),
            }
        };
    }
    close(c);
    status
}

fn finish(t: &Tool<'_, '_>, r: &ldap::LdapResult) -> i32 {
    if r.is_success() {
        0
    } else {
        t.fail(super::code(r), &super::text(r))
    }
}

fn paged(
    t: &Tool<'_, '_>,
    c: &mut ldap::client::Client,
    req: &SearchRequest,
    ctrls: &[Control],
    size: i32,
    wait: i32,
    out: &mut Out,
) -> i32 {
    let mut cookie = Vec::new();
    loop {
        match c.search_page(req, size, &cookie, ctrls) {
            Ok((r, next)) => {
                for e in &r.entries {
                    out.entry(t, &e.dn, &e.attrs);
                }
                for refs in &r.references {
                    out.reference(t, refs);
                }
                if !r.result.is_success() {
                    return finish(t, &r.result);
                }
                if next.is_empty() {
                    if PagedResults::find(&r.controls).ok().flatten().is_none() {
                        t.verbose(&Args::default(), "the server did not page the results");
                    }
                    return 0;
                }
                cookie = next;
                if wait > 0 {
                    t.cx.sys.sleep(std::time::Duration::from_secs(u64::from(
                        wait.unsigned_abs(),
                    )));
                }
            }
            Err(e) => return t.fail(status_of(&e), &format!("search: {e}")),
        }
    }
}

fn persistent(
    t: &Tool<'_, '_>,
    c: &mut ldap::client::Client,
    req: SearchRequest,
    ctrls: &[Control],
    (bits, only, ecs): (i32, bool, bool),
    out: &mut Out,
) -> i32 {
    let mut ctrls = ctrls.to_vec();
    ctrls.push(
        PersistentSearch {
            change_types: bits,
            changes_only: only,
            return_ecs: ecs,
        }
        .to_control(true),
    );
    let id = match c.send(ProtocolOp::SearchRequest(req), ctrls) {
        Ok(id) => id,
        Err(e) => return t.fail(status_of(&e), &format!("search: {e}")),
    };
    loop {
        match c.poll(Some(id), None) {
            Ok(Some(m)) => match m.op {
                ProtocolOp::SearchResultEntry { dn, attrs } => {
                    if let Ok(Some(ec)) = EntryChange::find(&m.controls) {
                        let what = match ec.change_type {
                            change_type::ADD => "add",
                            change_type::DELETE => "delete",
                            change_type::MODDN => "moddn",
                            _ => "modify",
                        };
                        if out.ldif {
                            t.say(&format!("# change: {what}\n"));
                        } else {
                            t.say(&format!("change{}{what}\n", out.sep));
                        }
                    }
                    out.entry(t, &dn, &attrs);
                }
                ProtocolOp::SearchResultReference(u) => out.reference(t, &u),
                ProtocolOp::SearchResultDone(r) => return finish(t, &r),
                _ => {}
            },
            Ok(None) => return 0,
            // The server ending the connection ends the search.
            Err(ldap::client::ClientError::Closed) => return 0,
            Err(e) => return t.fail(status_of(&e), &format!("search: {e}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn change_type_lists() {
        assert_eq!(change_types("any"), Some((15, true, true)));
        assert_eq!(change_types("add,delete:false"), Some((3, false, true)));
        assert_eq!(change_types("bogus"), None);
        assert!(printable("Ärger\tx".as_bytes()));
        assert!(!printable(&[0xFF, 0x00]));
        assert_eq!(super::super::PARAM, 89);
    }
}
