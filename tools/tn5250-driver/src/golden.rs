// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Golden screens on disk: `NAME.txt` holds the masked character
//! grid, `NAME.attr` the attribute grid, and `INDEX.toml` records
//! where each came from.

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result};

/// One index entry.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The golden's name.
    pub name: String,
    /// `oracle`, `pub400` or `qs400`.
    pub source: String,
    /// The release of the source.
    pub release: String,
    /// The script that captured it.
    pub script: String,
    /// The screen width.
    pub width: usize,
}

/// Write both grids.
pub fn write(dir: &Path, name: &str, rows: &[String], attrs: &[String]) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    let mut text = rows.join("\n");
    text.push('\n');
    std::fs::write(dir.join(format!("{name}.txt")), text)
        .with_context(|| format!("writing {name}.txt"))?;
    let mut a = attrs.join("\n");
    a.push('\n');
    std::fs::write(dir.join(format!("{name}.attr")), a)?;
    Ok(())
}

/// Read a grid file's rows, tolerant of CRLF.
pub fn read_rows(path: &Path) -> Result<Vec<String>> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .collect())
}

/// Compare with the stored golden; the differences as text, `None`
/// when equal. A missing golden is a difference. Without `attrs` only
/// the characters are compared.
pub fn compare(
    dir: &Path,
    name: &str,
    rows: &[String],
    attrs: Option<&[String]>,
    masks: &crate::mask::Masks,
) -> Result<Option<String>> {
    let path = dir.join(format!("{name}.txt"));
    if !path.is_file() {
        return Ok(Some(format!("    no golden {}", path.display())));
    }
    // The golden was masked when captured; masks added since apply to
    // it too, so both sides wear the same ones.
    let want = masks.apply(&read_rows(&path)?);
    let mut out = String::new();
    for (i, (w, g)) in want.iter().zip(rows.iter()).enumerate() {
        if w.trim_end() != g.trim_end() {
            let _ = writeln!(out, "    row {:>2} want |{}|", i + 1, w);
            let _ = writeln!(out, "    row {:>2} got  |{}|", i + 1, g);
        }
    }
    if want.len() != rows.len() {
        let _ = writeln!(
            out,
            "    {} rows in the golden, {} on the screen",
            want.len(),
            rows.len()
        );
    }
    let apath = dir.join(format!("{name}.attr"));
    if let Some(attrs) = attrs
        && apath.is_file()
    {
        let want = read_rows(&apath)?;
        for (i, (w, g)) in want.iter().zip(attrs.iter()).enumerate() {
            if w != g {
                let _ = writeln!(out, "    attr row {:>2} want |{}|", i + 1, w);
                let _ = writeln!(out, "    attr row {:>2} got  |{}|", i + 1, g);
            }
        }
    }
    Ok(if out.is_empty() { None } else { Some(out) })
}

/// Append entries to `INDEX.toml`, replacing any with the same name.
pub fn index_append(dir: &Path, entries: &[Entry]) -> Result<()> {
    let path = dir.join("INDEX.toml");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let date = today();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    let mut out = String::new();
    // Keep the blocks whose name is not being replaced.
    for block in existing.split("[[golden]]") {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let name = block
            .lines()
            .find_map(|l| {
                l.trim()
                    .strip_prefix("name = ")
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_default();
        if !names.contains(&name.as_str()) {
            let _ = writeln!(out, "[[golden]]\n{block}\n");
        }
    }
    for e in entries {
        let _ = writeln!(
            out,
            "[[golden]]\nname = \"{}\"\nsource = \"{}\"\nrelease = \"{}\"\ndate = \"{}\"\nscript = \"{}\"\nwidth = {}\n",
            e.name, e.source, e.release, date, e.script, e.width
        );
    }
    std::fs::write(&path, out)?;
    Ok(())
}

/// Today as `YYYY-MM-DD` from the system clock (UTC).
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = secs / 86400;
    // Civil-from-days (Howard Hinnant), proleptic Gregorian.
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_is_plausible() {
        let d = today();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"));
    }
}
