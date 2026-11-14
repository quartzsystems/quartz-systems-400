// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LDIF (RFC 2849): content and change records, folding, base64 and
//! URL values (docs/DIRSRV.md §3).
//!
//! [`parse`] reads a file into [`Ldif`]; [`write`] prints one. A `:<`
//! value comes back as [`Value::Url`]: the caller fetches it. IBM's files
//! may start with a `charset:` line before `version:`; it is accepted and
//! kept in [`Ldif::charset`]. Content and change records may be mixed.

use crate::msg::{Attribute, ModOp};

/// A parsed LDIF file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ldif {
    /// The `version:` line's number, if there was one.
    pub version: Option<u32>,
    /// IBM's `charset:` line's value, if there was one.
    pub charset: Option<String>,
    /// The records, in order.
    pub records: Vec<Record>,
}

/// A value: inline bytes or a URL to read them from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// The bytes (`:` or `::`).
    Data(Vec<u8>),
    /// A URL (`:<`), not resolved.
    Url(String),
}

/// One `type: value` line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrValue {
    /// The attribute description (type and options).
    pub ty: String,
    /// The value.
    pub value: Value,
}

/// A `control:` line of a change record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LdifControl {
    /// The control's OID.
    pub oid: String,
    /// Its criticality.
    pub critical: bool,
    /// Its value, if any.
    pub value: Option<Value>,
}

/// One modification of a `changetype: modify` record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModSpec {
    /// The operation.
    pub op: ModOp,
    /// The attribute description.
    pub ty: String,
    /// The values (none deletes the attribute, or replaces with nothing).
    pub values: Vec<Value>,
}

/// What a record says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// A content record: the entry's attribute lines.
    Content(Vec<AttrValue>),
    /// `changetype: add`.
    Add(Vec<AttrValue>),
    /// `changetype: delete`.
    Delete,
    /// `changetype: modify`.
    Modify(Vec<ModSpec>),
    /// `changetype: modrdn` or `moddn`.
    ModDn {
        /// The new RDN.
        new_rdn: String,
        /// Whether the old RDN's values go.
        delete_old_rdn: bool,
        /// The new superior, if it moves.
        new_superior: Option<String>,
    },
}

/// One record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The line its `dn:` was on (1-based); 0 for records built in code.
    pub line: usize,
    /// The DN as written.
    pub dn: String,
    /// The `control:` lines (change records only).
    pub controls: Vec<LdifControl>,
    /// The content or the change.
    pub body: Body,
}

/// An LDIF error and the line it was found on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("LDIF line {line}: {what}")]
pub struct LdifError {
    /// The line (1-based).
    pub line: usize,
    /// What was wrong.
    pub what: String,
}

fn err<T>(line: usize, what: impl Into<String>) -> Result<T, LdifError> {
    Err(LdifError {
        line,
        what: what.into(),
    })
}

/// Groups attribute lines into attributes (values in order, first
/// appearance of each type, compared ignoring ASCII case), reading each
/// URL value through `fetch`.
///
/// # Errors
///
/// What `fetch` returns.
pub fn group<E>(
    lines: &[AttrValue],
    mut fetch: impl FnMut(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<Attribute>, E> {
    let mut out: Vec<Attribute> = Vec::new();
    for l in lines {
        let v = match &l.value {
            Value::Data(d) => d.clone(),
            Value::Url(u) => fetch(u)?,
        };
        if let Some(a) = out.iter_mut().find(|a| a.ty.eq_ignore_ascii_case(&l.ty)) {
            a.vals.push(v);
        } else {
            out.push(Attribute {
                ty: l.ty.clone(),
                vals: vec![v],
            });
        }
    }
    Ok(out)
}

/// One logical (unfolded) line and where it started.
struct Line {
    no: usize,
    text: Vec<u8>,
}

/// Unfolds `src` into paragraphs of logical lines, comments dropped.
fn paragraphs(src: &[u8]) -> Vec<Vec<Line>> {
    let mut paras = Vec::new();
    let mut cur: Vec<Line> = Vec::new();
    // Whether the last physical line was a comment (folds continue it).
    let mut in_comment = false;
    for (i, raw) in src.split(|&b| b == b'\n').enumerate() {
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        let no = i + 1;
        if let Some(rest) = raw.strip_prefix(b" ") {
            if in_comment {
                continue;
            }
            if let Some(last) = cur.last_mut() {
                last.text.extend_from_slice(rest);
                continue;
            }
            // A leading space with nothing to continue: a line of its own.
            cur.push(Line {
                no,
                text: raw.to_vec(),
            });
            continue;
        }
        in_comment = false;
        if raw.is_empty() {
            if !cur.is_empty() {
                paras.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if raw[0] == b'#' {
            in_comment = true;
            continue;
        }
        cur.push(Line {
            no,
            text: raw.to_vec(),
        });
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras
}

/// A line split into its type and value.
fn split(l: &Line) -> Result<(String, Value), LdifError> {
    let t = &l.text;
    let Some(colon) = t.iter().position(|&b| b == b':') else {
        return err(l.no, "no ':' in the line");
    };
    let ty = std::str::from_utf8(&t[..colon])
        .map_err(|_| LdifError {
            line: l.no,
            what: "attribute type is not UTF-8".into(),
        })?
        .trim()
        .to_string();
    if ty.is_empty() {
        return err(l.no, "empty attribute type");
    }
    let rest = &t[colon + 1..];
    let value = match rest.first() {
        Some(b':') => {
            let b = trim_spaces(&rest[1..]);
            match base64_decode(b) {
                Some(v) => Value::Data(v),
                None => return err(l.no, "bad base64 value"),
            }
        }
        Some(b'<') => {
            let u = trim_spaces(&rest[1..]);
            Value::Url(String::from_utf8_lossy(u).into_owned())
        }
        _ => {
            let mut v = rest;
            while let Some((b' ', r)) = v.split_first() {
                v = r;
            }
            Value::Data(v.to_vec())
        }
    };
    Ok((ty, value))
}

fn trim_spaces(mut b: &[u8]) -> &[u8] {
    while let Some((b' ', r)) = b.split_first() {
        b = r;
    }
    while let Some((b' ', r)) = b.split_last() {
        b = r;
    }
    b
}

fn data_text(no: usize, what: &str, v: Value) -> Result<String, LdifError> {
    match v {
        Value::Data(d) => String::from_utf8(d).or_else(|_| err(no, format!("{what} is not UTF-8"))),
        Value::Url(_) => err(no, format!("{what} cannot be a URL")),
    }
}

/// Parses an LDIF file.
///
/// # Errors
///
/// An [`LdifError`] with the line of the first problem.
pub fn parse(src: &[u8]) -> Result<Ldif, LdifError> {
    let mut out = Ldif::default();
    let mut header = true;
    for para in paragraphs(src) {
        let mut lines = para.into_iter().peekable();
        while header {
            let Some(l) = lines.peek() else { break };
            let (ty, v) = split(l)?;
            let no = l.no;
            if ty.eq_ignore_ascii_case("charset") && out.charset.is_none() && out.version.is_none()
            {
                out.charset = Some(data_text(no, "charset", v)?.trim().to_string());
            } else if ty.eq_ignore_ascii_case("version") && out.version.is_none() {
                let s = data_text(no, "version", v)?;
                match s.trim().parse::<u32>() {
                    Ok(1) => out.version = Some(1),
                    _ => return err(no, format!("unsupported LDIF version {}", s.trim())),
                }
            } else {
                header = false;
                break;
            }
            lines.next();
        }
        let rest: Vec<Line> = lines.collect();
        if rest.is_empty() {
            continue;
        }
        header = false;
        out.records.push(record(&rest)?);
    }
    Ok(out)
}

fn parse_bool01(no: usize, s: &str) -> Result<bool, LdifError> {
    match s.trim() {
        "0" | "false" | "FALSE" => Ok(false),
        "1" | "true" | "TRUE" => Ok(true),
        o => err(no, format!("deleteoldrdn must be 0 or 1, not {o}")),
    }
}

fn record(lines: &[Line]) -> Result<Record, LdifError> {
    let first = &lines[0];
    let (ty, v) = split(first)?;
    if !ty.eq_ignore_ascii_case("dn") {
        return err(first.no, "a record must start with dn:");
    }
    let dn = data_text(first.no, "the DN", v)?;
    let mut i = 1;
    let mut controls = Vec::new();
    while let Some(l) = lines.get(i) {
        let (ty, v) = split(l)?;
        if !ty.eq_ignore_ascii_case("control") {
            break;
        }
        controls.push(control(l.no, v)?);
        i += 1;
    }
    let change = match lines.get(i) {
        Some(l) => {
            let (ty, v) = split(l)?;
            if ty.eq_ignore_ascii_case("changetype") {
                i += 1;
                Some((l.no, data_text(l.no, "changetype", v)?.trim().to_ascii_lowercase()))
            } else {
                None
            }
        }
        None => None,
    };
    if !controls.is_empty() && change.is_none() {
        return err(lines[i.min(lines.len() - 1)].no, "control: without changetype:");
    }
    let rest = &lines[i..];
    let body = match change {
        None => Body::Content(attr_lines(rest)?),
        Some((_, ct)) if ct == "add" => Body::Add(attr_lines(rest)?),
        Some((_, ct)) if ct == "delete" => {
            if let Some(l) = rest.first() {
                return err(l.no, "a delete record has no more lines");
            }
            Body::Delete
        }
        Some((_, ct)) if ct == "modify" => Body::Modify(mods(rest)?),
        Some((no, ct)) if ct == "modrdn" || ct == "moddn" => moddn(no, rest)?,
        Some((no, ct)) => return err(no, format!("unknown changetype {ct}")),
    };
    Ok(Record {
        line: first.no,
        dn,
        controls,
        body,
    })
}

fn control(no: usize, v: Value) -> Result<LdifControl, LdifError> {
    // control: oid [true|false] [value-spec], the value spec as for an
    // attribute (": text", ":: base64", ":< url").
    let text = match v {
        Value::Data(d) => d,
        Value::Url(_) => return err(no, "bad control line"),
    };
    let (head, value) = match text.iter().position(|&b| b == b':') {
        Some(p) => {
            let l = Line {
                no,
                text: [b"v".as_slice(), &text[p..]].concat(),
            };
            (&text[..p], Some(split(&l)?.1))
        }
        None => (&text[..], None),
    };
    let head = std::str::from_utf8(head).or_else(|_| err(no, "bad control line"))?;
    let mut words = head.split_ascii_whitespace();
    let Some(oid) = words.next() else {
        return err(no, "control: without an OID");
    };
    let critical = match words.next() {
        None | Some("false") => false,
        Some("true") => true,
        Some(o) => return err(no, format!("bad criticality {o}")),
    };
    if words.next().is_some() {
        return err(no, "bad control line");
    }
    Ok(LdifControl {
        oid: oid.to_string(),
        critical,
        value,
    })
}

fn attr_lines(lines: &[Line]) -> Result<Vec<AttrValue>, LdifError> {
    lines
        .iter()
        .map(|l| {
            let (ty, value) = split(l)?;
            if ty == "-" {
                return err(l.no, "'-' outside a modify record");
            }
            Ok(AttrValue { ty, value })
        })
        .collect()
}

fn mods(lines: &[Line]) -> Result<Vec<ModSpec>, LdifError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        let (op, v) = split(l)?;
        let op = match op.to_ascii_lowercase().as_str() {
            "add" => ModOp::Add,
            "delete" => ModOp::Delete,
            "replace" => ModOp::Replace,
            "increment" => ModOp::Increment,
            o => return err(l.no, format!("unknown modify operation {o}")),
        };
        let ty = data_text(l.no, "the attribute", v)?.trim().to_string();
        if ty.is_empty() {
            return err(l.no, "empty attribute type");
        }
        i += 1;
        let mut values = Vec::new();
        while let Some(l) = lines.get(i) {
            i += 1;
            if l.text.trim_ascii_end() == b"-" {
                break;
            }
            let (t, v) = split(l)?;
            if !t.eq_ignore_ascii_case(&ty) {
                return err(l.no, format!("{t} in a modification of {ty}"));
            }
            values.push(v);
        }
        out.push(ModSpec { op, ty, values });
    }
    Ok(out)
}

fn moddn(no: usize, lines: &[Line]) -> Result<Body, LdifError> {
    let mut new_rdn = None;
    let mut delete_old_rdn = None;
    let mut new_superior = None;
    for l in lines {
        let (t, v) = split(l)?;
        match t.to_ascii_lowercase().as_str() {
            "newrdn" if new_rdn.is_none() => new_rdn = Some(data_text(l.no, "newrdn", v)?),
            "deleteoldrdn" if delete_old_rdn.is_none() => {
                delete_old_rdn = Some(parse_bool01(l.no, &data_text(l.no, "deleteoldrdn", v)?)?);
            }
            "newsuperior" if new_superior.is_none() => {
                new_superior = Some(data_text(l.no, "newsuperior", v)?);
            }
            _ => return err(l.no, format!("unexpected {t} in a modrdn record")),
        }
    }
    let Some(new_rdn) = new_rdn else {
        return err(no, "modrdn without newrdn:");
    };
    let Some(delete_old_rdn) = delete_old_rdn else {
        return err(no, "modrdn without deleteoldrdn:");
    };
    Ok(Body::ModDn {
        new_rdn,
        delete_old_rdn,
        new_superior,
    })
}

/// The width lines are folded at.
pub const FOLD: usize = 76;

/// Prints `ldif`: `charset:` and `version:` first if set, each record
/// after a blank line, values base64 unless RFC 2849's SAFE-STRING, lines
/// folded at [`FOLD`] columns.
#[must_use]
pub fn write(ldif: &Ldif) -> String {
    let mut out = String::new();
    if let Some(c) = &ldif.charset {
        line(&mut out, &format!("charset: {c}"));
    }
    if let Some(v) = ldif.version {
        line(&mut out, &format!("version: {v}"));
    }
    for r in &ldif.records {
        if !out.is_empty() {
            out.push('\n');
        }
        write_record(&mut out, r);
    }
    out
}

/// Prints one record (no blank line before or after).
#[must_use]
pub fn write_one(r: &Record) -> String {
    let mut out = String::new();
    write_record(&mut out, r);
    out
}

fn write_record(out: &mut String, r: &Record) {
    attr(out, "dn", &Value::Data(r.dn.as_bytes().to_vec()));
    for c in &r.controls {
        let mut s = format!("control: {}", c.oid);
        if c.critical {
            s.push_str(" true");
        }
        match &c.value {
            None => line(out, &s),
            Some(v) => {
                s.push_str(&spec(v));
                line(out, &s);
            }
        }
    }
    match &r.body {
        Body::Content(a) => {
            for l in a {
                attr(out, &l.ty, &l.value);
            }
        }
        Body::Add(a) => {
            line(out, "changetype: add");
            for l in a {
                attr(out, &l.ty, &l.value);
            }
        }
        Body::Delete => line(out, "changetype: delete"),
        Body::Modify(m) => {
            line(out, "changetype: modify");
            for s in m {
                let op = match s.op {
                    ModOp::Add => "add",
                    ModOp::Delete => "delete",
                    ModOp::Replace => "replace",
                    ModOp::Increment => "increment",
                };
                line(out, &format!("{op}: {}", s.ty));
                for v in &s.values {
                    attr(out, &s.ty, v);
                }
                line(out, "-");
            }
        }
        Body::ModDn {
            new_rdn,
            delete_old_rdn,
            new_superior,
        } => {
            line(out, "changetype: modrdn");
            attr(out, "newrdn", &Value::Data(new_rdn.as_bytes().to_vec()));
            line(out, &format!("deleteoldrdn: {}", u8::from(*delete_old_rdn)));
            if let Some(s) = new_superior {
                attr(out, "newsuperior", &Value::Data(s.as_bytes().to_vec()));
            }
        }
    }
}

/// RFC 2849's SAFE-STRING.
fn safe(v: &[u8]) -> bool {
    match v.first() {
        None => true,
        Some(b' ' | b':' | b'<') => false,
        Some(_) => {
            v.last() != Some(&b' ')
                && v.iter().all(|&b| b != 0 && b != b'\n' && b != b'\r' && b < 0x80)
        }
    }
}

/// The value part of a line after the type: `: text`, `:: base64` or
/// `:< url`.
fn spec(v: &Value) -> String {
    match v {
        Value::Url(u) => format!(":< {u}"),
        Value::Data(d) if d.is_empty() => ":".to_string(),
        Value::Data(d) if safe(d) => format!(": {}", String::from_utf8_lossy(d)),
        Value::Data(d) => format!(":: {}", base64_encode(d)),
    }
}

fn attr(out: &mut String, ty: &str, v: &Value) {
    line(out, &format!("{ty}{}", spec(v)));
}

/// Appends `s` folded at [`FOLD`] columns. Everything printed is ASCII
/// except a type name, so folding never splits a character.
fn line(out: &mut String, s: &str) {
    let mut rest = s;
    let mut width = FOLD;
    loop {
        if rest.len() <= width {
            out.push_str(rest);
            out.push('\n');
            return;
        }
        let mut cut = width;
        while !rest.is_char_boundary(cut) {
            cut -= 1;
        }
        out.push_str(&rest[..cut]);
        out.push_str("\n ");
        rest = &rest[cut..];
        width = FOLD - 1;
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Base64 (RFC 4648) with padding.
#[must_use]
pub fn base64_encode(d: &[u8]) -> String {
    let mut out = String::with_capacity(d.len().div_ceil(3) * 4);
    for c in d.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for k in 0..4 {
            if k <= c.len() {
                out.push(B64[((n >> (18 - 6 * k)) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Decodes base64, padding optional, spaces ignored; none if invalid.
#[must_use]
pub fn base64_decode(s: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    let mut pad = 0;
    for &b in s {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => {
                pad += 1;
                continue;
            }
            b' ' | b'\t' => continue,
            _ => return None,
        };
        if pad > 0 {
            return None;
        }
        acc = ((acc << 6) | u32::from(v)) & 0xFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).unwrap_or(0));
        }
    }
    // Leftover bits must be zero padding; one leftover sextet is invalid.
    if bits >= 6 || pad > 2 || (acc & ((1 << bits) - 1)) != 0 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{Body, Ldif, ModOp, Value, base64_decode, base64_encode, group, parse, write};

    fn ok(src: &str) -> Ldif {
        parse(src.as_bytes()).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn base64() {
        for s in [&b""[..], b"f", b"fo", b"foo", b"foob", b"fooba", b"foobar", &[0, 255, 128]] {
            assert_eq!(base64_decode(base64_encode(s).as_bytes()).unwrap(), s);
        }
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64_decode(b"Zm9vYg"), Some(b"foob".to_vec()));
        assert!(base64_decode(b"Zm9vY").is_none());
        assert!(base64_decode(b"Zm=9").is_none());
        assert!(base64_decode(b"Zm9!").is_none());
    }

    #[test]
    fn rfc2849_content() {
        let src = "version: 1\n\
dn: cn=Barbara Jensen, ou=Product Development, dc=airius, dc=com\n\
objectclass: top\n\
objectclass: person\n\
cn: Barbara Jensen\n\
cn: Barbara J Jensen\n\
description: A big sailing fan.\n\
\n\
# a comment\n\
#  folded\n\
\x20 comment\n\
dn: cn=Bjorn Jensen, ou=Accounting, dc=airius, dc=com\n\
description:Babs is a big sailing fan, and travels extensively in sea\n\
\x20rch of perfect sailing conditions.\n\
title::IFByb2R1Y3QgTWFuYWdlcg==\n\
jpegphoto:< file:///usr/local/directory/photos/fiona.jpg\n\
empty:\n";
        let l = ok(src);
        assert_eq!(l.version, Some(1));
        assert_eq!(l.records.len(), 2);
        let Body::Content(a) = &l.records[1].body else {
            panic!()
        };
        assert_eq!(l.records[1].line, 12);
        assert_eq!(
            a[0].value,
            Value::Data(b"Babs is a big sailing fan, and travels extensively in search of perfect sailing conditions.".to_vec())
        );
        assert_eq!(a[1].value, Value::Data(b" Product Manager".to_vec()));
        assert_eq!(a[2].value, Value::Url("file:///usr/local/directory/photos/fiona.jpg".into()));
        assert_eq!(a[3].value, Value::Data(Vec::new()));
        let g = group(a, |u| Ok::<_, ()>(u.as_bytes().to_vec())).unwrap();
        assert_eq!(g.len(), 4);
        let again = ok(&write(&l));
        assert_eq!(strip_lines(again), strip_lines(l));
    }

    fn strip_lines(mut l: Ldif) -> Ldif {
        for r in &mut l.records {
            r.line = 0;
        }
        l
    }

    #[test]
    fn rfc2849_changes() {
        let src = "version: 1\n\
dn: cn=Fiona Jensen, ou=Marketing, dc=airius, dc=com\n\
changetype: add\n\
objectclass: top\n\
cn: Fiona Jensen\n\
\n\
dn: cn=Robert Jensen, ou=Marketing, dc=airius, dc=com\n\
changetype: delete\n\
\n\
dn: cn=Paul Jensen, ou=Product Development, dc=airius, dc=com\n\
changetype: modrdn\n\
newrdn: cn=Paula Jensen\n\
deleteoldrdn: 1\n\
\n\
dn: ou=PD Accountants, ou=Product Development, dc=airius, dc=com\n\
changetype: moddn\n\
newrdn: ou=Product Development Accountants\n\
deleteoldrdn: 0\n\
newsuperior: ou=Accounting, dc=airius, dc=com\n\
\n\
dn: cn=Paula Jensen, ou=Product Development, dc=airius, dc=com\n\
changetype: modify\n\
add: postaladdress\n\
postaladdress: 123 Anystreet $ Sunnyvale, CA $ 94086\n\
-\n\
delete: description\n\
-\n\
replace: telephonenumber\n\
telephonenumber: +1 408 555 1234\n\
telephonenumber: +1 408 555 5678\n\
-\n\
delete: facsimiletelephonenumber\n\
facsimiletelephonenumber: +1 408 555 9876\n\
-\n\
increment: uidNumber\n\
uidNumber: 1\n\
\n\
dn: ou=Product Development, dc=airius, dc=com\n\
control: 1.2.840.113556.1.4.805 true\n\
control: 1.2.3 false:: AAE=\n\
control: 1.2.4: text\n\
changetype: delete\n";
        let l = ok(src);
        assert_eq!(l.records.len(), 6);
        assert!(matches!(l.records[1].body, Body::Delete));
        let Body::ModDn { delete_old_rdn, new_superior, .. } = &l.records[3].body else {
            panic!()
        };
        assert!(!delete_old_rdn);
        assert_eq!(new_superior.as_deref(), Some("ou=Accounting, dc=airius, dc=com"));
        let Body::Modify(m) = &l.records[4].body else { panic!() };
        assert_eq!(m.len(), 5);
        assert_eq!(m[1].op, ModOp::Delete);
        assert!(m[1].values.is_empty());
        assert_eq!(m[2].values.len(), 2);
        assert_eq!(m[4].op, ModOp::Increment);
        let c = &l.records[5].controls;
        assert_eq!(c.len(), 3);
        assert!(c[0].critical && c[0].value.is_none());
        assert_eq!(c[1].value, Some(Value::Data(vec![0, 1])));
        assert_eq!(c[2].value, Some(Value::Data(b"text".to_vec())));
        assert_eq!(strip_lines(ok(&write(&l))), strip_lines(l));
    }

    #[test]
    fn errors_carry_lines() {
        let cases: [(&str, usize); 10] = [
            ("version: 2\n", 1),
            ("dn: a\nnocolon\n", 2),
            ("cn: a\n", 1),
            ("dn: a\ncn:: !!\n", 2),
            ("dn: a\nchangetype: frob\n", 2),
            ("dn: a\nchangetype: delete\ncn: x\n", 3),
            ("dn: a\nchangetype: modify\nfrob: cn\n", 3),
            ("dn: a\nchangetype: modify\nadd: cn\nsn: x\n", 4),
            ("dn: a\nchangetype: modrdn\nnewrdn: cn=b\n", 2),
            ("\n\ndn: a\ncontrol: 1.2 maybe\nchangetype: delete\n", 4),
        ];
        for (src, line) in cases {
            let e = parse(src.as_bytes()).unwrap_err();
            assert_eq!(e.line, line, "{src:?}: {e}");
        }
    }

    #[test]
    fn writer_folds_and_encodes() {
        let l = Ldif {
            version: Some(1),
            charset: None,
            records: vec![super::Record {
                line: 0,
                dn: "cn=Lučić,dc=x".into(),
                controls: vec![],
                body: Body::Content(vec![
                    super::AttrValue { ty: "description".into(), value: Value::Data(vec![b'x'; 200]) },
                    super::AttrValue { ty: "a".into(), value: Value::Data(b" lead".to_vec()) },
                    super::AttrValue { ty: "b".into(), value: Value::Data(b"x\ny".to_vec()) },
                ]),
            }],
        };
        let s = write(&l);
        assert!(s.lines().all(|x| x.len() <= super::FOLD));
        assert!(s.contains("dn:: "));
        assert!(s.contains("a:: IGxlYWQ="));
        assert_eq!(strip_lines(ok(&s)), l);
    }

    #[test]
    fn ibm_files() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tools/conformance/oracle/dirsrv/");
        let conf = std::fs::read(format!("{root}ibmslapd-shape.ldif")).unwrap();
        let l = parse(&conf).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(l.charset.as_deref(), Some("ISO-8859-1"));
        assert_eq!(l.version, Some(1));
        assert_eq!(l.records.len(), 27);
        assert_eq!(l.records[0].dn, "cn=Configuration");
        assert!(l.records.iter().all(|r| matches!(r.body, Body::Content(_))));
        assert_eq!(strip_lines(ok(&write(&l))), strip_lines(l));

        let inst = std::fs::read(format!("{root}idsinstances.ldif")).unwrap();
        let l = parse(&inst).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(l.charset.as_deref(), Some("ISO-8859-1"));
        assert_eq!(l.records.len(), 2);
        assert_eq!(l.records[1].dn, "cn=QUSRDIR, CN=IDS INSTANCES");
        assert_eq!(l.records[1].line, 10);
    }
}
