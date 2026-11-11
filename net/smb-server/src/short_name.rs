//! QS/400: 8.3 short names ([MS-FSCC] 2.1.5.2.1) for backends that store none.
//!
//! A backend that wants its long names to have short names gives each
//! listing's entries to [`assign`], with a stable value per file (its file
//! number), and sets [`FileInfo::short_name`](crate::FileInfo::short_name)
//! and [`Handle::short_name`](crate::Handle::short_name) from the result;
//! [`resolve`] finds the entry a client named by its short name.
//!
//! The form is Windows' hash form, `XXHHHH~1.EXT`: up to two characters of
//! the cleaned base name, four hex digits, `~1`, and up to three characters
//! of the cleaned extension. Windows takes the hex digits from a checksum of
//! the long name and stores the result; with nothing stored, they come from
//! the file's stable value instead, so a file keeps its short name across
//! listings, opens and jobs while its directory keeps it apart from the
//! others. Names that collide in a directory are settled in order of the
//! stable value (then the long name): the first keeps its candidate, a later
//! one takes the next hex value (`~1`, then `~2` to `~9` once the 65 536
//! values are taken), and no candidate is ever another entry's long name.
//! A short name changes only when its file is renamed, or when a file with a
//! smaller stable value and the same two characters and extension appears
//! and takes its candidate (one chance in 65 536 per such file).

use std::collections::HashSet;

/// The punctuation a valid 8.3 name may hold besides letters and digits.
const PUNCT: &str = "$%'-_@~`!(){}^#&";

fn legal_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || PUNCT.contains(c)
}

/// Whether `name` is a valid 8.3 name ([MS-FSCC] 2.1.5.2.1): a base of one
/// to eight characters and an optional extension of one to three, both of
/// ASCII letters, digits and `$%'-_@~`!(){}^#&`. Case is not considered.
pub fn is_8dot3(name: &str) -> bool {
    let (base, ext) = match name.split_once('.') {
        Some((b, e)) => (b, Some(e)),
        None => (name, None),
    };
    let ok = |s: &str, max: usize| (1..=max).contains(&s.len()) && s.chars().all(legal_char);
    ok(base, 8) && ext.is_none_or(|e| ok(e, 3))
}

/// Whether `name` needs a short name. A valid 8.3 name needs none where
/// names are compared without regard to case. In a case-sensitive directory
/// a valid 8.3 name with lower-case letters needs one too, since its
/// upper-case form, which a client sees as the same name, may be another
/// file's.
pub fn needs_short_name(name: &str, case_sensitive: bool) -> bool {
    !is_8dot3(name) || (case_sensitive && name.bytes().any(|b| b.is_ascii_lowercase()))
}

/// The cleaned characters of a part of a long name: upper case, spaces and
/// dots dropped, anything else a short name cannot hold as `_`.
fn clean(s: &str) -> String {
    s.chars()
        .filter(|&c| c != ' ' && c != '.')
        .map(|c| {
            let c = c.to_ascii_uppercase();
            if legal_char(c) { c } else { '_' }
        })
        .collect()
}

/// The two characters and the extension a long name's short name keeps.
fn parts(name: &str) -> (String, String) {
    let trimmed = name.trim_start_matches('.');
    let (base, ext) = match trimmed.rfind('.') {
        Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
        None => (trimmed, ""),
    };
    let base: String = clean(base).chars().take(2).collect();
    let ext: String = clean(ext).chars().take(3).collect();
    (base, ext)
}

/// Where the probe for a stable value starts.
fn start(id: u64) -> u16 {
    // Fibonacci hashing: the top 16 bits of the product.
    (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 48) as u16
}

/// The `n`th candidate short name of the long name `name` with stable value
/// `id`; `None` past the last (`9 * 65536`).
pub fn candidate(name: &str, id: u64, n: u32) -> Option<String> {
    if n >= 9 * 65_536 {
        return None;
    }
    let (base, ext) = parts(name);
    let hex = start(id).wrapping_add((n & 0xFFFF) as u16);
    let tail = n / 65_536 + 1;
    let mut s = format!("{base}{hex:04X}~{tail}");
    if !ext.is_empty() {
        s.push('.');
        s.push_str(&ext);
    }
    Some(s)
}

/// The short names of a directory's entries, given as `(long name, stable
/// value)`, in the same order: `None` for an entry that needs none
/// ([`needs_short_name`]). `.` and `..` are not entries.
pub fn assign(entries: &[(&str, u64)], case_sensitive: bool) -> Vec<Option<String>> {
    // Every long name a client could type as a short name is taken.
    let mut taken: HashSet<String> = entries
        .iter()
        .filter(|(n, _)| is_8dot3(n))
        .map(|(n, _)| n.to_ascii_uppercase())
        .collect();
    let mut order: Vec<usize> = (0..entries.len())
        .filter(|&i| needs_short_name(entries[i].0, case_sensitive))
        .collect();
    order.sort_by(|&a, &b| (entries[a].1, entries[a].0).cmp(&(entries[b].1, entries[b].0)));
    let mut out = vec![None; entries.len()];
    for i in order {
        let (name, id) = entries[i];
        let mut n = 0;
        while let Some(c) = candidate(name, id, n) {
            if taken.insert(c.clone()) {
                out[i] = Some(c);
                break;
            }
            n += 1;
        }
    }
    out
}

/// Whether `name` could be a short name [`assign`] gives: a valid 8.3 name
/// with a `~`. Backends look a name up as a short name only then.
pub fn could_be_short(name: &str) -> bool {
    name.contains('~') && is_8dot3(name)
}

/// The entry of `entries` (as [`assign`] takes them) whose short name is
/// `short`, compared without regard to case.
pub fn resolve(entries: &[(&str, u64)], case_sensitive: bool, short: &str) -> Option<usize> {
    if !could_be_short(short) {
        return None;
    }
    assign(entries, case_sensitive)
        .iter()
        .position(|s| s.as_deref().is_some_and(|s| s.eq_ignore_ascii_case(short)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_8dot3_names() {
        for n in [
            "README.TXT",
            "readme.txt",
            "A",
            "AUTOEXEC.BAT",
            "X~1.Y",
            "$$$.{}",
        ] {
            assert!(is_8dot3(n), "{n}");
        }
        for n in [
            "LONGFILENAME.TXT",
            "A.HTML",
            "a b.txt",
            ".txt",
            "x.",
            "a.b.c",
            "a+b.txt",
            "caf\u{e9}.txt",
            "",
        ] {
            assert!(!is_8dot3(n), "{n}");
        }
    }

    #[test]
    fn case_rules() {
        assert!(!needs_short_name("readme.txt", false));
        assert!(needs_short_name("readme.txt", true));
        assert!(!needs_short_name("README.TXT", true));
        assert!(needs_short_name("Long File Name.html", false));
    }

    #[test]
    fn the_hash_form() {
        let c = candidate("Long File Name.html", 0, 0).unwrap();
        assert_eq!(c, "LO0000~1.HTM");
        let c = candidate("a+b;c.tar.gz", 1, 0).unwrap();
        assert!(c.starts_with("A_") && c.ends_with("~1.GZ"), "{c}");
        assert_eq!(candidate(".htaccess", 0, 0).unwrap(), "HT0000~1");
        assert_eq!(candidate("x.y", 0, 65_536).unwrap(), "X0000~2.Y");
        assert!(candidate("x", 0, 9 * 65_536).is_none());
        let c = candidate("Long File Name.html", 12_345, 0).unwrap();
        assert!(is_8dot3(&c) && could_be_short(&c));
    }

    #[test]
    fn unique_and_stable() {
        // Many names sharing their two characters and extension, some
        // with the same stable value.
        let names: Vec<String> = (0..2000).map(|i| format!("Report {i}.docx")).collect();
        let entries: Vec<(&str, u64)> = names
            .iter()
            .enumerate()
            .map(|(i, n)| (n.as_str(), (i % 1500) as u64 * 7))
            .collect();
        let short = assign(&entries, false);
        let set: HashSet<_> = short.iter().map(|s| s.clone().unwrap()).collect();
        assert_eq!(set.len(), names.len());
        // The same in any order.
        let mut rev = entries.clone();
        rev.reverse();
        let mut back = assign(&rev, false);
        back.reverse();
        assert_eq!(short, back);
        // A long name that is a candidate is never given.
        let first = short[0].clone().unwrap();
        let with = [(first.as_str(), 99_999), entries[0]];
        let again = assign(&with, false);
        assert_eq!(again[0], None);
        assert_ne!(again[1].as_deref(), Some(first.as_str()));
        assert_eq!(
            resolve(&with, false, &again[1].clone().unwrap().to_lowercase()),
            Some(1)
        );
        assert_eq!(resolve(&with, false, "README.TXT"), None);
    }
}
