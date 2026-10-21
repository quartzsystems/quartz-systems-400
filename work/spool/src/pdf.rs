// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A minimal PDF writer: text only, the standard Courier font, one PDF
//! page per spooled page. The page is the printer's: its width from the
//! print positions and characters per inch, its height from the lines
//! and lines per inch, with a quarter inch margin all round. The output
//! is deterministic: no dates, no identifiers.

use std::fmt::Write as _;

use crate::scs::Page;
use crate::scs::decode::Format;

/// Points per inch.
const PT: f64 = 72.0;
/// The margin, in points.
const MARGIN: f64 = 18.0;

/// A PDF string literal: printable ASCII as is, `(`, `)` and `\`
/// escaped, anything else as `?`.
fn literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('(');
    for c in s.chars() {
        match c {
            '(' | ')' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            ' '..='~' => out.push(c),
            _ => out.push('?'),
        }
    }
    out.push(')');
    out
}

/// A number as PDF writes it: at most two decimals, no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The PDF of `pages` laid out in `fmt`.
#[must_use]
pub fn render(pages: &[Page], fmt: Format) -> Vec<u8> {
    let cpi = f64::from(fmt.cpi.max(1));
    let lpi = f64::from(fmt.lpi.max(1));
    let width = f64::from(fmt.width.max(1)) * PT / cpi + 2.0 * MARGIN;
    let height = f64::from(fmt.len.max(1)) * PT / lpi + 2.0 * MARGIN;
    // Courier is 0.6 em wide: the size that makes one character 1/cpi.
    let size = PT / cpi / 0.6;
    let leading = PT / lpi;
    let n = pages.len().max(1);
    // Objects: 1 catalog, 2 pages, 3 font, then a page and its content
    // for each page.
    let mut objects: Vec<String> = Vec::new();
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objects.push(format!(
        "<< /Type /Pages /Kids [{}] /Count {n} >>",
        kids.join(" ")
    ));
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding >>".into(),
    );
    let empty = Page::default();
    for i in 0..n {
        let page = pages.get(i).unwrap_or(&empty);
        let mut text = format!(
            "BT\n/F1 {} Tf\n{} TL\n{} {} Td\n",
            num(size),
            num(leading),
            num(MARGIN),
            num(height - MARGIN - leading * 0.8)
        );
        for line in 1..=page.last_line() {
            if let Some(l) = page.lines.get(&line) {
                let _ = writeln!(text, "{} Tj", literal(l));
            }
            text.push_str("T*\n");
        }
        text.push_str("ET\n");
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",
            num(width),
            num(height),
            5 + 2 * i
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{text}endstream",
            text.len()
        ));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        let _ = write!(out, "{} 0 obj\n{o}\nendobj\n", i + 1);
    }
    let xref = out.len();
    let _ = write!(out, "xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for off in offsets {
        let _ = writeln!(out, "{off:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    out.into_bytes()
}

/// Whether `pdf` is one this writer could have made: the header, the
/// trailer, and every cross-reference offset at its object; the
/// number of pages.
pub fn check(pdf: &[u8]) -> Result<usize, String> {
    let text = String::from_utf8_lossy(pdf);
    if !text.starts_with("%PDF-1.") || !text.trim_end().ends_with("%%EOF") {
        return Err("no PDF header or end marker".into());
    }
    let start: usize = text
        .rsplit("startxref\n")
        .next()
        .and_then(|t| t.lines().next())
        .and_then(|n| n.trim().parse().ok())
        .ok_or("no startxref")?;
    let xref = text.get(start..).ok_or("startxref past the end")?;
    if !xref.starts_with("xref\n") {
        return Err("startxref does not point at the xref table".into());
    }
    let mut objects = 0;
    for (i, line) in xref.lines().skip(3).enumerate() {
        let Some(off) = line
            .strip_suffix(" 00000 n ")
            .and_then(|o| o.parse::<usize>().ok())
        else {
            break;
        };
        let want = format!("{} 0 obj", i + 1);
        if text.get(off..off + want.len()) != Some(want.as_str()) {
            return Err(format!("object {} is not at offset {off}", i + 1));
        }
        objects += 1;
    }
    let pages = text.matches("/Type /Page ").count();
    if objects == 0 || pages == 0 {
        return Err("no objects or no pages".into());
    }
    Ok(pages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printer::{PageFormat, Printer};
    use crate::scs::decode;

    #[test]
    fn two_pages() {
        let mut p = Printer::new(PageFormat {
            len: 10,
            ..PageFormat::default()
        });
        for i in 1..=12 {
            p.write_line(&ccsid::to_ebcdic(&format!("LINE (A) {i}")).unwrap());
        }
        let (pages, fmt) = decode(&p.take());
        let pdf = render(&pages, fmt);
        assert_eq!(check(&pdf), Ok(2));
        let text = String::from_utf8(pdf.clone()).unwrap();
        assert!(text.contains("(LINE \\(A\\) 12) Tj"));
        assert!(text.contains("/MediaBox [0 0 986.4 156]"));
        assert_eq!(
            render(&pages, fmt),
            pdf,
            "the same pages make the same bytes"
        );
        let mut bad = pdf;
        bad.insert(20, b' ');
        assert!(check(&bad).is_err());
    }
}
