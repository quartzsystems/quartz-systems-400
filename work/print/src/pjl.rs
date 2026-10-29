// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The PJL job wrapper the `*HPPJLDRV` and `*IBMPJLDRV` drivers put
//! round what they send: the universal exit language sequence, `@PJL
//! JOB`, the language the data is in, the data, `@PJL EOJ`. Printer
//! status over PJL (`@PJL INFO STATUS`) is not read: the raw port is
//! write-only here, and `*IBMSNMPDRV`'s SNMP status is not done.

/// The universal exit language: leaves whatever language the printer
/// is in and starts PJL.
pub const UEL: &[u8] = b"\x1b%-12345X";

/// The language a PJL job enters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// PCL5.
    Pcl,
    /// PostScript.
    PostScript,
    /// PDF, on printers that take it.
    Pdf,
    /// Nothing declared: the printer decides from the data.
    Auto,
}

impl Language {
    fn name(self) -> Option<&'static str> {
        match self {
            Self::Pcl => Some("PCL"),
            Self::PostScript => Some("POSTSCRIPT"),
            Self::Pdf => Some("PDF"),
            Self::Auto => None,
        }
    }
}

/// A job name as PJL quotes it: printable ASCII, no quotes, at most 80.
fn quoted(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .filter(|c| *c != '"')
        .take(80)
        .collect()
}

/// `data` wrapped as one PJL job named `name`, in `language`, printed
/// `copies` times (when more than one).
#[must_use]
pub fn wrap(name: &str, language: Language, copies: u16, data: &[u8]) -> Vec<u8> {
    let name = quoted(name);
    let mut out = Vec::with_capacity(data.len() + 160);
    out.extend_from_slice(UEL);
    out.extend_from_slice(format!("@PJL JOB NAME=\"{name}\"\r\n").as_bytes());
    if copies > 1 {
        out.extend_from_slice(format!("@PJL SET COPIES={copies}\r\n").as_bytes());
    }
    if let Some(l) = language.name() {
        out.extend_from_slice(format!("@PJL ENTER LANGUAGE={l}\r\n").as_bytes());
    }
    out.extend_from_slice(data);
    out.extend_from_slice(UEL);
    out.extend_from_slice(format!("@PJL EOJ NAME=\"{name}\"\r\n").as_bytes());
    out.extend_from_slice(UEL);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_a_job() {
        let w = wrap("REPORT \"1\"", Language::Pcl, 2, b"\x1bEhello\x0c");
        let text = String::from_utf8_lossy(&w);
        assert!(text.starts_with("\x1b%-12345X@PJL JOB NAME=\"REPORT 1\"\r\n@PJL SET COPIES=2\r\n@PJL ENTER LANGUAGE=PCL\r\n\x1bEhello\x0c"));
        assert!(text.ends_with("\x1b%-12345X@PJL EOJ NAME=\"REPORT 1\"\r\n\x1b%-12345X"));
        let a = wrap("x", Language::Auto, 1, b"");
        assert!(!String::from_utf8_lossy(&a).contains("ENTER LANGUAGE"));
    }
}
