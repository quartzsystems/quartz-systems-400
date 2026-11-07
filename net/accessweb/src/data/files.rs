// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Transfer and download files: the formats, their options, and rows
//! written to and read from them.

use super::{sheets, text};
use crate::env::db::{ResultColumn, Val};

/// A file format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Format {
    /// Comma-separated values.
    #[default]
    Csv,
    /// Tab-separated values.
    Tsv,
    /// Office Open XML spreadsheet.
    Xlsx,
    /// `OpenDocument` spreadsheet.
    Ods,
    /// Fixed-width columns.
    Fixed,
    /// A page of its own (download only).
    Html,
    /// PDF (download only).
    Pdf,
    /// The Excel-compatible XML spreadsheet (download only).
    Xls,
}

impl Format {
    /// Transfer formats, both ways.
    pub const TRANSFER: [Format; 5] = [
        Format::Csv,
        Format::Tsv,
        Format::Xlsx,
        Format::Ods,
        Format::Fixed,
    ];
    /// Run SQL's download formats.
    pub const DOWNLOAD: [Format; 7] = [
        Format::Csv,
        Format::Tsv,
        Format::Xlsx,
        Format::Ods,
        Format::Fixed,
        Format::Html,
        Format::Pdf,
    ];

    /// Its name in the selects.
    pub fn label(self) -> &'static str {
        match self {
            Format::Csv => "CSV",
            Format::Tsv => "TSV",
            Format::Xlsx => "XLSX",
            Format::Ods => "ODS",
            Format::Fixed => "Fixed width",
            Format::Html => "HTML",
            Format::Pdf => "PDF",
            Format::Xls => "Excel-compatible",
        }
    }

    /// The format a select's choice names (`XLSX (first sheet)`,
    /// `Excel-compatible (.xls)` included).
    pub fn of(label: &str) -> Option<Format> {
        let l = label.trim();
        let l = l.split(" (").next().unwrap_or(l);
        [
            Format::Csv,
            Format::Tsv,
            Format::Xlsx,
            Format::Ods,
            Format::Fixed,
            Format::Html,
            Format::Pdf,
            Format::Xls,
        ]
        .into_iter()
        .find(|f| f.label().eq_ignore_ascii_case(l))
    }

    /// The desktop's `.qdx` name (`csv`, `fixed`).
    pub fn qdx(self) -> &'static str {
        match self {
            Format::Csv => "csv",
            Format::Tsv => "tsv",
            Format::Xlsx => "xlsx",
            Format::Ods => "ods",
            Format::Fixed => "fixed",
            Format::Html => "html",
            Format::Pdf => "pdf",
            Format::Xls => "xls",
        }
    }

    /// The format of a `.qdx` name.
    pub fn of_qdx(s: &str) -> Option<Format> {
        Self::TRANSFER.into_iter().find(|f| f.qdx() == s)
    }

    /// The file extension.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Fixed => "txt",
            f => f.qdx(),
        }
    }

    /// The format a file name's extension says.
    pub fn of_name(name: &str) -> Option<Format> {
        let ext = name.rsplit_once('.')?.1.to_ascii_lowercase();
        Self::TRANSFER.into_iter().find(|f| f.extension() == ext)
    }

    /// The content type it is downloaded as.
    pub fn content_type(self) -> &'static str {
        match self {
            Format::Csv => "text/csv; charset=utf-8",
            Format::Tsv => "text/tab-separated-values; charset=utf-8",
            Format::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            Format::Ods => "application/vnd.oasis.opendocument.spreadsheet",
            Format::Fixed => "text/plain; charset=utf-8",
            Format::Html => "text/html; charset=utf-8",
            Format::Pdf => "application/pdf",
            Format::Xls => "application/vnd.ms-excel",
        }
    }
}

/// A text file's code page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Encoding {
    /// UTF-8 (CCSID 1208).
    #[default]
    Utf8,
    /// Windows-1252.
    Cp1252,
    /// EBCDIC CCSID 37.
    Cp37,
}

impl Encoding {
    /// The encoding a select's choice names (`1208 (UTF-8)`, `UTF-8
    /// (1208)`, `1252`, `37`).
    pub fn of(label: &str) -> Option<Encoding> {
        let l = label.to_ascii_uppercase();
        if l.contains("1208") || l.contains("UTF-8") {
            Some(Encoding::Utf8)
        } else if l.contains("1252") {
            Some(Encoding::Cp1252)
        } else if l.trim() == "37" {
            Some(Encoding::Cp37)
        } else {
            None
        }
    }
}

/// How a file is written.
#[derive(Clone, Debug)]
pub struct Write {
    /// The column names in the first row.
    pub header: bool,
    /// Lines end in CR LF (else LF).
    pub crlf: bool,
    /// A text file's code page.
    pub encoding: Encoding,
    /// Dates as ISO (else `MM/DD/YY`).
    pub iso_dates: bool,
    /// The sheet's or document's name.
    pub title: String,
}

impl Default for Write {
    fn default() -> Self {
        Self {
            header: true,
            crlf: true,
            encoding: Encoding::Utf8,
            iso_dates: true,
            title: String::new(),
        }
    }
}

/// `2025-03-11` as `03/11/25`.
fn job_date(s: &str) -> Option<String> {
    let b = s.as_bytes();
    if b.len() == 10 && b[4] == b'-' && b[7] == b'-' {
        return Some(format!("{}/{}/{}", &s[5..7], &s[8..10], &s[2..4]));
    }
    None
}

/// `rows` of `cols` as a file.
pub fn write(fmt: Format, cols: &[ResultColumn], rows: &[Vec<Val>], o: &Write) -> Vec<u8> {
    let names: Vec<String> = cols.iter().map(|c| c.name.clone()).collect();
    let numeric: Vec<bool> = cols.iter().map(ResultColumn::numeric).collect();
    let dated;
    let rows = if o.iso_dates {
        rows
    } else {
        dated = rows
            .iter()
            .map(|r| {
                r.iter()
                    .zip(cols)
                    .map(|(v, c)| match v {
                        Val::Text(t) if c.typ == "DATE" => {
                            Val::Text(job_date(t).unwrap_or_else(|| t.clone()))
                        }
                        v => v.clone(),
                    })
                    .collect()
            })
            .collect::<Vec<Vec<Val>>>();
        &dated
    };
    let header = o.header.then_some(names.as_slice());
    let eol = if o.crlf { "\r\n" } else { "\n" };
    let sheet = if o.title.is_empty() {
        "Sheet1"
    } else {
        o.title.as_str()
    };
    match fmt {
        Format::Csv => text::encode(&text::delimited(header, rows, ',', eol), o.encoding),
        Format::Tsv => text::encode(&text::delimited(header, rows, '\t', eol), o.encoding),
        Format::Fixed => text::encode(&text::fixed(header, &numeric, rows, eol), o.encoding),
        Format::Xlsx => sheets::xlsx(header, rows, sheet),
        Format::Ods => sheets::ods(header, rows, sheet),
        Format::Xls => sheets::xls(&names, rows, sheet),
        Format::Html => sheets::html(sheet, &names, &numeric, rows),
        Format::Pdf => sheets::pdf(&text::fixed(Some(&names), &numeric, rows, "\n")),
    }
}

/// What a transfer file holds: the column names (the first row's, or
/// `COL1`... without one) and the rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sheet {
    /// Column names.
    pub names: Vec<String>,
    /// The rows; a row may be shorter than the names.
    pub rows: Vec<Vec<Val>>,
}

/// A transfer file's rows; `header` says the first row is column names.
pub fn read(fmt: Format, b: &[u8], header: bool, enc: Encoding) -> Result<Sheet, String> {
    let mut rows = match fmt {
        Format::Csv => text::parse_delimited(&text::decode(b, enc)?, ',')?,
        Format::Tsv => text::parse_delimited(&text::decode(b, enc)?, '\t')?,
        Format::Xlsx => sheets::read_xlsx(b)?,
        Format::Ods => sheets::read_ods(b)?,
        Format::Fixed => {
            if !header {
                return Err("a fixed-width file needs the column names in its first row to show where the columns start".into());
            }
            let (names, rows) = text::parse_fixed(&text::decode(b, enc)?)?;
            return Ok(Sheet { names, rows });
        }
        other => {
            return Err(format!(
                "{} files cannot be copied to a table",
                other.label()
            ));
        }
    };
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let names = if header && !rows.is_empty() {
        let first = rows.remove(0);
        (0..width)
            .map(|i| {
                first
                    .get(i)
                    .map(Val::text)
                    .unwrap_or_default()
                    .trim()
                    .to_string()
            })
            .collect()
    } else {
        (1..=width).map(|i| format!("COL{i}")).collect()
    };
    Ok(Sheet { names, rows })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_transfer_format_both_ways() {
        let cols = vec![
            ResultColumn {
                name: "ASSET_TAG".into(),
                typ: "CHAR".into(),
            },
            ResultColumn {
                name: "COST".into(),
                typ: "DECIMAL".into(),
            },
            ResultColumn {
                name: "ACQUIRED".into(),
                typ: "DATE".into(),
            },
        ];
        let rows = vec![vec![
            Val::Text("QS000404".into()),
            Val::Num("1200.00".into()),
            Val::Text("2023-05-02".into()),
        ]];
        for f in Format::TRANSFER {
            let b = write(f, &cols, &rows, &Write::default());
            let s = read(f, &b, true, Encoding::Utf8).unwrap();
            assert_eq!(s.names, ["ASSET_TAG", "COST", "ACQUIRED"], "{f:?}");
            assert_eq!(s.rows[0][1].text(), "1200.00", "{f:?}");
            assert_eq!(s.rows[0][2].text(), "2023-05-02", "{f:?}");
        }
        let job = write(
            Format::Csv,
            &cols,
            &rows,
            &Write {
                iso_dates: false,
                header: false,
                crlf: false,
                ..Write::default()
            },
        );
        assert_eq!(job, b"QS000404,1200.00,05/02/23\n");
        assert_eq!(Format::of("XLSX (first sheet)"), Some(Format::Xlsx));
        assert_eq!(Format::of("Excel-compatible (.xls)"), Some(Format::Xls));
        assert_eq!(Encoding::of("1208 (UTF-8)"), Some(Encoding::Utf8));
        assert_eq!(
            read(Format::Csv, b"a,b\n1,2\n", false, Encoding::Utf8)
                .unwrap()
                .names,
            ["COL1", "COL2"]
        );
    }
}
