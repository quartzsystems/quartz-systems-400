// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `XMLPARSE(DOCUMENT text [STRIP | PRESERVE WHITESPACE])` and
//! `XMLSERIALIZE(xml AS type [INCLUDING | EXCLUDING XMLDECLARATION])`,
//! as the primary does them (`tools/conformance/sql/cases/types/XML1.sql`):
//! a well-formed document is kept in its serialized form (attributes in
//! double quotes, one blank apart; character and entity references
//! resolved and written back as `&amp;`, `&lt;`, `&gt;`, `&quot;`; CDATA
//! as escaped text; the declaration dropped; blank text between
//! elements dropped unless `PRESERVE WHITESPACE`). An XML value is that
//! text. The `XML` column type and the other XML functions are refused
//! (`SQL0084`).

use std::fmt::Write as _;

use crate::error::{Result, SqlError};
use crate::value::Value;

/// `SQ20398`: text that is no well-formed XML document.
fn parse_failed(why: &str) -> SqlError {
    SqlError::Coded { code: -20398, state: "2200M", what: format!("XML parsing failed: {why}") }
}

struct P<'a> {
    s: &'a [char],
    at: usize,
    strip: bool,
    out: String,
}

fn name_char(c: char, first: bool) -> bool {
    c.is_alphabetic() || c == '_' || c == ':' || (!first && (c.is_ascii_digit() || c == '-' || c == '.'))
}

impl P<'_> {
    fn starts(&self, w: &str) -> bool {
        (self.at..).zip(w.chars()).all(|(i, c)| self.s.get(i) == Some(&c))
    }

    fn eat(&mut self, w: &str) -> bool {
        if self.starts(w) {
            self.at += w.chars().count();
            true
        } else {
            false
        }
    }

    fn ws(&mut self) -> bool {
        let start = self.at;
        while self.s.get(self.at).is_some_and(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
            self.at += 1;
        }
        self.at > start
    }

    fn name(&mut self) -> Result<String> {
        let start = self.at;
        while self.s.get(self.at).is_some_and(|c| name_char(*c, self.at == start)) {
            self.at += 1;
        }
        if self.at == start {
            return Err(parse_failed("a name is expected"));
        }
        Ok(self.s[start..self.at].iter().collect())
    }

    fn until(&mut self, end: &str) -> Result<String> {
        let start = self.at;
        while !self.starts(end) {
            if self.at >= self.s.len() {
                return Err(parse_failed(&format!("{end} is missing")));
            }
            self.at += 1;
        }
        let t = self.s[start..self.at].iter().collect();
        self.at += end.chars().count();
        Ok(t)
    }

    /// A reference after `&`.
    fn reference(&mut self) -> Result<char> {
        let body = self.until(";")?;
        let c = match body.as_str() {
            "lt" => '<',
            "gt" => '>',
            "amp" => '&',
            "quot" => '"',
            "apos" => '\'',
            b => {
                let n = if let Some(h) = b.strip_prefix("#x") {
                    u32::from_str_radix(h, 16).ok()
                } else {
                    b.strip_prefix('#').and_then(|d| d.parse().ok())
                };
                return n.and_then(char::from_u32).ok_or_else(|| parse_failed(&format!("&{b}; is not a known reference")));
            }
        };
        Ok(c)
    }

    fn misc(&mut self) -> Result<bool> {
        if self.eat("<!--") {
            let c = self.until("-->")?;
            let _ = write!(self.out, "<!--{c}-->");
            return Ok(true);
        }
        if self.starts("<?") && !self.starts("<?xml ") {
            self.at += 2;
            let target = self.name()?;
            let rest = self.until("?>")?;
            let rest = rest.trim_start();
            let _ = write!(self.out, "<?{target}{}{rest}?>", if rest.is_empty() { "" } else { " " });
            return Ok(true);
        }
        Ok(false)
    }

    fn element(&mut self) -> Result<()> {
        if !self.eat("<") {
            return Err(parse_failed("an element is expected"));
        }
        let name = self.name()?;
        self.out.push('<');
        self.out.push_str(&name);
        loop {
            let blank = self.ws();
            if self.eat("/>") {
                self.out.push_str("/>");
                return Ok(());
            }
            if self.eat(">") {
                self.out.push('>');
                break;
            }
            if !blank {
                return Err(parse_failed("a blank is expected before an attribute"));
            }
            let attr = self.name()?;
            self.ws();
            if !self.eat("=") {
                return Err(parse_failed("= is expected after an attribute name"));
            }
            self.ws();
            let q = match self.s.get(self.at) {
                Some('"') => "\"",
                Some('\'') => "'",
                _ => return Err(parse_failed("an attribute value is expected")),
            };
            self.at += 1;
            let mut value = String::new();
            loop {
                match self.s.get(self.at) {
                    None | Some('<') => return Err(parse_failed("an attribute value is not closed")),
                    Some(c) if c.to_string() == q => {
                        self.at += 1;
                        break;
                    }
                    Some('&') => {
                        self.at += 1;
                        value.push(self.reference()?);
                    }
                    Some(c) => {
                        value.push(*c);
                        self.at += 1;
                    }
                }
            }
            let _ = write!(self.out, " {attr}=\"");
            for c in value.chars() {
                match c {
                    '&' => self.out.push_str("&amp;"),
                    '<' => self.out.push_str("&lt;"),
                    '"' => self.out.push_str("&quot;"),
                    c => self.out.push(c),
                }
            }
            self.out.push('"');
        }
        // Content.
        let mut text = String::new();
        loop {
            if self.at >= self.s.len() {
                return Err(parse_failed(&format!("element {name} is not closed")));
            }
            if self.starts("</") {
                self.flush(&mut text);
                self.at += 2;
                let end = self.name()?;
                self.ws();
                if end != name || !self.eat(">") {
                    return Err(parse_failed(&format!("</{end}> does not close <{name}>")));
                }
                let _ = write!(self.out, "</{name}>");
                return Ok(());
            }
            if self.eat("<![CDATA[") {
                text.push_str(&self.until("]]>")?);
                continue;
            }
            if self.starts("<!--") || self.starts("<?") {
                self.flush(&mut text);
                self.misc()?;
                continue;
            }
            if self.starts("<") {
                self.flush(&mut text);
                self.element()?;
                continue;
            }
            let c = self.s[self.at];
            self.at += 1;
            if c == '&' {
                text.push(self.reference()?);
            } else {
                text.push(c);
            }
        }
    }

    /// Text before markup: dropped when only blanks and stripping, else
    /// escaped.
    fn flush(&mut self, text: &mut String) {
        if self.strip && text.chars().all(|c| matches!(c, ' ' | '\t' | '\n' | '\r')) {
            text.clear();
            return;
        }
        for c in text.chars() {
            match c {
                '&' => self.out.push_str("&amp;"),
                '<' => self.out.push_str("&lt;"),
                '>' => self.out.push_str("&gt;"),
                c => self.out.push(c),
            }
        }
        text.clear();
    }

    fn document(&mut self) -> Result<()> {
        self.ws();
        if self.eat("<?xml") {
            self.until("?>")?;
        }
        loop {
            self.ws();
            if !self.misc()? {
                break;
            }
        }
        self.element()?;
        loop {
            self.ws();
            if !self.misc()? {
                break;
            }
        }
        if self.at != self.s.len() {
            return Err(parse_failed("text follows the document element"));
        }
        Ok(())
    }
}

/// `XMLPARSE(DOCUMENT text)`: the document's serialized form.
pub fn parse(text: &str, preserve: bool) -> Result<String> {
    let s: Vec<char> = text.chars().collect();
    let mut p = P { s: &s, at: 0, strip: !preserve, out: String::new() };
    p.document()?;
    Ok(p.out)
}

/// `XMLSERIALIZE`: the text, with the declaration when asked for (the
/// job's CCSID 37 is `IBM037`).
#[must_use]
pub fn serialize(xml: &str, declaration: bool) -> String {
    if declaration { format!("<?xml version=\"1.0\" encoding=\"IBM037\"?>{xml}") } else { xml.to_string() }
}

/// Evaluated operands of `XMLPARSE` (`[text]`, opts `w` = preserve).
pub(crate) fn call_parse(v: &Value, preserve: bool) -> Result<Value> {
    match v {
        Value::Null => Ok(Value::Null),
        Value::Str(s) => parse(s, preserve).map(Value::Str),
        Value::Bin(b) => parse(&ccsid::from_ebcdic(b), preserve).map(Value::Str),
        v => Err(SqlError::Coded {
            code: -171,
            state: "42815",
            what: format!("argument 1 of XMLPARSE is {}, not a string", v.type_name()),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn as_the_primary_serializes() {
        assert_eq!(parse("<a><b>x</b></a>", false).unwrap(), "<a><b>x</b></a>");
        assert_eq!(
            parse("<a>  <b x=\"1\"  y='2'>x &amp; y</b>  </a>", false).unwrap(),
            "<a><b x=\"1\" y=\"2\">x &amp; y</b></a>"
        );
        assert_eq!(parse("<a>  <b>x</b>  </a>", true).unwrap(), "<a>  <b>x</b>  </a>");
        assert_eq!(parse("<?xml version=\"1.0\"?><a/>", false).unwrap(), "<a/>");
        assert_eq!(
            parse("<a><!-- c --><?pi x?><![CDATA[<z>]]>&lt;&#65;</a>", false).unwrap(),
            "<a><!-- c --><?pi x?>&lt;z&gt;&lt;A</a>"
        );
        assert_eq!(parse("<a b=\"&quot;\"/>", false).unwrap(), "<a b=\"&quot;\"/>");
        assert_eq!(parse("<a>&#xE9;é</a>", false).unwrap(), "<a>éé</a>");
        assert_eq!(serialize("<a/>", true), "<?xml version=\"1.0\" encoding=\"IBM037\"?><a/>");
        for bad in ["<a>x</b>", "text", "<a", "<a b=1/>", "<a/><b/>", "<a>&nope;</a>"] {
            assert_eq!(parse(bad, false).unwrap_err().sqlcode(), -20398, "{bad}");
        }
    }
}
