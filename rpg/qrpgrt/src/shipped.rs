// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The parsers and generators QS/400 ships in `QSYS` for `DATA-INTO`
//! and `DATA-GEN` (IBM ships none): `QRNPJSON` and `QRNPXML` parse JSON
//! and XML, `QRNGJSON` and `QRNGXML` generate them. They take the same
//! parameter a program written to `QRNDTAINTO` or `QRNDTAGEN` takes.
//!
//! The JSON parser reports the items of the outermost object at the top
//! (as IBM's sample `JSONPARSE` does), other objects as structures,
//! arrays as arrays, strings and numbers as values, `true` and `false`
//! as those words (`1` and `0` with the option `boolean=ind`) and
//! `null` as an empty value. The XML parser reports the root element by
//! name, an element with children or attributes as a structure (its
//! attributes as attributes), any other element as a value, and
//! repeated elements one after another under their name.

use crate::datainto::{Builder, Fail, decode};

/// The document a parser's parameter holds, as text.
fn document(j: &mi::Job, parm: u64) -> String {
    let b = crate::value::bytes(parm, 144);
    let p = mi::FatPtr::from_bytes(&b[0..16].try_into().unwrap_or([0; 16]));
    let len = u32::from_be_bytes([b[64], b[65], b[66], b[67]]) as usize;
    let ccsid = u32::from_be_bytes([b[68], b[69], b[70], b[71]]);
    match j.spaces.deref(p) {
        Ok(a) if a != 0 && len > 0 => decode(ccsid, crate::value::bytes(a, len)),
        _ => String::new(),
    }
}

/// A JSON reader over characters.
struct Json<'a> {
    s: &'a [char],
    i: usize,
    depth: usize,
    ind: bool,
}

impl Json<'_> {
    fn ws(&mut self) {
        while self.s.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
    }

    fn err(&self) -> Fail {
        Fail { status: 357, reason: u32::try_from(self.i).unwrap_or(0) }
    }

    fn string(&mut self) -> Result<String, Fail> {
        // At the opening quote.
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = *self.s.get(self.i).ok_or_else(|| self.err())?;
            self.i += 1;
            match c {
                '"' => return Ok(out),
                '\\' => {
                    let e = *self.s.get(self.i).ok_or_else(|| self.err())?;
                    self.i += 1;
                    match e {
                        'n' => out.push('\n'),
                        't' => out.push('\t'),
                        'r' => out.push('\r'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            let h: String = self.s.get(self.i..self.i + 4).ok_or_else(|| self.err())?.iter().collect();
                            self.i += 4;
                            let v = u32::from_str_radix(&h, 16).map_err(|_| self.err())?;
                            out.push(char::from_u32(v).unwrap_or('\u{FFFD}'));
                        }
                        other => out.push(other),
                    }
                }
                c => out.push(c),
            }
        }
    }

    fn value(&mut self, b: &mut Builder) -> Result<(), Fail> {
        self.ws();
        match self.s.get(self.i).copied() {
            Some('{') => {
                self.i += 1;
                // The outermost object is the document itself.
                let outer = self.depth == 0;
                self.depth += 1;
                if !outer {
                    b.start_struct()?;
                }
                let end = |b: &mut Builder| if outer { Ok(()) } else { b.end_struct() };
                self.ws();
                if self.s.get(self.i) == Some(&'}') {
                    self.i += 1;
                    return end(b);
                }
                loop {
                    self.ws();
                    if self.s.get(self.i) != Some(&'"') {
                        return Err(self.err());
                    }
                    let k = self.string()?;
                    self.ws();
                    if self.s.get(self.i) != Some(&':') {
                        return Err(self.err());
                    }
                    self.i += 1;
                    b.name(&k)?;
                    self.value(b)?;
                    self.ws();
                    match self.s.get(self.i) {
                        Some(',') => self.i += 1,
                        Some('}') => {
                            self.i += 1;
                            return end(b);
                        }
                        _ => return Err(self.err()),
                    }
                }
            }
            Some('[') => {
                self.i += 1;
                self.depth += 1;
                b.start_array()?;
                self.ws();
                if self.s.get(self.i) == Some(&']') {
                    self.i += 1;
                    return b.end_array();
                }
                loop {
                    self.value(b)?;
                    self.ws();
                    match self.s.get(self.i) {
                        Some(',') => self.i += 1,
                        Some(']') => {
                            self.i += 1;
                            return b.end_array();
                        }
                        _ => return Err(self.err()),
                    }
                }
            }
            Some('"') => {
                let v = self.string()?;
                b.value(&v)
            }
            Some(c) if c == '-' || c.is_ascii_digit() => {
                let start = self.i;
                while self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')) {
                    self.i += 1;
                }
                let v: String = self.s[start..self.i].iter().collect();
                b.value(&v)
            }
            Some(_) => {
                let (t, f) = if self.ind { ("1", "0") } else { ("true", "false") };
                for (word, v) in [("true", t), ("false", f), ("null", "")] {
                    let w: Vec<char> = word.chars().collect();
                    if self.s.get(self.i..self.i + w.len()) == Some(&w[..]) {
                        self.i += w.len();
                        return b.value(v);
                    }
                }
                Err(self.err())
            }
            None => Err(self.err()),
        }
    }
}

/// Parse JSON into the builder; `opts` the parser's options.
pub fn parse_json(text: &str, opts: &str, b: &mut Builder) -> Result<(), Fail> {
    let chars: Vec<char> = text.chars().collect();
    let ind = opts.split_whitespace().any(|w| w.eq_ignore_ascii_case("boolean=ind"));
    let mut p = Json { s: &chars, i: 0, depth: 0, ind };
    b.start()?;
    p.value(b)?;
    p.ws();
    if p.i < chars.len() {
        return Err(p.err());
    }
    b.finish()
}

/// Parse XML into the builder.
pub fn parse_xml(text: &str, _opts: &str, b: &mut Builder) -> Result<(), Fail> {
    let root = crate::xml::tree(text).map_err(|e| Fail { status: 357, reason: u32::try_from(e.offset).unwrap_or(0) })?;
    b.start()?;
    b.name(&root.name)?;
    element(&root, b)?;
    b.finish()
}

fn element(e: &crate::xml::Element, b: &mut Builder) -> Result<(), Fail> {
    if e.children.is_empty() && e.attrs.is_empty() {
        return b.value(&e.text);
    }
    b.start_struct()?;
    for (n, v) in &e.attrs {
        b.attr(n, v)?;
    }
    for c in &e.children {
        b.name(&c.name)?;
        element(c, b)?;
    }
    b.end_struct()
}

/// The native programs, by name.
pub fn natives() -> Vec<(&'static str, mi::job::NativeFn)> {
    use std::rc::Rc;
    vec![
        ("QRNPJSON", Rc::new(|j: &mut mi::Job, args: &[u64]| parser(j, args, parse_json)) as mi::job::NativeFn),
        ("QRNPXML", Rc::new(|j: &mut mi::Job, args: &[u64]| parser(j, args, parse_xml))),
        ("QRNGJSON", Rc::new(|j: &mut mi::Job, args: &[u64]| crate::datagen::shipped_generator(j, args, crate::datagen::Style::Json))),
        ("QRNGXML", Rc::new(|j: &mut mi::Job, args: &[u64]| crate::datagen::shipped_generator(j, args, crate::datagen::Style::Xml))),
    ]
}

/// Run a shipped parser over the parameter's document.
fn parser(j: &mut mi::Job, args: &[u64], f: fn(&str, &str, &mut Builder) -> Result<(), Fail>) {
    let Some(&parm) = args.first() else { return };
    let text = document(j, parm);
    let opts = crate::datainto::parser_options(j, parm);
    crate::datainto::with_builder(j, |b| f(&text, &opts, b));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datainto::Node;

    #[test]
    fn json_becomes_the_tree() {
        let mut b = Builder::default();
        parse_json(r#"{"o": {"id": 7, "tags": ["a", "b"], "ok": true, "x": null}}"#, "boolean=ind", &mut b).unwrap();
        let (name, n) = b.root().unwrap();
        assert_eq!(name, "o");
        assert_eq!(
            n,
            Node::Struct(vec![
                ("id".into(), Node::Value("7".into())),
                ("tags".into(), Node::Array(vec![Node::Value("a".into()), Node::Value("b".into())])),
                ("ok".into(), Node::Value("1".into())),
                ("x".into(), Node::Value(String::new())),
            ])
        );
    }

    #[test]
    fn xml_becomes_the_tree() {
        let mut b = Builder::default();
        parse_xml(r#"<cust id="5"><name>Al</name><tag>x</tag><tag>y</tag></cust>"#, "", &mut b).unwrap();
        let (n, node) = b.root().unwrap();
        assert_eq!(n, "cust");
        assert_eq!(
            node,
            Node::Struct(vec![
                ("id".into(), Node::Value("5".into())),
                ("name".into(), Node::Value("Al".into())),
                ("tag".into(), Node::Value("x".into())),
                ("tag".into(), Node::Value("y".into())),
            ])
        );
    }

    #[test]
    fn bad_json_is_an_error() {
        let mut b = Builder::default();
        assert_eq!(parse_json("{\"a\" 1}", "", &mut b).unwrap_err().status, 357);
    }
}
