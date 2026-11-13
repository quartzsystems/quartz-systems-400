// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! XDR names as Rust identifiers: kept as they are, a Rust keyword
//! raw (`r#type`), the four that cannot be raw with `_` added.

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "dyn", "else", "enum", "extern", "false",
    "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "static", "struct", "trait", "true", "type", "unsafe", "use", "where",
    "while", "abstract", "become", "box", "do", "final", "macro", "override", "priv", "typeof",
    "unsized", "virtual", "yield", "try",
];

/// `name` as a Rust identifier.
pub fn ident(name: &str) -> String {
    if matches!(name, "self" | "Self" | "super" | "crate") {
        format!("{name}_")
    } else if KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::ident;

    #[test]
    fn keywords() {
        assert_eq!(ident("type"), "r#type");
        assert_eq!(ident("priv"), "r#priv");
        assert_eq!(ident("self"), "self_");
        assert_eq!(ident("fattr3"), "fattr3");
    }
}
