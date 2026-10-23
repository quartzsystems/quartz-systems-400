// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! RPG names as IR names. RPG names are upper case and may hold `@`
//! and `$`, which the IR does not allow; those become lower-case
//! escapes, and the compiler's own names are lower case, so no two
//! names collide.

/// The IR spelling of an upper-case RPG name.
#[must_use]
pub fn mangle(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        match c {
            '@' => out.push_str("_at_"),
            '$' => out.push_str("_dl_"),
            c if c.is_ascii_alphanumeric() || c == '_' || c == '#' => out.push(c.to_ascii_uppercase()),
            _ => out.push_str("_x_"),
        }
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, '_');
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn escapes_what_the_ir_refuses() {
        assert_eq!(super::mangle("CUST@NAME$"), "CUST_at_NAME_dl_");
        assert_eq!(super::mangle("#TOTAL"), "#TOTAL");
    }
}
