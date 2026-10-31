// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The function check a processing program ends with when something
//! it relies on fails in a way no message of its own describes: a read
//! over the job's link, a reply it cannot use. The `*ESCAPE` is
//! `CPF9898` with the command's name; the detail goes to the job log
//! only, as a `CPD9898` `*DIAG`. Raw internal text never reaches a
//! screen.

/// The escape.
pub const ESCAPE: &str = "CPF9898";

/// The diagnostic that carries the detail.
pub const DIAG: &str = "CPD9898";

/// A form of a command (a value of one of its parameters) that a later
/// release brings; the command's name is the data.
pub const LATER: &str = "CPF9B70";

/// The `CPF9898` escape's first-level text for a command. `CPF9898`
/// is `&1` (user programs send it with their own text), so the
/// sentence is its data; this is the one place it is written.
#[must_use]
pub fn text(cmd: &str) -> String {
    format!("Function check. Unexpected condition in {}.", cmd.trim())
}

/// The escape's data.
#[must_use]
pub fn escape_data(cmd: &str) -> Vec<u8> {
    cmd::error::data(&[(&text(cmd), 512)])
}

/// Whether escape data is a function check's (and not a user
/// program's own `CPF9898`).
#[must_use]
pub fn is_check_data(data: &[u8]) -> bool {
    let t = text("X");
    let head = &t[..t.len() - 2];
    data.starts_with(&cmd::error::data(&[(head, head.len())]))
}

/// The diagnostic's data: the command, then the detail.
#[must_use]
pub fn diag_data(cmd: &str, detail: &str) -> Vec<u8> {
    let detail: String = detail.split_whitespace().collect::<Vec<_>>().join(" ");
    let detail: String = detail.chars().take(480).collect();
    cmd::error::data(&[(cmd.trim(), 10), (&detail, 480)])
}

/// Whether a failure's text is internal detail rather than a message:
/// no leading identifier, or the service's catch-all `CPF9899`.
#[must_use]
pub fn is_internal(text: &str) -> bool {
    let t = text.trim_start();
    let b = t.as_bytes();
    let id = b.len() >= 7
        && b[..3].iter().all(u8::is_ascii_uppercase)
        && b[3..7].iter().all(u8::is_ascii_hexdigit)
        && b.get(7).is_none_or(|c| *c == b' ' || *c == b':' || *c == b'\n');
    !id || t.starts_with("CPF9899") || t.starts_with(ESCAPE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_text_is_told_from_messages() {
        assert!(is_internal("hw is answered by the console"));
        assert!(is_internal("CPF9899 no panel data named X"));
        assert!(is_internal("connection reset"));
        assert!(!is_internal("CPF2204 User profile X not found."));
        assert!(!is_internal("CPF9801"));
    }

    #[test]
    fn data_fits_the_descriptions() {
        assert_eq!(escape_data("DSPPTF").len(), 512);
        assert_eq!(diag_data("DSPPTF", "a\nb").len(), 490);
    }
}
