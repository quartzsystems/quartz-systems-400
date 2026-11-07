// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `.qdx` request files, as QS/400 Access for Desktop writes and reads
//! them (its `qsa-transfer` request format, version 2): one file runs in
//! either client. A table download names its source and file, an upload
//! its file, target table, mode and mapping. A request never holds a
//! password. Files requests (`type = "files"`) belong to the IFS pages
//! and are refused here.
//!
//! ```toml
//! [qsaccess]
//! kind = "transfer"
//! version = 2
//!
//! [transfer]
//! type = "table"
//! direction = "download"
//! system = "QSHQ400"
//! file = "assets.csv"
//!
//! [transfer.source]
//! kind = "table"
//! schema = "HLINV"
//! table = "ASSETS"
//! ```

use serde::{Deserialize, Serialize};

/// The `[qsaccess]` kind of a request file.
pub const KIND: &str = "transfer";
/// The version written, and the newest read.
pub const VERSION: u32 = 2;

/// Which way a request goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    /// From the system to a file.
    #[default]
    Download,
    /// From a file to the system.
    Upload,
}

/// What a request moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Type {
    /// A table's rows.
    #[default]
    Table,
    /// Integrated file system files.
    Files,
}

/// A file's format, as the desktop names it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    /// CSV.
    #[default]
    Csv,
    /// TSV.
    Tsv,
    /// XLSX.
    Xlsx,
    /// ODS.
    Ods,
    /// Fixed width.
    Fixed,
}

impl Format {
    /// The format of the file pages.
    pub fn file(self) -> super::files::Format {
        use super::files::Format as F;
        match self {
            Format::Csv => F::Csv,
            Format::Tsv => F::Tsv,
            Format::Xlsx => F::Xlsx,
            Format::Ods => F::Ods,
            Format::Fixed => F::Fixed,
        }
    }

    /// The `.qdx` format of a file page's format (transfer formats only).
    pub fn of(f: super::files::Format) -> Option<Format> {
        use super::files::Format as F;
        Some(match f {
            F::Csv => Format::Csv,
            F::Tsv => Format::Tsv,
            F::Xlsx => Format::Xlsx,
            F::Ods => Format::Ods,
            F::Fixed => Format::Fixed,
            _ => return None,
        })
    }
}

/// A text file's encoding.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Encoding {
    /// UTF-8.
    #[default]
    Utf8,
    /// UTF-8 with a byte order mark.
    Utf8Bom,
    /// UTF-16 (read only).
    Utf16,
    /// Windows-1252 (read only).
    Windows1252,
}

/// Dates as ISO or the job's format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dates {
    /// ISO 8601.
    #[default]
    Iso,
    /// The job's format.
    Job,
}

/// A transfer file's options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FileOptions {
    /// The format.
    pub format: Format,
    /// CSV's delimiter.
    pub delimiter: char,
    /// The quote of CSV and TSV.
    pub quote: char,
    /// A header row of column names.
    pub header: bool,
    /// The text encoding.
    pub encoding: Encoding,
    /// A spreadsheet's sheet (empty: the first).
    pub sheet: String,
    /// Fixed width: each column's width.
    pub widths: Vec<usize>,
    /// Fixed width: a null indicator after each nullable column.
    pub null_indicators: bool,
    /// How dates are written and read.
    pub dates: Dates,
    /// The decimal point.
    pub point: char,
    /// CCSID 65535 data as text rather than hex.
    pub bit_data_as_text: bool,
}

impl Default for FileOptions {
    fn default() -> Self {
        Self {
            format: Format::Csv,
            delimiter: ',',
            quote: '"',
            header: true,
            encoding: Encoding::Utf8,
            sheet: String::new(),
            widths: Vec::new(),
            null_indicators: true,
            dates: Dates::Iso,
            point: '.',
            bit_data_as_text: false,
        }
    }
}

/// Where a download's rows come from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Source {
    /// A table, view, alias or file.
    Table {
        /// Its schema.
        schema: String,
        /// Its name.
        table: String,
        /// A `WHERE` clause without the word.
        #[serde(default, rename = "where", skip_serializing_if = "String::is_empty")]
        where_clause: String,
    },
    /// A query.
    Query {
        /// Its text.
        sql: String,
    },
}

/// Append to the table, or replace its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Add the rows.
    #[default]
    Append,
    /// Delete the table's rows first.
    Replace,
}

/// How an upload commits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "commit", rename_all = "kebab-case")]
pub enum Commit {
    /// One transaction, committed at the end.
    #[default]
    Once,
    /// A commit every `batches` batches.
    Every {
        /// Batches between commits.
        batches: u32,
    },
    /// No commitment control.
    Without,
}

/// Where a target column's values come from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SourceColumn {
    /// The source column of this header name.
    Name(String),
    /// The source column at this position, from 0.
    Position(usize),
}

/// One target column and its source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Map {
    /// The target column.
    pub target: String,
    /// Its source.
    pub from: SourceColumn,
}

#[derive(Serialize, Deserialize)]
struct Header {
    kind: String,
    version: u32,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Transfer {
    #[serde(rename = "type")]
    kind: Type,
    direction: Direction,
    system: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<FileOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    columns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    existing: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    schema: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    table: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    mode: Option<Mode>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    mapping: Vec<Map>,
    #[serde(skip_serializing_if = "Option::is_none")]
    commit: Option<Commit>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resume_row: Option<u64>,
    // A files request's fields, read only to refuse it.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    local: Vec<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    remote: String,
}

#[derive(Serialize, Deserialize)]
struct File {
    qsaccess: Header,
    transfer: Transfer,
}

/// A table download.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Download {
    /// The rows' source.
    pub source: Source,
    /// The columns, in order; empty for all.
    pub columns: Vec<String>,
    /// At most this many rows.
    pub limit: Option<u64>,
    /// The file written (a name on the web).
    pub file: String,
    /// Its options.
    pub options: FileOptions,
}

/// A table upload.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Upload {
    /// The file read.
    pub file: String,
    /// Its options.
    pub options: FileOptions,
    /// The target's schema.
    pub schema: String,
    /// The target table.
    pub table: String,
    /// Append or replace.
    pub mode: Mode,
    /// The mapping; empty maps by name with a header row, else by position.
    pub mapping: Vec<Map>,
    /// How it commits.
    pub commit: Commit,
    /// The first data row to send, from 1 (a resume).
    pub resume_row: Option<u64>,
}

/// A request: the system, and the transfer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// A download.
    Download {
        /// The system.
        system: String,
        /// The transfer.
        download: Download,
    },
    /// An upload.
    Upload {
        /// The system.
        system: String,
        /// The transfer.
        upload: Upload,
    },
}

/// Why a request file could not be read.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Not TOML, or not a request's.
    #[error("not a request file: {0}")]
    Parse(String),
    /// Another kind of file.
    #[error("not a transfer request (kind {0})")]
    Kind(String),
    /// A version this program does not read.
    #[error("request file version {0} is newer than this program reads")]
    Newer(u32),
    /// A field it must have.
    #[error("the request names no {0}")]
    Missing(&'static str),
    /// A files request.
    #[error("a files request is run from the Files pages")]
    Files,
}

/// `r` as the text of a `.qdx` file.
pub fn to_text(r: &Request) -> String {
    let t = match r {
        Request::Download { system, download: d } => Transfer {
            kind: Type::Table,
            direction: Direction::Download,
            system: system.clone(),
            file: d.file.clone(),
            options: Some(d.options.clone()),
            source: Some(d.source.clone()),
            columns: d.columns.clone(),
            limit: d.limit,
            existing: "replace".into(),
            mode: Some(Mode::default()),
            commit: Some(Commit::default()),
            ..Transfer::default()
        },
        Request::Upload { system, upload: u } => Transfer {
            kind: Type::Table,
            direction: Direction::Upload,
            system: system.clone(),
            file: u.file.clone(),
            options: Some(u.options.clone()),
            existing: "ask".into(),
            schema: u.schema.clone(),
            table: u.table.clone(),
            mode: Some(u.mode),
            mapping: u.mapping.clone(),
            commit: Some(u.commit),
            resume_row: u.resume_row,
            ..Transfer::default()
        },
    };
    let f = File {
        qsaccess: Header {
            kind: KIND.into(),
            version: VERSION,
        },
        transfer: t,
    };
    toml::to_string(&f).unwrap_or_default()
}

/// A request from a `.qdx` file's text.
pub fn from_text(text: &str) -> Result<Request, Error> {
    let f: File = toml::from_str(text).map_err(|e| Error::Parse(e.message().to_owned()))?;
    if f.qsaccess.kind != KIND {
        return Err(Error::Kind(f.qsaccess.kind));
    }
    if f.qsaccess.version > VERSION {
        return Err(Error::Newer(f.qsaccess.version));
    }
    let t = f.transfer;
    if t.system.trim().is_empty() {
        return Err(Error::Missing("system"));
    }
    // Version 1 knew no other type.
    if f.qsaccess.version >= 2 && t.kind == Type::Files {
        return Err(Error::Files);
    }
    let options = t.options.unwrap_or_default();
    Ok(match t.direction {
        Direction::Download => Request::Download {
            system: t.system,
            download: Download {
                source: t.source.ok_or(Error::Missing("source"))?,
                columns: t.columns,
                limit: t.limit,
                file: t.file,
                options,
            },
        },
        Direction::Upload => {
            if t.schema.is_empty() || t.table.is_empty() {
                return Err(Error::Missing("target table"));
            }
            Request::Upload {
                system: t.system,
                upload: Upload {
                    file: t.file,
                    options,
                    schema: t.schema,
                    table: t.table,
                    mode: t.mode.unwrap_or_default(),
                    mapping: t.mapping,
                    commit: t.commit.unwrap_or_default(),
                    resume_row: t.resume_row,
                },
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request as the desktop's own test writes it.
    const DESKTOP: &str = "[qsaccess]\nkind = \"transfer\"\nversion = 2\n\n[transfer]\ntype = \"table\"\ndirection = \"download\"\nsystem = \"qs400.example.net\"\nfile = \"out/assets.tsv\"\ncolumns = [\"ASSET_TAG\", \"COST\"]\nlimit = 1000\nexisting = \"replace\"\n\n[transfer.options]\nformat = \"tsv\"\ndelimiter = \",\"\nquote = '\"'\nheader = true\nencoding = \"utf8\"\nsheet = \"\"\nwidths = []\nnull_indicators = true\ndates = \"job\"\npoint = \",\"\nbit_data_as_text = false\n\n[transfer.source]\nkind = \"table\"\nschema = \"HLINV\"\ntable = \"ASSETS\"\nwhere = \"STATUS = 'ACTIVE'\"\n\n[transfer.commit]\ncommit = \"once\"\n";

    #[test]
    fn reads_the_desktops_and_round_trips() {
        let Request::Download { system, download } = from_text(DESKTOP).unwrap() else { panic!("a download") };
        assert_eq!(system, "qs400.example.net");
        assert_eq!(download.options.format, Format::Tsv);
        assert_eq!(download.options.dates, Dates::Job);
        assert!(matches!(&download.source, Source::Table { where_clause, .. } if where_clause == "STATUS = 'ACTIVE'"));
        let r = Request::Upload {
            system: "QSHQ400".into(),
            upload: Upload {
                file: "assets_br1.csv".into(),
                schema: "HLINV".into(),
                table: "ASSETS".into(),
                mapping: vec![Map { target: "ASSET_TAG".into(), from: SourceColumn::Name("tag".into()) }],
                ..Upload::default()
            },
        };
        let text = to_text(&r);
        assert!(text.starts_with("[qsaccess]\nkind = \"transfer\"\nversion = 2\n"), "{text}");
        assert!(!text.to_ascii_lowercase().contains("password"));
        assert_eq!(from_text(&text).unwrap(), r);
    }

    #[test]
    fn what_is_refused() {
        assert!(matches!(from_text("x = ["), Err(Error::Parse(_))));
        assert_eq!(
            from_text("[qsaccess]\nkind = \"display\"\nversion = 1\n[transfer]\n").unwrap_err(),
            Error::Kind("display".into())
        );
        assert_eq!(from_text("[qsaccess]\nkind = \"transfer\"\nversion = 3\n[transfer]\n").unwrap_err(), Error::Newer(3));
        let files = "[qsaccess]\nkind = \"transfer\"\nversion = 2\n[transfer]\ntype = \"files\"\nsystem = \"S\"\nlocal = [\"a\"]\nremote = \"/home\"\n";
        assert_eq!(from_text(files).unwrap_err(), Error::Files);
    }
}
