// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Schema descriptions (RFC 4512 §4.1), read by hand into what the
//! scenarios check: the OID, the names and the keyword fields.

use crate::Error;

/// One description: `( OID NAME ... )`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Desc {
    /// The numeric OID (or the descriptor some servers write in its place).
    pub oid: String,
    /// `NAME`, every one.
    pub names: Vec<String>,
    /// Every other keyword and its values, in order (`SUP`, `MUST`, `SYNTAX`, ...).
    pub fields: Vec<(String, Vec<String>)>,
}

impl Desc {
    /// The values of `key`.
    pub fn get(&self, key: &str) -> Option<&[String]> {
        self.fields
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_slice())
    }

    /// Whether `name` is one of its names (case ignored).
    pub fn named(&self, name: &str) -> bool {
        self.names.iter().any(|n| n.eq_ignore_ascii_case(name))
    }
}

#[derive(Debug, PartialEq)]
enum Tok {
    Open,
    Close,
    Dollar,
    Word(String),
    Quoted(String),
}

fn lex(s: &str) -> Result<Vec<Tok>, Error> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '(' => out.push(Tok::Open),
            ')' => out.push(Tok::Close),
            '$' => out.push(Tok::Dollar),
            '\'' => {
                let mut q = String::new();
                loop {
                    match it.next() {
                        Some('\'') => break,
                        Some(ch) => q.push(ch),
                        None => return Err(Error::Decode("unterminated quoted string".into())),
                    }
                }
                out.push(Tok::Quoted(q));
            }
            c if c.is_whitespace() => {}
            c => {
                let mut w = String::from(c);
                while let Some(&n) = it.peek() {
                    if n.is_whitespace() || matches!(n, '(' | ')' | '$' | '\'') {
                        break;
                    }
                    w.push(n);
                    it.next();
                }
                out.push(Tok::Word(w));
            }
        }
    }
    Ok(out)
}

/// Parses one description.
pub fn parse(s: &str) -> Result<Desc, Error> {
    let toks = lex(s)?;
    let mut it = toks.into_iter().peekable();
    if it.next() != Some(Tok::Open) {
        return Err(Error::Decode("description does not start with (".into()));
    }
    let Some(Tok::Word(oid)) = it.next() else {
        return Err(Error::Decode("no OID".into()));
    };
    let mut d = Desc {
        oid,
        ..Desc::default()
    };
    loop {
        match it.next() {
            Some(Tok::Close) => break,
            Some(Tok::Word(key)) => {
                let mut vals = Vec::new();
                match it.peek() {
                    Some(Tok::Open) => {
                        it.next();
                        loop {
                            match it.next() {
                                Some(Tok::Close) => break,
                                Some(Tok::Dollar) => {}
                                Some(Tok::Word(w) | Tok::Quoted(w)) => vals.push(w),
                                Some(Tok::Open) | None => {
                                    return Err(Error::Decode(format!("bad list after {key}")));
                                }
                            }
                        }
                    }
                    Some(Tok::Word(_) | Tok::Quoted(_)) if !is_flag(&key) => {
                        if let Some(Tok::Word(w) | Tok::Quoted(w)) = it.next() {
                            vals.push(w);
                        }
                    }
                    _ => {}
                }
                if key.eq_ignore_ascii_case("NAME") {
                    d.names = vals;
                } else {
                    d.fields.push((key, vals));
                }
            }
            other => return Err(Error::Decode(format!("unexpected {other:?}"))),
        }
    }
    if it.next().is_some() {
        return Err(Error::Decode("text after the closing )".into()));
    }
    Ok(d)
}

/// Keywords that take no value.
fn is_flag(key: &str) -> bool {
    [
        "OBSOLETE",
        "SINGLE-VALUE",
        "COLLECTIVE",
        "NO-USER-MODIFICATION",
        "ABSTRACT",
        "STRUCTURAL",
        "AUXILIARY",
    ]
    .iter()
    .any(|k| k.eq_ignore_ascii_case(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_type() {
        let d = parse(
            "( 2.5.4.3 NAME ( 'cn' 'commonName' ) DESC 'a (paren) inside' SUP name \
             EQUALITY caseIgnoreMatch SYNTAX 1.3.6.1.4.1.1466.115.121.1.15{256} USAGE userApplications )",
        )
        .unwrap();
        assert_eq!(d.oid, "2.5.4.3");
        assert_eq!(d.names, ["cn", "commonName"]);
        assert_eq!(d.get("DESC").unwrap(), ["a (paren) inside"]);
        assert_eq!(
            d.get("syntax").unwrap(),
            ["1.3.6.1.4.1.1466.115.121.1.15{256}"]
        );
        assert!(d.named("CN"));
    }

    #[test]
    fn object_class_with_flags_and_lists() {
        let d = parse(
            "( 2.5.6.6 NAME 'person' SUP top STRUCTURAL MUST ( sn $ cn ) MAY ( userPassword $ telephoneNumber ) )",
        )
        .unwrap();
        assert_eq!(d.names, ["person"]);
        assert_eq!(d.get("STRUCTURAL").unwrap(), [] as [String; 0]);
        assert_eq!(d.get("MUST").unwrap(), ["sn", "cn"]);
        assert_eq!(d.get("MAY").unwrap(), ["userPassword", "telephoneNumber"]);
    }

    #[test]
    fn single_value_flag_before_a_keyword() {
        let d =
            parse("( 1.2.3 NAME 'x' SINGLE-VALUE NO-USER-MODIFICATION USAGE directoryOperation )")
                .unwrap();
        assert!(d.get("SINGLE-VALUE").unwrap().is_empty());
        assert_eq!(d.get("USAGE").unwrap(), ["directoryOperation"]);
    }

    #[test]
    fn malformed() {
        assert!(parse("2.5.4.3 NAME 'cn' )").is_err());
        assert!(parse("( 2.5.4.3 NAME 'cn )").is_err());
        assert!(parse("( 2.5.4.3 NAME 'cn' ) x").is_err());
    }
}
