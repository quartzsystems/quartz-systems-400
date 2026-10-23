// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A corpus member and the directives in its header comments. A
//! directive is a comment that starts with `;`: `//; key: value` in
//! free form, `*; key: value` with the `*` in column 7 in fixed form.
//!
//! - `cmd:` the compile command, with `{lib}` and `{name}`; the
//!   default is `CRTBNDRPG` with `DBGVIEW(*NONE) OUTPUT(*PRINT)`.
//! - `call:` the call command; `run: no` compiles only.
//! - `rcdlen:` the source file's record length (112 by default).
//! - `setup:` a command run before the compile (several allowed).
//! - `target:` `pub400`, the only system with a compiler.

use std::path::Path;

/// One member of the corpus.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Case {
    /// Member name, upper case, at most 10 characters.
    pub name: String,
    /// The source lines.
    pub lines: Vec<String>,
    /// The compile command template.
    pub cmd: String,
    /// The call command template; empty when the case only compiles.
    pub call: String,
    /// Source record length.
    pub rcdlen: usize,
    /// Commands before the compile.
    pub setup: Vec<String>,
}

/// The default compile command.
pub const DEFAULT_CMD: &str =
    "CRTBNDRPG PGM({lib}/{name}) SRCFILE({lib}/QRPGLESRC) SRCMBR({name}) DBGVIEW(*NONE) OUTPUT(*PRINT)";

/// The default call command.
pub const DEFAULT_CALL: &str = "CALL PGM({lib}/{name})";

impl Case {
    /// Read a member from its text.
    #[must_use]
    pub fn parse(name: &str, text: &str) -> Self {
        let mut c = Self {
            name: name.to_ascii_uppercase(),
            lines: text.replace("\r\n", "\n").lines().map(str::to_string).collect(),
            cmd: DEFAULT_CMD.into(),
            call: DEFAULT_CALL.into(),
            rcdlen: 112,
            setup: Vec::new(),
        };
        for l in c.lines.clone() {
            let Some((key, value)) = directive(&l) else {
                continue;
            };
            match key.as_str() {
                "cmd" => c.cmd = value,
                "call" => c.call = value,
                "run" if value.eq_ignore_ascii_case("no") => c.call.clear(),
                "rcdlen" => c.rcdlen = value.parse().unwrap_or(112),
                "setup" => c.setup.push(value),
                _ => {}
            }
        }
        c
    }

    /// Read a member from a file named `NAME.rpgle` (or `.sqlrpgle`).
    pub fn load(path: &Path) -> std::io::Result<Self> {
        let text = std::fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        Ok(Self::parse(&name, &text))
    }

    /// A template with `{lib}` and `{name}` filled.
    #[must_use]
    pub fn fill(&self, template: &str, lib: &str) -> String {
        template.replace("{lib}", lib).replace("{name}", &self.name)
    }

    /// The source's data width: the record length less the sequence
    /// and date fields.
    #[must_use]
    pub fn width(&self) -> usize {
        self.rcdlen.saturating_sub(12)
    }

    /// Whether the member is too wide for its record length.
    #[must_use]
    pub fn too_wide(&self) -> Option<usize> {
        self.lines
            .iter()
            .position(|l| l.chars().count() > self.width())
            .map(|i| i + 1)
    }
}

/// A directive comment: `//; key: value` or `*; key: value` in column 7.
fn directive(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    let rest = if let Some(r) = t.strip_prefix("//;") {
        r
    } else if line.chars().nth(6) == Some('*') && line.chars().nth(7) == Some(';') {
        &line[line.char_indices().nth(8)?.0..]
    } else {
        return None;
    };
    let (k, v) = rest.split_once(':')?;
    Some((k.trim().to_ascii_lowercase(), v.trim().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_directives_in_both_forms() {
        let c = Case::parse(
            "hello",
            "**FREE\n//; cmd: CRTRPGMOD MODULE({lib}/{name})\n//; run: no\n",
        );
        assert_eq!(c.name, "HELLO");
        assert_eq!(c.fill(&c.cmd, "L"), "CRTRPGMOD MODULE(L/HELLO)");
        assert!(c.call.is_empty());
        let f = Case::parse("x", "      *; rcdlen: 252\n      *; setup: DLTF A\n");
        assert_eq!(f.rcdlen, 252);
        assert_eq!(f.setup, vec!["DLTF A".to_string()]);
    }
}
