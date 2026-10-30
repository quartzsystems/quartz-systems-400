// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client's subcommands: their names, the shortest form each takes,
//! and the words IBM's client also answers to (`!` for `SYSCMD`, `BYE`
//! for `QUIT` ...).

/// A subcommand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum Sub {
    Open,
    User,
    Pass,
    Close,
    Quit,
    Cd,
    Cdup,
    Pwd,
    Lcd,
    Lpwd,
    Ls,
    Dir,
    Get,
    Put,
    Append,
    Mget,
    Mput,
    Delete,
    Rename,
    Mkdir,
    Rmdir,
    Binary,
    Ascii,
    Ebcdic,
    Type,
    Mode,
    Struct,
    Namefmt,
    Quote,
    Site,
    Syscmd,
    Locsite,
    Sendsite,
    Sendpasv,
    Sunique,
    Debug,
    Status,
    System,
    Help,
    Noop,
}

/// Name, shortest form, subcommand.
pub const TABLE: &[(&str, usize, Sub)] = &[
    ("APPEND", 2, Sub::Append),
    ("ASCII", 2, Sub::Ascii),
    ("BINARY", 3, Sub::Binary),
    ("CD", 2, Sub::Cd),
    ("CDUP", 3, Sub::Cdup),
    ("CLOSE", 3, Sub::Close),
    ("DEBUG", 3, Sub::Debug),
    ("DELETE", 3, Sub::Delete),
    ("DIR", 3, Sub::Dir),
    ("EBCDIC", 2, Sub::Ebcdic),
    ("GET", 3, Sub::Get),
    ("HELP", 1, Sub::Help),
    ("LCD", 3, Sub::Lcd),
    ("LOCSITE", 4, Sub::Locsite),
    ("LPWD", 4, Sub::Lpwd),
    ("LS", 2, Sub::Ls),
    ("MGET", 2, Sub::Mget),
    ("MKDIR", 3, Sub::Mkdir),
    ("MODE", 3, Sub::Mode),
    ("MPUT", 2, Sub::Mput),
    ("NAMEFMT", 3, Sub::Namefmt),
    ("NOOP", 3, Sub::Noop),
    ("OPEN", 1, Sub::Open),
    ("PASS", 4, Sub::Pass),
    ("PUT", 3, Sub::Put),
    ("PWD", 2, Sub::Pwd),
    ("QUIT", 1, Sub::Quit),
    ("QUOTE", 3, Sub::Quote),
    ("RENAME", 3, Sub::Rename),
    ("RMDIR", 3, Sub::Rmdir),
    ("SENDPASV", 5, Sub::Sendpasv),
    ("SENDSITE", 5, Sub::Sendsite),
    ("SITE", 2, Sub::Site),
    ("STATUS", 4, Sub::Status),
    ("STRUCT", 3, Sub::Struct),
    ("SUNIQUE", 2, Sub::Sunique),
    ("SYSCMD", 4, Sub::Syscmd),
    ("SYSTEM", 4, Sub::System),
    ("TYPE", 2, Sub::Type),
    ("USER", 1, Sub::User),
];

/// Other words for subcommands.
const ALIASES: &[(&str, Sub)] = &[
    ("!", Sub::Syscmd),
    ("BYE", Sub::Quit),
    ("END", Sub::Quit),
    ("EXIT", Sub::Quit),
    ("IMAGE", Sub::Binary),
    ("MDELETE", Sub::Delete),
    ("RECV", Sub::Get),
    ("SEND", Sub::Put),
    ("RCMD", Sub::Quote),
    ("?", Sub::Help),
];

impl Sub {
    /// The subcommand a line starts with, and the rest of the line.
    pub fn parse(line: &str) -> Option<(Sub, &str)> {
        let line = line.trim_start();
        // `!cmd` needs no blank.
        if let Some(rest) = line.strip_prefix('!') {
            return Some((Sub::Syscmd, rest));
        }
        let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let w = word.to_ascii_uppercase();
        if let Some((_, s)) = ALIASES.iter().find(|(a, _)| *a == w) {
            // `RCMD x` goes to the server as `RCMD x`.
            if *s == Sub::Quote && w == "RCMD" {
                return Some((Sub::Quote, line));
            }
            return Some((*s, rest));
        }
        TABLE
            .iter()
            .find(|(name, min, _)| w.len() >= *min && name.starts_with(w.as_str()))
            .map(|(_, _, s)| (*s, rest))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_short_forms() {
        assert_eq!(Sub::parse("get a b"), Some((Sub::Get, "a b")));
        assert_eq!(Sub::parse("BIN").map(|x| x.0), Some(Sub::Binary));
        assert_eq!(Sub::parse("q").map(|x| x.0), Some(Sub::Quit));
        assert_eq!(Sub::parse("!DSPLIBL"), Some((Sub::Syscmd, "DSPLIBL")));
        assert_eq!(Sub::parse("quote rcmd x").map(|x| x.0), Some(Sub::Quote));
        assert_eq!(Sub::parse("rcmd CHKOBJ"), Some((Sub::Quote, "rcmd CHKOBJ")));
        assert_eq!(Sub::parse("sendpasv off").map(|x| x.0), Some(Sub::Sendpasv));
        assert!(Sub::parse("xyz").is_none());
        // Every name is found by itself and by its shortest form.
        for (name, min, s) in TABLE {
            assert_eq!(Sub::parse(name).map(|x| x.0), Some(*s), "{name}");
            assert_eq!(Sub::parse(&name[..*min]).map(|x| x.0), Some(*s), "{name} short");
        }
    }
}
