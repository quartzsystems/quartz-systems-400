// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The XML parser behind `XML-SAX` and `XML-INTO`: a document read as
//! the events `XML-SAX` hands its handler, in order, and the tree
//! `XML-INTO` matches against its target.

/// What an event reports. The numbers are the values of the
/// `*XML_...` special words the compiler gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `*XML_START_DOCUMENT`.
    StartDocument = 1,
    /// `*XML_VERSION_INFO`.
    VersionInfo = 2,
    /// `*XML_ENCODING_DECL`.
    EncodingDecl = 3,
    /// `*XML_STANDALONE_DECL`.
    StandaloneDecl = 4,
    /// `*XML_DOCTYPE_DECL`.
    DoctypeDecl = 5,
    /// `*XML_START_ELEMENT`.
    StartElement = 6,
    /// `*XML_CHARS`.
    Chars = 7,
    /// `*XML_PREDEF_REF`.
    PredefRef = 8,
    /// `*XML_UCS2_REF`.
    Ucs2Ref = 9,
    /// `*XML_UNKNOWN_REF`.
    UnknownRef = 10,
    /// `*XML_END_ELEMENT`.
    EndElement = 11,
    /// `*XML_ATTR_NAME`.
    AttrName = 12,
    /// `*XML_ATTR_CHARS`.
    AttrChars = 13,
    /// `*XML_ATTR_PREDEF_REF`.
    AttrPredefRef = 14,
    /// `*XML_ATTR_UCS2_REF`.
    AttrUcs2Ref = 15,
    /// `*XML_UNKNOWN_ATTR_REF`.
    UnknownAttrRef = 16,
    /// `*XML_END_ATTR`.
    EndAttr = 17,
    /// `*XML_PI_TARGET`.
    PiTarget = 18,
    /// `*XML_PI_DATA`.
    PiData = 19,
    /// `*XML_START_CDATA`.
    StartCdata = 20,
    /// `*XML_END_CDATA`.
    EndCdata = 21,
    /// `*XML_COMMENT`.
    Comment = 22,
    /// `*XML_EXCEPTION`.
    Exception = 23,
    /// `*XML_END_DOCUMENT`.
    EndDocument = 24,
}

/// The special words and their values, for the compiler.
pub const WORDS: &[(&str, Kind)] = &[
    ("*XML_START_DOCUMENT", Kind::StartDocument),
    ("*XML_VERSION_INFO", Kind::VersionInfo),
    ("*XML_ENCODING_DECL", Kind::EncodingDecl),
    ("*XML_STANDALONE_DECL", Kind::StandaloneDecl),
    ("*XML_DOCTYPE_DECL", Kind::DoctypeDecl),
    ("*XML_START_ELEMENT", Kind::StartElement),
    ("*XML_CHARS", Kind::Chars),
    ("*XML_PREDEF_REF", Kind::PredefRef),
    ("*XML_UCS2_REF", Kind::Ucs2Ref),
    ("*XML_UNKNOWN_REF", Kind::UnknownRef),
    ("*XML_END_ELEMENT", Kind::EndElement),
    ("*XML_ATTR_NAME", Kind::AttrName),
    ("*XML_ATTR_CHARS", Kind::AttrChars),
    ("*XML_ATTR_PREDEF_REF", Kind::AttrPredefRef),
    ("*XML_ATTR_UCS2_REF", Kind::AttrUcs2Ref),
    ("*XML_UNKNOWN_ATTR_REF", Kind::UnknownAttrRef),
    ("*XML_END_ATTR", Kind::EndAttr),
    ("*XML_PI_TARGET", Kind::PiTarget),
    ("*XML_PI_DATA", Kind::PiData),
    ("*XML_START_CDATA", Kind::StartCdata),
    ("*XML_END_CDATA", Kind::EndCdata),
    ("*XML_COMMENT", Kind::Comment),
    ("*XML_EXCEPTION", Kind::Exception),
    ("*XML_END_DOCUMENT", Kind::EndDocument),
];

/// One event: its kind and text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    /// What it reports.
    pub kind: Kind,
    /// The characters it carries (a name, a value, a reference's
    /// character).
    pub text: String,
}

/// A parse error: the parser's code and the offset it was found at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XmlError {
    /// The parser's error code.
    pub code: u32,
    /// Offset in the document, from 0.
    pub offset: usize,
}

fn ev(kind: Kind, text: impl Into<String>) -> Event {
    Event { kind, text: text.into() }
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':')
}

/// The predefined entity a reference names.
fn predef(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "apos" => '\'',
        "quot" => '"',
        _ => return None,
    })
}

/// The events of a document, in order. Parsing stops at the first
/// error, whose code and offset come back with the events before it.
#[must_use]
pub fn events(doc: &str) -> (Vec<Event>, Option<XmlError>) {
    let mut p = Parser { s: doc.chars().collect(), i: 0, out: vec![ev(Kind::StartDocument, "")], open: Vec::new() };
    let r = p.document();
    if r.is_ok() {
        p.out.push(ev(Kind::EndDocument, ""));
    }
    (p.out, r.err())
}

struct Parser {
    s: Vec<char>,
    i: usize,
    out: Vec<Event>,
    open: Vec<String>,
}

impl Parser {
    fn err(&self, code: u32) -> XmlError {
        XmlError { code, offset: self.i }
    }

    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }

    fn starts(&self, t: &str) -> bool {
        t.chars().enumerate().all(|(k, c)| self.s.get(self.i + k) == Some(&c))
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }

    fn name(&mut self) -> Result<String, XmlError> {
        let start = self.i;
        while self.peek().is_some_and(is_name_char) {
            self.i += 1;
        }
        if self.i == start {
            return Err(self.err(1));
        }
        Ok(self.s[start..self.i].iter().collect())
    }

    fn document(&mut self) -> Result<(), XmlError> {
        if self.starts("<?xml") {
            self.decl()?;
        }
        let mut root = false;
        loop {
            self.skip_ws();
            match self.peek() {
                None if root => return Ok(()),
                None => return Err(self.err(2)),
                Some('<') if self.starts("<!--") => self.comment()?,
                Some('<') if self.starts("<?") => self.pi()?,
                Some('<') if !root => {
                    self.element()?;
                    root = true;
                }
                _ => return Err(self.err(3)),
            }
        }
    }

    fn decl(&mut self) -> Result<(), XmlError> {
        self.i += 5;
        loop {
            self.skip_ws();
            if self.starts("?>") {
                self.i += 2;
                return Ok(());
            }
            let n = self.name()?;
            self.skip_ws();
            if self.peek() != Some('=') {
                return Err(self.err(4));
            }
            self.i += 1;
            self.skip_ws();
            let v = self.quoted()?;
            let kind = match n.as_str() {
                "version" => Kind::VersionInfo,
                "encoding" => Kind::EncodingDecl,
                "standalone" => Kind::StandaloneDecl,
                _ => return Err(self.err(4)),
            };
            self.out.push(ev(kind, v));
        }
    }

    fn quoted(&mut self) -> Result<String, XmlError> {
        let q = self.peek().filter(|c| *c == '"' || *c == '\'').ok_or_else(|| self.err(5))?;
        self.i += 1;
        let start = self.i;
        while self.peek().is_some_and(|c| c != q) {
            self.i += 1;
        }
        if self.peek().is_none() {
            return Err(self.err(5));
        }
        let v: String = self.s[start..self.i].iter().collect();
        self.i += 1;
        Ok(v)
    }

    fn comment(&mut self) -> Result<(), XmlError> {
        self.i += 4;
        let start = self.i;
        while !self.starts("-->") {
            if self.peek().is_none() {
                return Err(self.err(6));
            }
            self.i += 1;
        }
        let t: String = self.s[start..self.i].iter().collect();
        self.i += 3;
        self.out.push(ev(Kind::Comment, t));
        Ok(())
    }

    fn pi(&mut self) -> Result<(), XmlError> {
        self.i += 2;
        let target = self.name()?;
        self.out.push(ev(Kind::PiTarget, target));
        self.skip_ws();
        let start = self.i;
        while !self.starts("?>") {
            if self.peek().is_none() {
                return Err(self.err(7));
            }
            self.i += 1;
        }
        let data: String = self.s[start..self.i].iter().collect();
        self.i += 2;
        self.out.push(ev(Kind::PiData, data));
        Ok(())
    }

    /// A reference after its `&`: the event for it.
    fn reference(&mut self, attr: bool) -> Result<Event, XmlError> {
        let start = self.i;
        while self.peek().is_some_and(|c| c != ';') {
            if self.i - start > 32 || self.peek().is_some_and(char::is_whitespace) {
                return Err(self.err(8));
            }
            self.i += 1;
        }
        if self.peek().is_none() {
            return Err(self.err(8));
        }
        let name: String = self.s[start..self.i].iter().collect();
        self.i += 1;
        if let Some(num) = name.strip_prefix('#') {
            let v = match num.strip_prefix('x') {
                Some(h) => u32::from_str_radix(h, 16).ok(),
                None => num.parse().ok(),
            };
            let c = v.and_then(char::from_u32).ok_or_else(|| self.err(8))?;
            return Ok(ev(if attr { Kind::AttrUcs2Ref } else { Kind::Ucs2Ref }, c.to_string()));
        }
        Ok(match predef(&name) {
            Some(c) => ev(if attr { Kind::AttrPredefRef } else { Kind::PredefRef }, c.to_string()),
            None => ev(if attr { Kind::UnknownAttrRef } else { Kind::UnknownRef }, name),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn element(&mut self) -> Result<(), XmlError> {
        self.i += 1;
        let name = self.name()?;
        self.out.push(ev(Kind::StartElement, name.clone()));
        // Attributes.
        loop {
            let had_ws = self.peek().is_some_and(char::is_whitespace);
            self.skip_ws();
            match self.peek() {
                Some('/') if self.starts("/>") => {
                    self.i += 2;
                    self.out.push(ev(Kind::EndElement, name));
                    return Ok(());
                }
                Some('>') => {
                    self.i += 1;
                    break;
                }
                Some(c) if is_name_char(c) && had_ws => {
                    let an = self.name()?;
                    self.skip_ws();
                    if self.peek() != Some('=') {
                        return Err(self.err(9));
                    }
                    self.i += 1;
                    self.skip_ws();
                    let q = self.peek().filter(|c| *c == '"' || *c == '\'').ok_or_else(|| self.err(9))?;
                    self.i += 1;
                    self.out.push(ev(Kind::AttrName, an));
                    let mut run = String::new();
                    loop {
                        match self.peek() {
                            None | Some('<') => return Err(self.err(9)),
                            Some(c) if c == q => {
                                self.i += 1;
                                break;
                            }
                            Some('&') => {
                                self.i += 1;
                                if !run.is_empty() {
                                    self.out.push(ev(Kind::AttrChars, std::mem::take(&mut run)));
                                }
                                let r = self.reference(true)?;
                                self.out.push(r);
                            }
                            Some(c) => {
                                run.push(c);
                                self.i += 1;
                            }
                        }
                    }
                    if !run.is_empty() {
                        self.out.push(ev(Kind::AttrChars, run));
                    }
                    self.out.push(ev(Kind::EndAttr, ""));
                }
                _ => return Err(self.err(10)),
            }
        }
        self.open.push(name.clone());
        // Content.
        let mut run = String::new();
        loop {
            match self.peek() {
                None => return Err(self.err(11)),
                Some('<') => {
                    if !run.is_empty() {
                        self.out.push(ev(Kind::Chars, std::mem::take(&mut run)));
                    }
                    if self.starts("</") {
                        self.i += 2;
                        let n = self.name()?;
                        self.skip_ws();
                        if self.peek() != Some('>') || n != name {
                            return Err(self.err(12));
                        }
                        self.i += 1;
                        self.open.pop();
                        self.out.push(ev(Kind::EndElement, n));
                        return Ok(());
                    } else if self.starts("<!--") {
                        self.comment()?;
                    } else if self.starts("<![CDATA[") {
                        self.i += 9;
                        let start = self.i;
                        while !self.starts("]]>") {
                            if self.peek().is_none() {
                                return Err(self.err(13));
                            }
                            self.i += 1;
                        }
                        let t: String = self.s[start..self.i].iter().collect();
                        self.i += 3;
                        self.out.push(ev(Kind::StartCdata, ""));
                        self.out.push(ev(Kind::Chars, t));
                        self.out.push(ev(Kind::EndCdata, ""));
                    } else if self.starts("<?") {
                        self.pi()?;
                    } else {
                        self.element()?;
                    }
                }
                Some('&') => {
                    if !run.is_empty() {
                        self.out.push(ev(Kind::Chars, std::mem::take(&mut run)));
                    }
                    self.i += 1;
                    let r = self.reference(false)?;
                    self.out.push(r);
                }
                Some(c) => {
                    run.push(c);
                    self.i += 1;
                }
            }
        }
    }
}

/// An element of the tree `XML-INTO` reads: its name, attributes,
/// text and child elements, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Element {
    /// The element's name.
    pub name: String,
    /// Attributes: name and value.
    pub attrs: Vec<(String, String)>,
    /// The character content, references resolved.
    pub text: String,
    /// Child elements.
    pub children: Vec<Element>,
}

/// The document's root element.
pub fn tree(doc: &str) -> Result<Element, XmlError> {
    let (evs, e) = events(doc);
    if let Some(e) = e {
        return Err(e);
    }
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut attr: Option<(String, String)> = None;
    for e in evs {
        match e.kind {
            Kind::StartElement => stack.push(Element { name: e.text, ..Element::default() }),
            Kind::AttrName => attr = Some((e.text, String::new())),
            Kind::AttrChars | Kind::AttrPredefRef | Kind::AttrUcs2Ref => {
                if let Some(a) = attr.as_mut() {
                    a.1.push_str(&e.text);
                }
            }
            Kind::EndAttr => {
                if let (Some(a), Some(top)) = (attr.take(), stack.last_mut()) {
                    top.attrs.push(a);
                }
            }
            Kind::Chars | Kind::PredefRef | Kind::Ucs2Ref => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&e.text);
                }
            }
            Kind::EndElement => {
                let done = stack.pop().unwrap_or_default();
                match stack.last_mut() {
                    Some(parent) => parent.children.push(done),
                    None => root = Some(done),
                }
            }
            _ => {}
        }
    }
    root.ok_or(XmlError { code: 2, offset: doc.len() })
}

/// The options of `%XML`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `path=`: the element names from the root.
    pub path: Vec<String>,
    /// `case=`: `lower` (the default), `upper`, `any`.
    pub case: String,
    /// `allowmissing=yes`.
    pub allow_missing: bool,
    /// `allowextra=yes`.
    pub allow_extra: bool,
    /// `trim=none`.
    pub trim_none: bool,
}

/// Read `%XML`'s options; the first one not valid, as written.
pub fn options(text: &str) -> Result<Options, String> {
    let mut o = Options { case: "lower".into(), ..Options::default() };
    for w in text.split_whitespace() {
        let (k, v) = w.split_once('=').ok_or_else(|| w.to_string())?;
        let lv = v.to_ascii_lowercase();
        match k.to_ascii_lowercase().as_str() {
            "doc" if lv == "string" => {}
            "path" => o.path = v.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect(),
            "case" if matches!(lv.as_str(), "lower" | "upper" | "any") => o.case = lv,
            "allowmissing" if matches!(lv.as_str(), "yes" | "no") => o.allow_missing = lv == "yes",
            "allowextra" if matches!(lv.as_str(), "yes" | "no") => o.allow_extra = lv == "yes",
            "trim" if matches!(lv.as_str(), "all" | "none") => o.trim_none = lv == "none",
            "ccsid" => {}
            _ => return Err(w.to_string()),
        }
    }
    Ok(o)
}

/// Whether an XML name matches an RPG name (upper case) under `case=`.
fn name_matches(xml: &str, rpg: &str, case: &str) -> bool {
    match case {
        "upper" => xml == rpg,
        "any" => xml.eq_ignore_ascii_case(rpg),
        _ => xml == rpg.to_ascii_lowercase(),
    }
}

/// One node of a target's shape (`rpgc::codegen::xml`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    /// The RPG name, upper case.
    pub name: String,
    /// The scalar's descriptor; 0 for a structure.
    pub desc: i64,
    /// Offset in the parent.
    pub off: u64,
    /// Elements; 0 for a scalar.
    pub dim: u32,
    /// Bytes per element.
    pub stride: u64,
    /// A structure's subfields.
    pub children: Vec<Shape>,
}

/// The shape the compiler wrote.
#[must_use]
pub fn shape(text: &str) -> Option<Shape> {
    let recs: Vec<&str> = text.split(';').filter(|r| !r.is_empty()).collect();
    let mut i = 0;
    read_node(&recs, &mut i)
}

fn read_node(recs: &[&str], i: &mut usize) -> Option<Shape> {
    let f: Vec<&str> = recs.get(*i)?.split(',').collect();
    *i += 1;
    let n: usize = f.get(5)?.parse().ok()?;
    let mut s = Shape {
        name: (*f.first()?).to_string(),
        desc: f.get(1)?.parse().ok()?,
        off: f.get(2)?.parse().ok()?,
        dim: f.get(3)?.parse().ok()?,
        stride: f.get(4)?.parse().ok()?,
        children: Vec::new(),
    };
    for _ in 0..n {
        s.children.push(read_node(recs, i)?);
    }
    Some(s)
}

/// Why `XML-INTO` failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntoError {
    /// The document is not well formed.
    Parse,
    /// An option is not valid.
    Option,
    /// The document does not match the target.
    Mismatch,
    /// A value does not convert to its field's type.
    Data,
}

/// Whitespace under `trim=all`: the ends dropped and each run inside
/// made one blank.
fn trim_all(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Text into fixed character storage, blank padded.
fn store_chars(addr: u64, size: usize, text: &str) {
    let b = crate::value::bytes(addr, size);
    b.fill(0x40);
    let e = crate::value::ebcdic(text);
    let n = e.len().min(b.len());
    b[..n].copy_from_slice(&e[..n]);
}

/// Write one scalar value at an address.
fn store_scalar(addr: u64, d: &crate::value::Desc, text: &str) -> Result<(), IntoError> {
    use crate::value::Kind as K;
    match d.kind {
        K::VarChar => {
            let mut e = crate::value::ebcdic(text);
            e.truncate(usize::try_from(d.len).unwrap_or(0));
            crate::value::write_varchar(addr, &e);
        }
        K::Ind => {
            crate::value::bytes(addr, 1)[0] = match text.trim() {
                "1" => 0xF1,
                "0" | "" => 0xF0,
                _ => return Err(IntoError::Data),
            };
        }
        K::Float => {
            let v = crate::cvt::parse(text, '.').ok_or(IntoError::Data)?;
            crate::value::write_num(addr, d, &v).map_err(|_| IntoError::Data)?;
        }
        _ if d.numeric() => {
            let v = crate::cvt::parse(text, '.').ok_or(IntoError::Data)?;
            if !crate::cvt::write(addr, d, &v, qdecimal::Rounding::Truncate) {
                return Err(IntoError::Data);
            }
        }
        // Character data, and dates and times as their characters.
        _ => store_chars(addr, d.size(), text),
    }
    Ok(())
}

/// Fill one element of a shape at `addr` from an element.
fn fill(addr: u64, s: &Shape, el: &Element, o: &Options) -> Result<(), IntoError> {
    if s.children.is_empty() {
        let d = crate::value::Desc::of(s.desc);
        let text = if o.trim_none { el.text.clone() } else { trim_all(&el.text) };
        return store_scalar(addr, &d, &text);
    }
    // Every child element and attribute is a subfield.
    if !o.allow_extra {
        let known = |n: &str| s.children.iter().any(|c| name_matches(n, &c.name, &o.case));
        if el.children.iter().any(|c| !known(&c.name)) || el.attrs.iter().any(|(n, _)| !known(n)) {
            return Err(IntoError::Mismatch);
        }
    }
    for c in &s.children {
        let base = addr + c.off;
        let kids: Vec<&Element> = el.children.iter().filter(|k| name_matches(&k.name, &c.name, &o.case)).collect();
        if c.dim > 0 {
            if kids.len() > c.dim as usize || (kids.len() < c.dim as usize && !o.allow_missing) {
                return Err(IntoError::Mismatch);
            }
            for (k, e) in kids.iter().enumerate() {
                fill(base + c.stride * k as u64, c, e, o)?;
            }
            continue;
        }
        if let Some(e) = kids.first() {
            fill(base, c, e, o)?;
        } else if let Some((_, v)) = el.attrs.iter().find(|(n, _)| name_matches(n, &c.name, &o.case)) {
            let e = Element { text: v.clone(), ..Element::default() };
            fill(base, c, &e, o)?;
        } else if !o.allow_missing {
            return Err(IntoError::Mismatch);
        }
    }
    Ok(())
}

/// The elements a target takes: the root, or the ones `path` names.
fn selected<'a>(root: &'a Element, s: &Shape, o: &Options) -> Result<Vec<&'a Element>, IntoError> {
    if o.path.is_empty() {
        if !name_matches(&root.name, &s.name, &o.case) {
            return Err(IntoError::Mismatch);
        }
        return Ok(vec![root]);
    }
    let eq = |x: &str, p: &str| if o.case == "any" { x.eq_ignore_ascii_case(p) } else { x == p };
    if !eq(&root.name, &o.path[0]) {
        return Err(IntoError::Mismatch);
    }
    let mut level = vec![root];
    for p in &o.path[1..] {
        level = level.iter().flat_map(|e| e.children.iter().filter(|c| eq(&c.name, p))).collect();
    }
    Ok(level)
}

/// Fill a target from a document: the number of elements set.
pub fn into(addr: u64, s: &Shape, doc: &str, opts: &str) -> Result<i64, IntoError> {
    let o = options(opts).map_err(|_| IntoError::Option)?;
    let root = tree(doc).map_err(|_| IntoError::Parse)?;
    let els = selected(&root, s, &o)?;
    if s.dim > 0 {
        if els.len() > s.dim as usize {
            return Err(IntoError::Mismatch);
        }
        for (k, e) in els.iter().enumerate() {
            fill(addr + s.stride * k as u64, s, e, &o)?;
        }
        return Ok(i64::try_from(els.len()).unwrap_or(0));
    }
    let e = els.first().ok_or(IntoError::Mismatch)?;
    fill(addr, s, e, &o)?;
    Ok(1)
}

fn job(env: *mut codegen::abi::Env) -> &'static mut mi::Job {
    mi::storage::job_of(env)
}

/// A character operand's text: its descriptor says fixed or varying.
fn text_of(addr: i64, desc: i64) -> String {
    let d = crate::value::Desc::of(desc);
    let raw = crate::value::bytes(addr.cast_unsigned(), d.size());
    match d.kind {
        crate::value::Kind::VarChar => {
            let n = usize::from(u16::from_be_bytes([raw[0], raw[1]]));
            ccsid::from_ebcdic(&raw[2..2 + n.min(raw.len() - 2)])
        }
        _ => ccsid::from_ebcdic(raw),
    }
}

/// A `const varchar` parameter's text.
fn varchar_at(addr: i64) -> String {
    let len = crate::value::bytes(addr.cast_unsigned(), 2);
    let n = usize::from(u16::from_be_bytes([len[0], len[1]]));
    ccsid::from_ebcdic(&crate::value::bytes(addr.cast_unsigned(), 2 + n)[2..])
}

/// Send the runtime's escape for an RPG status.
fn escape(j: &mut mi::Job, status: u16) {
    let id = format!("RNX{status:04}");
    let _ = j.send_message(&id, "QRNXMSG", "*LIBL", Vec::new(), qir::MsgType::Escape, mi::QueueRef::Same);
}

/// The RPG status of a failed `XML-INTO`.
fn status(e: IntoError) -> u16 {
    match e {
        IntoError::Parse => 351,
        IntoError::Option => 352,
        IntoError::Mismatch | IntoError::Data => 353,
    }
}

/// The code address a procedure pointer in storage holds, if any.
fn proc_at(addr: i64) -> Option<usize> {
    let b: [u8; 16] = crate::value::bytes(addr.cast_unsigned(), 16).try_into().ok()?;
    let p = mi::FatPtr::from_bytes(&b);
    if p.kind() == Some(mi::PtrKind::Procedure) { usize::try_from(p.offset).ok() } else { None }
}

/// `XML-INTO %HANDLER`: the elements in groups of the handler's array
/// size; the number passed.
fn into_handler(j: &mut mi::Job, addr: usize, comm: i64, s: &Shape, doc: &str, opts: &str) -> Result<i64, IntoError> {
    let o = options(opts).map_err(|_| IntoError::Option)?;
    let root = tree(doc).map_err(|_| IntoError::Parse)?;
    let one = Shape { dim: 0, ..s.clone() };
    let named = Shape { name: o.path.last().map(|p| p.to_ascii_uppercase()).unwrap_or_default(), ..one.clone() };
    let els = selected(&root, &named, &Options { case: "any".into(), ..o.clone() })?;
    let group = usize::try_from(s.dim.max(1)).unwrap_or(1);
    let mut buf = vec![0u8; usize::try_from(s.stride).unwrap_or(0) * group];
    let mut total = 0i64;
    for chunk in els.chunks(group) {
        buf.fill(0);
        for (k, e) in chunk.iter().enumerate() {
            fill(buf.as_mut_ptr() as u64 + s.stride * k as u64, &one, e, &o)?;
        }
        let n = i64::try_from(chunk.len()).unwrap_or(0);
        total += n;
        let mark = j.raised.len();
        let rc = mi::act::call_into_handler(j, addr, comm.cast_unsigned(), buf.as_mut_ptr() as u64, n);
        if j.pending() {
            handler_failed(j, mark);
            break;
        }
        if rc != 0 {
            break;
        }
    }
    Ok(total)
}

/// A handler ended in error: the operation fails with the error that
/// ended it (its status, not the function check that followed).
fn handler_failed(j: &mut mi::Job, mark: usize) {
    let Some(first) = j.raised.get(mark).cloned() else { return };
    j.clear();
    let _ = j.send_message(&first.msgid, &first.msgf, "*LIBL", first.data, qir::MsgType::Escape, mi::QueueRef::Same);
}

/// `_QRNX_XMLINTO(target, shape, doc, dd, opts, od, handler, comm)`:
/// the target filled, or the handler called; the number of elements.
pub extern "sysv64" fn xml_into(
    env: *mut codegen::abi::Env,
    _argc: i64,
    _od: i64,
    target: i64,
    shp: i64,
    doc: i64,
    dd: i64,
    opts: i64,
    od: i64,
    h: i64,
    comm: i64,
) -> i64 {
    let j = job(env);
    let Some(s) = shape(&varchar_at(shp)) else { return 0 };
    let (doc, opts) = (text_of(doc, dd), text_of(opts, od));
    let r = match proc_at(h) {
        Some(addr) => into_handler(j, addr, comm, &s, &doc, &opts),
        None => into(target.cast_unsigned(), &s, &doc, &opts),
    };
    r.unwrap_or_else(|e| {
        escape(j, status(e));
        0
    })
}

/// `_QRNX_XMLSAX(handler, comm, doc, dd, opts, od)`: every event of the
/// document to the handler, until it returns nonzero.
pub extern "sysv64" fn xml_sax(env: *mut codegen::abi::Env, _argc: i64, _od: i64, h: i64, comm: i64, doc: i64, dd: i64, opts: i64, od: i64) {
    let j = job(env);
    let Some(addr) = proc_at(h) else { return };
    let text = text_of(doc, dd);
    if options(&text_of(opts, od)).is_err() {
        escape(j, 352);
        return;
    }
    let (evs, err) = events(&text);
    // Events without characters have length -1 (pub400).
    let mut list: Vec<(i32, Option<String>, i32)> = evs
        .into_iter()
        .map(|e| {
            let with_text = !matches!(e.kind, Kind::StartDocument | Kind::EndDocument | Kind::EndAttr | Kind::StartCdata | Kind::EndCdata);
            (e.kind as i32, with_text.then_some(e.text), 0)
        })
        .collect();
    // The exception's pointer is not set; its length is the offset.
    let mut exc_len = 0;
    if let Some(e) = err {
        exc_len = i64::try_from(e.offset).unwrap_or(0) + 1;
        list.push((Kind::Exception as i32, None, i32::try_from(e.code).unwrap_or(1)));
    }
    for (event, s, exc) in list {
        let mut data = crate::value::ebcdic(s.as_deref().unwrap_or(""));
        let len = match &s {
            Some(_) => i64::try_from(data.len()).unwrap_or(0),
            None if event == Kind::Exception as i32 => exc_len,
            None => -1,
        };
        let (ptr, base) = if data.is_empty() {
            (0u128, 0)
        } else {
            let base = data.as_mut_ptr() as u64;
            let p = j.spaces.add_region(base, data.len() as u64);
            (u128::from_le_bytes(p.to_bytes()), base)
        };
        let mark = j.raised.len();
        let rc = mi::act::call_sax_handler(j, addr, comm.cast_unsigned(), event, ptr, len, exc);
        if base != 0 {
            j.spaces.kill_at(base);
        }
        if j.pending() {
            handler_failed(j, mark);
            return;
        }
        // A handler that returns nonzero ends the parse as an error.
        if rc != 0 {
            escape(j, 351);
            return;
        }
    }
    if err.is_some() {
        escape(j, 351);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(doc: &str) -> Vec<(Kind, String)> {
        events(doc).0.into_iter().map(|e| (e.kind, e.text)).collect()
    }

    #[test]
    fn reads_a_small_document() {
        let k = kinds("<?xml version=\"1.0\"?><a x=\"1\"><b>hi</b><c/></a>");
        assert_eq!(k[1], (Kind::VersionInfo, "1.0".into()));
        assert_eq!(k[2], (Kind::StartElement, "a".into()));
        assert_eq!(k[3], (Kind::AttrName, "x".into()));
        assert_eq!(k[4], (Kind::AttrChars, "1".into()));
        assert_eq!(k.last().map(|x| x.0), Some(Kind::EndDocument));
    }

    #[test]
    fn errors_stop_the_parse() {
        let (evs, e) = events("<a><b></a>");
        assert!(e.is_some());
        assert!(evs.iter().all(|x| x.kind != Kind::EndDocument));
    }

    #[test]
    fn builds_the_tree() {
        let t = tree("<l><i id=\"3\">a&amp;b</i><i/></l>").expect("parses");
        assert_eq!(t.children.len(), 2);
        assert_eq!(t.children[0].text, "a&b");
        assert_eq!(t.children[0].attrs, vec![("id".into(), "3".into())]);
    }
}
