// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The rules names, paths and values follow. The API layer turns each
//! failure into IBM's message and reason code.

/// Longest server and domain name.
pub const NETBIOS_NAME_MAX: usize = 15;
/// Longest share name (`NNLEN`).
pub const SHARE_NAME_MAX: usize = 12;
/// Longest text.
pub const TEXT_MAX: usize = 50;
/// Longest share path in bytes (`MAX_PATH_NAME`).
pub const PATH_MAX: usize = 1024;
/// Longest file extension in bytes (`Qzls_Extension_Table_Entry`).
pub const EXTENSION_MAX: usize = 46;
/// Most extensions one share takes.
pub const EXTENSIONS_MAX: usize = 100;

/// Why a name is not valid.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    /// Nothing given.
    #[error("the name is blank")]
    Empty,
    /// Longer than allowed.
    #[error("the name is longer than {0} characters")]
    TooLong(usize),
    /// Begins with a blank.
    #[error("the name begins with a blank")]
    LeadingBlank,
    /// A character the name may not hold.
    #[error("the character {0:?} is not allowed")]
    BadChar(char),
}

/// A server or domain name: A-Z, 0-9, `#`, `$`, `@` and embedded
/// blanks, at most 15, no leading blank (the rule `CPFB68B` states).
/// Lower case letters are taken as upper case. The name, trimmed and
/// upper case.
pub fn netbios_name(s: &str) -> Result<String, NameError> {
    let t = s.trim_end();
    if t.is_empty() {
        return Err(NameError::Empty);
    }
    if t.starts_with(' ') {
        return Err(NameError::LeadingBlank);
    }
    if t.chars().count() > NETBIOS_NAME_MAX {
        return Err(NameError::TooLong(NETBIOS_NAME_MAX));
    }
    let up = t.to_ascii_uppercase();
    if let Some(c) = up.chars().find(|c| {
        !(c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '#' | '$' | '@' | ' '))
    }) {
        return Err(NameError::BadChar(c));
    }
    Ok(up)
}

/// A share name: at most 12 characters, no leading blank, none of
/// the characters Windows reserves (`\ / : * ? " < > |`, `[ ] ; = + ,`)
/// or a control character. Upper case.
pub fn share_name(s: &str) -> Result<String, NameError> {
    let t = s.trim_end();
    if t.is_empty() {
        return Err(NameError::Empty);
    }
    if t.starts_with(' ') {
        return Err(NameError::LeadingBlank);
    }
    if t.chars().count() > SHARE_NAME_MAX {
        return Err(NameError::TooLong(SHARE_NAME_MAX));
    }
    if let Some(c) = t.chars().find(|c| {
        c.is_control()
            || matches!(
                c,
                '\\' | '/'
                    | ':'
                    | '*'
                    | '?'
                    | '"'
                    | '<'
                    | '>'
                    | '|'
                    | '['
                    | ']'
                    | ';'
                    | '='
                    | '+'
                    | ','
            )
    }) {
        return Err(NameError::BadChar(c));
    }
    Ok(t.to_uppercase())
}

/// An object name (`*AUTL`, `*OUTQ`, a profile): 1 to 10 characters,
/// A-Z, 0-9, `$`, `#`, `@`, `_`, `.`, not starting with a digit,
/// `_` or `.`. Upper case.
pub fn object_name(s: &str) -> Result<String, NameError> {
    let t = s.trim();
    if t.is_empty() {
        return Err(NameError::Empty);
    }
    if t.len() > 10 {
        return Err(NameError::TooLong(10));
    }
    let up = t.to_ascii_uppercase();
    let mut cs = up.chars();
    let first = cs.next().unwrap_or(' ');
    if !(first.is_ascii_uppercase() || matches!(first, '$' | '#' | '@')) {
        return Err(NameError::BadChar(first));
    }
    if let Some(c) = cs.find(|c| {
        !(c.is_ascii_uppercase() || c.is_ascii_digit() || matches!(c, '$' | '#' | '@' | '_' | '.'))
    }) {
        return Err(NameError::BadChar(c));
    }
    Ok(up)
}

/// Why a share path is not valid.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PathError {
    /// Its length is 0 or more than 1024 bytes.
    #[error("the path's length {0} is not between 1 and 1024")]
    Length(usize),
    /// It does not begin with `/`.
    #[error("the path does not begin with /")]
    NotAbsolute,
}

/// A share path: begins with `/`, 1 to 1024 bytes. Trailing slashes
/// (but the root's) are dropped.
pub fn path(p: &str) -> Result<String, PathError> {
    if p.is_empty() || p.len() > PATH_MAX {
        return Err(PathError::Length(p.len()));
    }
    if !p.starts_with('/') {
        return Err(PathError::NotAbsolute);
    }
    let t = p.trim_end_matches('/');
    Ok(if t.is_empty() { "/".into() } else { t.into() })
}

/// A file extension: 1 to 46 bytes with no `.`, `/` or blank inside.
/// Upper case, a leading `.` dropped.
pub fn extension(s: &str) -> Result<String, NameError> {
    let t = s.trim().trim_start_matches('.');
    if t.is_empty() {
        return Err(NameError::Empty);
    }
    if t.len() > EXTENSION_MAX {
        return Err(NameError::TooLong(EXTENSION_MAX));
    }
    if let Some(c) = t
        .chars()
        .find(|c| matches!(c, '.' | '/' | ' ' | '\\') || c.is_control())
    {
        return Err(NameError::BadChar(c));
    }
    Ok(t.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netbios_names_follow_cpfb68b() {
        assert_eq!(netbios_name("qs400").as_deref(), Ok("QS400"));
        assert_eq!(netbios_name("MY SERVER#1").as_deref(), Ok("MY SERVER#1"));
        assert_eq!(netbios_name(" X"), Err(NameError::LeadingBlank));
        assert_eq!(netbios_name("A-B"), Err(NameError::BadChar('-')));
        assert_eq!(
            netbios_name("ABCDEFGHIJKLMNOP"),
            Err(NameError::TooLong(15))
        );
        assert_eq!(netbios_name("   "), Err(NameError::Empty));
    }

    #[test]
    fn share_names_refuse_windows_reserved_characters() {
        assert_eq!(share_name("home").as_deref(), Ok("HOME"));
        assert_eq!(share_name("PUB-1_$").as_deref(), Ok("PUB-1_$"));
        assert_eq!(share_name("A/B"), Err(NameError::BadChar('/')));
        assert_eq!(share_name("ABCDEFGHIJKLM"), Err(NameError::TooLong(12)));
    }

    #[test]
    fn object_names_and_paths_and_extensions() {
        assert_eq!(object_name("qprint").as_deref(), Ok("QPRINT"));
        assert_eq!(object_name("1ABC"), Err(NameError::BadChar('1')));
        assert_eq!(path("/home/bob/").as_deref(), Ok("/home/bob"));
        assert_eq!(path("/").as_deref(), Ok("/"));
        assert_eq!(path("home"), Err(PathError::NotAbsolute));
        assert_eq!(path(""), Err(PathError::Length(0)));
        assert_eq!(extension(".txt").as_deref(), Ok("TXT"));
        assert_eq!(extension("a b"), Err(NameError::BadChar(' ')));
    }
}
