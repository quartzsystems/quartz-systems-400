// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The SQL/JSON path language as IBM i takes it: `lax` (the default)
//! or `strict`, `$`, member accessors (`.name`, `."a b"`, `.*`) and
//! array accessors (`[0]`, `[last]`, `[last - 1]`, `[0 to 2]`, `[1, 3]`,
//! `[*]`). In lax mode a member accessor looks into every element of an
//! array and an array accessor treats a lone value as an array of one;
//! a missing member or index gives nothing. In strict mode both are
//! errors. Filters (`?(...)`), item methods (`.type()`), `@`, variables
//! and `**` are not implemented.

use super::value::Json;

/// Why a path cannot be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathError {
    /// Text that is not a path.
    Syntax(String),
    /// A path feature this release does not implement.
    Unsupported(String),
    /// Strict mode: a member, index or kind the item does not have.
    Structure(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Index {
    At(i64),
    /// `last - n`.
    Last(i64),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Step {
    Member(String),
    AnyMember,
    Elements(Vec<(Index, Option<Index>)>),
    AnyElement,
}

/// A parsed path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Path {
    /// `strict`.
    pub strict: bool,
    steps: Vec<Step>,
}

struct Lex<'a> {
    s: &'a [char],
    at: usize,
}

impl Lex<'_> {
    fn ws(&mut self) {
        while self.s.get(self.at).is_some_and(|c| c.is_whitespace()) {
            self.at += 1;
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.ws();
        self.s.get(self.at).copied()
    }

    fn word(&mut self) -> String {
        self.ws();
        let start = self.at;
        while self
            .s
            .get(self.at)
            .is_some_and(|c| c.is_alphanumeric() || *c == '_')
        {
            self.at += 1;
        }
        self.s[start..self.at].iter().collect()
    }

    fn int(&mut self) -> Option<i64> {
        self.ws();
        let start = self.at;
        while self.s.get(self.at).is_some_and(char::is_ascii_digit) {
            self.at += 1;
        }
        self.s[start..self.at]
            .iter()
            .collect::<String>()
            .parse()
            .ok()
    }
}

fn syntax(text: &str) -> PathError {
    PathError::Syntax(format!("'{text}' is not an SQL/JSON path"))
}

fn index(l: &mut Lex<'_>, text: &str) -> Result<Index, PathError> {
    if l.peek().is_some_and(char::is_alphabetic) {
        let save = l.at;
        if l.word() != "last" {
            l.at = save;
            return Err(syntax(text));
        }
        return Ok(match l.peek() {
            Some('-') => {
                l.at += 1;
                Index::Last(l.int().ok_or_else(|| syntax(text))?)
            }
            Some('+') => {
                l.at += 1;
                Index::Last(-l.int().ok_or_else(|| syntax(text))?)
            }
            _ => Index::Last(0),
        });
    }
    l.int().map(Index::At).ok_or_else(|| syntax(text))
}

/// Parse a path.
pub fn parse(text: &str) -> Result<Path, PathError> {
    let chars: Vec<char> = text.chars().collect();
    let mut l = Lex { s: &chars, at: 0 };
    let mut strict = false;
    if l.peek().is_some_and(char::is_alphabetic) {
        match l.word().as_str() {
            "strict" => strict = true,
            "lax" => {}
            _ => return Err(syntax(text)),
        }
    }
    if l.peek() != Some('$') {
        return Err(syntax(text));
    }
    l.at += 1;
    let mut steps = Vec::new();
    loop {
        match l.peek() {
            None => break,
            Some('.') => {
                l.at += 1;
                match l.peek() {
                    Some('*') if l.s.get(l.at + 1) == Some(&'*') => {
                        return Err(PathError::Unsupported("the ** accessor".into()));
                    }
                    Some('*') => {
                        l.at += 1;
                        steps.push(Step::AnyMember);
                    }
                    Some('"') => {
                        l.at += 1;
                        let start = l.at;
                        while l.s.get(l.at).is_some_and(|c| *c != '"') {
                            l.at += 1;
                        }
                        if l.at >= l.s.len() {
                            return Err(syntax(text));
                        }
                        steps.push(Step::Member(l.s[start..l.at].iter().collect()));
                        l.at += 1;
                    }
                    _ => {
                        let w = l.word();
                        if w.is_empty() {
                            return Err(syntax(text));
                        }
                        if l.peek() == Some('(') {
                            return Err(PathError::Unsupported(format!("the item method {w}()")));
                        }
                        steps.push(Step::Member(w));
                    }
                }
            }
            Some('[') => {
                l.at += 1;
                if l.peek() == Some('*') {
                    l.at += 1;
                    if l.peek() != Some(']') {
                        return Err(syntax(text));
                    }
                    l.at += 1;
                    steps.push(Step::AnyElement);
                    continue;
                }
                let mut subs = Vec::new();
                loop {
                    let from = index(&mut l, text)?;
                    let save = l.at;
                    let to = if l.word() == "to" {
                        Some(index(&mut l, text)?)
                    } else {
                        l.at = save;
                        None
                    };
                    subs.push((from, to));
                    match l.peek() {
                        Some(',') => l.at += 1,
                        Some(']') => {
                            l.at += 1;
                            break;
                        }
                        _ => return Err(syntax(text)),
                    }
                }
                steps.push(Step::Elements(subs));
            }
            Some('?') => return Err(PathError::Unsupported("filter expressions".into())),
            Some(_) => return Err(syntax(text)),
        }
    }
    Ok(Path { strict, steps })
}

fn resolve(i: &Index, len: usize) -> i64 {
    let last = i64::try_from(len).unwrap_or(i64::MAX) - 1;
    match i {
        Index::At(n) => *n,
        Index::Last(k) => last - k,
    }
}

impl Path {
    /// The items `doc` gives.
    pub fn eval<'a>(&self, doc: &'a Json) -> Result<Vec<&'a Json>, PathError> {
        let mut items = vec![doc];
        for step in &self.steps {
            let mut next = Vec::new();
            for item in items {
                self.step(step, item, &mut next)?;
            }
            items = next;
        }
        Ok(items)
    }

    fn step<'a>(
        &self,
        step: &Step,
        item: &'a Json,
        out: &mut Vec<&'a Json>,
    ) -> Result<(), PathError> {
        match (step, item) {
            (Step::Member(k), Json::Object(m)) => {
                let before = out.len();
                out.extend(m.iter().filter(|(n, _)| n == k).map(|(_, v)| v).take(1));
                if self.strict && out.len() == before {
                    return Err(PathError::Structure(format!(
                        "the object has no member {k}"
                    )));
                }
            }
            (Step::AnyMember, Json::Object(m)) => out.extend(m.iter().map(|(_, v)| v)),
            (Step::Member(_) | Step::AnyMember, Json::Array(items)) if !self.strict => {
                for i in items {
                    self.step(step, i, out)?;
                }
            }
            (Step::Member(k), _) => {
                if self.strict {
                    return Err(PathError::Structure(format!(
                        "member {k} of a value that is not an object"
                    )));
                }
            }
            (Step::AnyMember, _) => {
                if self.strict {
                    return Err(PathError::Structure(
                        "members of a value that is not an object".into(),
                    ));
                }
            }
            (Step::AnyElement, Json::Array(items)) => out.extend(items.iter()),
            (Step::Elements(subs), Json::Array(items)) => {
                let len = items.len();
                for (from, to) in subs {
                    let a = resolve(from, len);
                    let b = to.as_ref().map_or(a, |t| resolve(t, len));
                    for i in a..=b {
                        match usize::try_from(i).ok().and_then(|i| items.get(i)) {
                            Some(v) => out.push(v),
                            None if self.strict => {
                                return Err(PathError::Structure(format!(
                                    "index {i} is past the array"
                                )));
                            }
                            None => {}
                        }
                    }
                }
            }
            // Lax: a lone value is an array of one.
            (Step::AnyElement, v) if !self.strict => out.push(v),
            (Step::Elements(subs), v) if !self.strict => {
                if subs.iter().any(|(f, t)| {
                    let a = resolve(f, 1);
                    let b = t.as_ref().map_or(a, |t| resolve(t, 1));
                    a <= 0 && b >= 0
                }) {
                    out.push(v);
                }
            }
            (Step::AnyElement | Step::Elements(_), _) => {
                return Err(PathError::Structure(
                    "an array accessor on a value that is not an array".into(),
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::value::parse as json;
    use super::*;

    fn texts(path: &str, doc: &str) -> Result<Vec<String>, PathError> {
        let d = json(doc).unwrap();
        Ok(parse(path)?.eval(&d)?.iter().map(|j| j.text()).collect())
    }

    #[test]
    fn the_accessors() {
        let doc = r#"{"a":{"b":[10,20,30]},"x":[{"b":1},{"b":2}],"a b":3}"#;
        assert_eq!(texts("$.a.b[1]", doc).unwrap(), ["20"]);
        assert_eq!(texts("$.a.b[last]", doc).unwrap(), ["30"]);
        assert_eq!(texts("$.a.b[last - 1]", doc).unwrap(), ["20"]);
        assert_eq!(texts("$.a.b[0 to 1]", doc).unwrap(), ["10", "20"]);
        assert_eq!(texts("$.a.b[0, 2]", doc).unwrap(), ["10", "30"]);
        assert_eq!(texts("$.a.b[*]", doc).unwrap(), ["10", "20", "30"]);
        assert_eq!(texts("$.x.b", doc).unwrap(), ["1", "2"]);
        assert_eq!(texts("$.\"a b\"", doc).unwrap(), ["3"]);
        assert_eq!(texts("lax $.a.b[5]", doc).unwrap(), Vec::<String>::new());
        assert_eq!(texts("$.a.*", doc).unwrap(), ["[10,20,30]"]);
        assert_eq!(
            texts("lax $.a.b[0].c[0]", doc).unwrap(),
            Vec::<String>::new()
        );
        assert_eq!(texts("lax $[0]", "5").unwrap(), ["5"]);
        assert!(matches!(
            texts("strict $.c", doc),
            Err(PathError::Structure(_))
        ));
        assert!(matches!(
            texts("strict $.a.b[5]", doc),
            Err(PathError::Structure(_))
        ));
        assert!(matches!(texts("bad path", doc), Err(PathError::Syntax(_))));
        assert!(matches!(
            texts("$.a ? (@ > 1)", doc),
            Err(PathError::Unsupported(_))
        ));
        assert!(matches!(
            texts("$.a.type()", doc),
            Err(PathError::Unsupported(_))
        ));
    }
}
