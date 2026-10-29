// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The regular expression engine behind `grep` and `sed`: a table of
//! POSIX cases (leftmost-longest, BRE and ERE differences, brackets,
//! intervals, groups, back-references, anchors, errors), and a random
//! comparison against a brute-force matcher for a simple subset.

#![allow(clippy::type_complexity, clippy::cast_possible_truncation)]

use qsh_utils::regex::{Error, Regex, Syntax};

const B: Syntax = Syntax::Basic;
const E: Syntax = Syntax::Extended;

/// (pattern, syntax, text, expected whole-match span).
const SPANS: &[(&str, Syntax, &str, Option<(usize, usize)>)] = &[
    // Literals and the dot.
    ("abc", B, "xxabcxx", Some((2, 5))),
    ("abc", B, "ab", None),
    ("a.c", B, "abc", Some((0, 3))),
    ("a.c", B, "a\nc", Some((0, 3))),
    ("", B, "abc", Some((0, 0))),
    ("", E, "", Some((0, 0))),
    // Anchors.
    ("^abc", B, "abc", Some((0, 3))),
    ("^abc", B, "xabc", None),
    ("abc$", B, "xabc", Some((1, 4))),
    ("abc$", B, "abcx", None),
    ("^$", B, "", Some((0, 0))),
    ("^$", B, "x", None),
    ("a^b", B, "a^b", Some((0, 3))),
    ("a$b", B, "a$b", Some((0, 3))),
    ("^*a", B, "*a", Some((0, 2))),
    ("^^a", B, "^a", Some((0, 2))),
    ("\\(^a\\)", B, "a", Some((0, 1))),
    ("(^a|b)", E, "cb", Some((1, 2))),
    ("a|^b", E, "b", Some((0, 1))),
    ("x$|y", E, "xy", Some((1, 2))),
    // Stars and leftmost-longest.
    ("a*", B, "aaa", Some((0, 3))),
    ("a*", B, "baaa", Some((0, 0))),
    ("ba*", B, "baaa", Some((0, 4))),
    ("*a", B, "x*a", Some((1, 3))),
    ("\\(*a\\)", B, "*a", Some((0, 2))),
    ("a**", B, "aa", Some((0, 2))),
    ("(a|ab)(c|bcd)(d*)", E, "abcd", Some((0, 4))),
    ("ab|abcd", E, "abcd", Some((0, 4))),
    ("x(a|ab)y", E, "xaby", Some((0, 4))),
    ("(a*)(ab)*b", E, "aab", Some((0, 3))),
    ("(.*)c", E, "abcbc", Some((0, 5))),
    ("a+", E, "aaa", Some((0, 3))),
    ("a+", E, "bbb", None),
    ("a?b", E, "b", Some((0, 1))),
    ("a?b", E, "ab", Some((0, 2))),
    ("(ab)+", E, "ababx", Some((0, 4))),
    ("(a|b)*c", E, "abbac", Some((0, 5))),
    // BRE treats + ? | ( ) { } as literals.
    ("a+", B, "a+", Some((0, 2))),
    ("a?", B, "a?", Some((0, 2))),
    ("a|b", B, "a|b", Some((0, 3))),
    ("(a)", B, "(a)", Some((0, 3))),
    ("a{2}", B, "a{2}", Some((0, 4))),
    // ERE leading quantifiers and braces are literal.
    ("*a", E, "*a", Some((0, 2))),
    ("+a", E, "+a", Some((0, 2))),
    ("a{", E, "a{", Some((0, 2))),
    ("a{x}", E, "a{x}", Some((0, 4))),
    ("a{,2}", E, "a{,2}", Some((0, 5))),
    // Intervals.
    ("a\\{2\\}", B, "aaaa", Some((0, 2))),
    ("a\\{2,\\}", B, "aaaa", Some((0, 4))),
    ("a\\{2,3\\}", B, "aaaa", Some((0, 3))),
    ("a\\{0\\}b", B, "ab", Some((1, 2))),
    ("a{2}", E, "aaaa", Some((0, 2))),
    ("a{1,2}", E, "aaa", Some((0, 2))),
    ("(ab){2,}", E, "abababx", Some((0, 6))),
    ("a{0,1}b", E, "aab", Some((1, 3))),
    ("x{3}", E, "xx", None),
    // Brackets.
    ("[abc]", B, "xxbx", Some((2, 3))),
    ("[^abc]", B, "abcd", Some((3, 4))),
    ("[a-c]*", B, "abcd", Some((0, 3))),
    ("[]a]", B, "]", Some((0, 1))),
    ("[^]a]", B, "]ab", Some((2, 3))),
    ("[a-]", B, "-", Some((0, 1))),
    ("[-a]", B, "-", Some((0, 1))),
    ("[[:alpha:]]*", B, "ab1", Some((0, 2))),
    ("[[:digit:]]\\{3\\}", B, "a1234", Some((1, 4))),
    ("[[:space:]]", B, "a b", Some((1, 2))),
    ("[[:upper:][:digit:]]*", B, "A1b", Some((0, 2))),
    ("[[:punct:]]", B, "ab,", Some((2, 3))),
    ("[[:xdigit:]]*", B, "fFz", Some((0, 2))),
    ("[[.a.]]", B, "a", Some((0, 1))),
    ("[[=a=]]", B, "a", Some((0, 1))),
    ("[[.-.]a]", B, "-", Some((0, 1))),
    ("[a.]", B, "x.", Some((1, 2))),
    ("[*]", B, "*", Some((0, 1))),
    ("[\\]", B, "\\", Some((0, 1))),
    ("[|]", E, "|", Some((0, 1))),
    ("[$]", B, "$", Some((0, 1))),
    ("[^\n]*", B, "ab\ncd", Some((0, 2))),
    // Escapes.
    ("a\\.c", B, "abc a.c", Some((4, 7))),
    ("\\*", B, "a*", Some((1, 2))),
    ("\\$", B, "a$", Some((1, 2))),
    ("\\^", B, "a^", Some((1, 2))),
    ("\\[", B, "[", Some((0, 1))),
    ("a\\|b", E, "a|b", Some((0, 3))),
    ("\\(", E, "(", Some((0, 1))),
    ("\\{", E, "{", Some((0, 1))),
    ("a\\nb", B, "a\nb", Some((0, 3))),
    ("a\\tb", B, "a\tb", Some((0, 3))),
    // Groups and back-references.
    ("\\(a\\)\\1", B, "aa", Some((0, 2))),
    ("\\(a*\\)\\1", B, "aaaa", Some((0, 4))),
    ("\\(a*\\)\\1b", B, "aaab", Some((1, 4))),
    ("\\(.\\)\\1", B, "abccd", Some((2, 4))),
    ("\\(a\\)\\(b\\)\\2\\1", B, "abba", Some((0, 4))),
    ("(a)\\1", E, "aa", Some((0, 2))),
    ("(a|b)\\1", E, "abb", Some((1, 3))),
    ("\\(ab*\\)c\\1", B, "abbcabb", Some((0, 7))),
    ("\\(ab*\\)c\\1", B, "abbcab", None),
    ("()", E, "x", Some((0, 0))),
    ("(a*)*", E, "aaa", Some((0, 3))),
    ("(a*)+", E, "aaa", Some((0, 3))),
    ("(a|)*b", E, "aab", Some((0, 3))),
    ("((a)|b)*", E, "ab", Some((0, 2))),
];

#[test]
fn spans() {
    let mut bad = Vec::new();
    for (pat, syn, text, want) in SPANS {
        let r = Regex::new(pat.as_bytes(), *syn, false)
            .unwrap_or_else(|e| panic!("{pat:?} did not compile: {e}"));
        let got = r.find_at(text.as_bytes(), 0).map(|m| (m.start, m.end));
        if got != *want {
            bad.push(format!("{pat:?} {syn:?} on {text:?}: got {got:?}, want {want:?}"));
        }
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// (pattern, syntax, text, groups 1.. as text).
const GROUPS: &[(&str, Syntax, &str, &[Option<&str>])] = &[
    ("\\(a*\\)\\(b*\\)", B, "aabb", &[Some("aa"), Some("bb")]),
    ("(a+)(b+)?", E, "aa", &[Some("aa"), None]),
    ("(a)|(b)", E, "b", &[None, Some("b")]),
    ("(x(y)z)", E, "xyz", &[Some("xyz"), Some("y")]),
    ("\\(.*\\) \\(.*\\)", B, "john smith", &[Some("john"), Some("smith")]),
    ("([0-9]+)-([0-9]+)", E, "tel 555-1234", &[Some("555"), Some("1234")]),
    ("(a*)b", E, "b", &[Some("")]),
    ("(ab)*", E, "ababab", &[Some("ab")]),
];

#[test]
fn groups() {
    for (pat, syn, text, want) in GROUPS {
        let r = Regex::new(pat.as_bytes(), *syn, false).expect("compile");
        let m = r.find_at(text.as_bytes(), 0).expect("match");
        for (i, w) in want.iter().enumerate() {
            let got = m.group(text.as_bytes(), i + 1).map(|g| String::from_utf8_lossy(g).into_owned());
            assert_eq!(got.as_deref(), *w, "{pat:?} group {} on {text:?}", i + 1);
        }
        assert_eq!(r.groups(), want.len(), "{pat:?} group count");
    }
}

#[test]
fn errors() {
    let cases: &[(&str, Syntax, Error)] = &[
        ("a[", B, Error::Unmatched('[')),
        ("[a", E, Error::Unmatched('[')),
        ("[[:alpha:]", B, Error::Unmatched('[')),
        ("\\(a", B, Error::Unmatched('(')),
        ("(a", E, Error::Unmatched('(')),
        ("a\\)", B, Error::Unmatched(')')),
        ("a)", E, Error::Unmatched(')')),
        ("a\\", B, Error::TrailingBackslash),
        ("a\\", E, Error::TrailingBackslash),
        ("a\\{1", B, Error::BadInterval),
        ("a\\{2,1\\}", B, Error::BadInterval),
        ("a\\{x\\}", B, Error::BadInterval),
        ("a\\{256\\}", B, Error::BadInterval),
        ("\\1", B, Error::BadBackref),
        ("\\(a\\1\\)", B, Error::BadBackref),
        ("(a)\\2", E, Error::BadBackref),
        ("[[:nope:]]", B, Error::BadClass),
        ("[[.ab.]]", B, Error::BadClass),
        ("[z-a]", B, Error::BadRange),
        ("^*", E, Error::BadRepeat),
        ("$*", E, Error::BadRepeat),
    ];
    for (pat, syn, want) in cases {
        match Regex::new(pat.as_bytes(), *syn, false) {
            Ok(_) => panic!("{pat:?} {syn:?} compiled; want {want:?}"),
            Err(e) => assert_eq!(e, *want, "{pat:?} {syn:?}"),
        }
    }
}

#[test]
fn case_folding() {
    let cases: &[(&str, Syntax, &str, bool)] = &[
        ("abc", B, "ABC", true),
        ("[a-c]x", E, "BX", true),
        ("[^a]", B, "A", false),
        ("\\(a\\)\\1", B, "aA", true),
        ("Q", B, "q", true),
        ("[[:lower:]]", B, "A", true),
    ];
    for (pat, syn, text, want) in cases {
        let r = Regex::new(pat.as_bytes(), *syn, true).expect("compile");
        assert_eq!(r.is_match(text.as_bytes()), *want, "{pat:?} on {text:?}");
    }
    let lit = Regex::literal(b"a.B", true);
    assert!(lit.is_match(b"xA.bx"));
    assert!(!lit.is_match(b"axb"));
}

#[test]
fn find_all_steps() {
    let spans = |pat: &str, syn: Syntax, text: &str| -> Vec<(usize, usize)> {
        Regex::new(pat.as_bytes(), syn, false)
            .expect("compile")
            .find_all(text.as_bytes())
            .into_iter()
            .map(|m| (m.start, m.end))
            .collect()
    };
    assert_eq!(spans("a", B, "banana"), vec![(1, 2), (3, 4), (5, 6)]);
    assert_eq!(spans("an", B, "banana"), vec![(1, 3), (3, 5)]);
    assert_eq!(spans("x*", B, "ab"), vec![(0, 0), (1, 1), (2, 2)]);
    assert_eq!(spans("^a", B, "aaa"), vec![(0, 1)]);
    assert_eq!(spans("a$", B, "aaa"), vec![(2, 3)]);
    assert_eq!(spans("[0-9]+", E, "a1b22c333"), vec![(1, 2), (3, 5), (6, 9)]);
}

#[test]
fn pathological_patterns_finish() {
    let long = "a".repeat(3000);
    let cases: &[(&str, Syntax)] = &[
        ("(a*)*b", E),
        ("(a|aa)*b", E),
        ("(a?){30}a{30}b", E),
        ("((a*)*)*b", E),
        ("(.*)(.*)(.*)(.*)(.*)b", E),
        ("\\(a*\\)*b", B),
    ];
    for (pat, syn) in cases {
        let r = Regex::new(pat.as_bytes(), *syn, false).expect("compile");
        assert!(!r.is_match(long.as_bytes()), "{pat:?}");
    }
    // Repeated groups at the limit still compile.
    assert!(Regex::new(b"(a{0,255}){0,4}", E, false).is_ok());
}

/// A small deterministic generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// A simple pattern: atoms of `a`, `b`, `.`, each maybe starred.
#[derive(Clone, Copy, Debug)]
enum Atom {
    Lit(u8),
    Dot,
}

fn atom_matches(a: Atom, b: u8) -> bool {
    match a {
        Atom::Lit(x) => x == b,
        Atom::Dot => true,
    }
}

/// Every end reachable matching `pat` from `pos`.
fn ends(pat: &[(Atom, bool)], text: &[u8], pos: usize, out: &mut Vec<usize>) {
    let Some(&(atom, star)) = pat.first() else {
        out.push(pos);
        return;
    };
    if star {
        ends(&pat[1..], text, pos, out);
        let mut p = pos;
        while p < text.len() && atom_matches(atom, text[p]) {
            p += 1;
            ends(&pat[1..], text, p, out);
        }
    } else if pos < text.len() && atom_matches(atom, text[pos]) {
        ends(&pat[1..], text, pos + 1, out);
    }
}

fn brute(pat: &[(Atom, bool)], text: &[u8]) -> Option<(usize, usize)> {
    for s in 0..=text.len() {
        let mut v = Vec::new();
        ends(pat, text, s, &mut v);
        if let Some(e) = v.into_iter().max() {
            return Some((s, e));
        }
    }
    None
}

#[test]
fn random_simple_patterns_agree_with_brute_force() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..4000 {
        let n = 1 + rng.below(5);
        let pat: Vec<(Atom, bool)> = (0..n)
            .map(|_| {
                let atom = match rng.below(3) {
                    0 => Atom::Lit(b'a'),
                    1 => Atom::Lit(b'b'),
                    _ => Atom::Dot,
                };
                (atom, rng.below(2) == 0)
            })
            .collect();
        let text: Vec<u8> = (0..rng.below(10))
            .map(|_| if rng.below(2) == 0 { b'a' } else { b'b' })
            .collect();
        let mut src = String::new();
        for (a, star) in &pat {
            src.push(match a {
                Atom::Lit(c) => *c as char,
                Atom::Dot => '.',
            });
            if *star {
                src.push('*');
            }
        }
        for syn in [B, E] {
            let r = Regex::new(src.as_bytes(), syn, false).expect("compile");
            let got = r.find_at(&text, 0).map(|m| (m.start, m.end));
            assert_eq!(
                got,
                brute(&pat, &text),
                "{src:?} on {:?}",
                String::from_utf8_lossy(&text)
            );
        }
    }
}

#[test]
fn random_patterns_never_panic() {
    let alphabet: &[&str] = &[
        "a", "b", ".", "*", "+", "?", "|", "(", ")", "[", "]", "^", "$", "\\", "{", "}", ",",
        "1", "2", "-", "[:alpha:]", "\\(", "\\)", "\\{", "\\}", "\\1",
    ];
    let mut rng = Rng(42);
    for _ in 0..20_000 {
        let n = rng.below(10);
        let pat: String = (0..n).map(|_| alphabet[rng.below(alphabet.len())]).collect();
        for syn in [B, E] {
            if let Ok(r) = Regex::new(pat.as_bytes(), syn, rng.below(2) == 0) {
                let _ = r.find_all(b"ab(a)b{1}a*b");
            }
        }
    }
}
