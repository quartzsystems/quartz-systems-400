// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Display-file diagnostics, `DDS6nnn`, in the scheme of the `dds`
//! crate: 61xx columns and conditioning, 62xx keywords, 63xx file
//! level, 64xx records, 65xx fields. Keyword syntax, duplicates,
//! conflicts and unknown keywords reuse the `DDS02xx` messages.

/// The messages, one function each.
pub mod ids {
    use dds::{Diagnostic, Severity};

    fn err(id: &'static str, line: usize, text: String) -> Diagnostic {
        Diagnostic::new(id, Severity::Error, Some(line), text)
    }

    fn warn(id: &'static str, line: usize, text: String) -> Diagnostic {
        Diagnostic::new(id, Severity::Warning, Some(line), text)
    }

    // Columns and conditioning.

    /// Positions 7-16 are malformed.
    pub fn bad_conditioning(line: usize, text: &str) -> Diagnostic {
        err(
            "DDS6101",
            line,
            format!(
                "Conditioning {text:?} is not valid; position 7 takes A, O or blank and positions 8-16 up to three indicators, each N or blank and 01-99"
            ),
        )
    }

    /// An indicator outside 01-99.
    pub fn indicator_range(line: usize, n: u8) -> Diagnostic {
        err(
            "DDS6102",
            line,
            format!("Indicator {n:02} is not valid; indicators are 01 to 99"),
        )
    }

    /// A `*DSn` that `DSPSIZ` does not define.
    pub fn unknown_size_condition(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6103",
            line,
            format!("Display size condition {name} is not defined by DSPSIZ"),
        )
    }

    /// Positions 39-44 are malformed.
    pub fn bad_location(line: usize, text: &str) -> Diagnostic {
        err(
            "DDS6104",
            line,
            format!("Location {text:?} must hold a row (39-41) and a column or +n (42-44)"),
        )
    }

    /// A displayed field with no location.
    pub fn location_missing(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6105",
            line,
            format!("Field {name} needs a row and column in positions 39-44"),
        )
    }

    /// A hidden or program-to-system field with a location.
    pub fn location_not_allowed(line: usize, name: &str, usage: char) -> Diagnostic {
        err(
            "DDS6106",
            line,
            format!("Field {name} has usage {usage} and takes no location"),
        )
    }

    /// `+n` with nothing before it on the row.
    pub fn relative_without_previous(line: usize) -> Diagnostic {
        err(
            "DDS6107",
            line,
            "A +n column needs a field before it on the same row".to_string(),
        )
    }

    /// A `*DSn` location line with no field before it.
    pub fn alt_location_without_field(line: usize) -> Diagnostic {
        err(
            "DDS6108",
            line,
            "A display size location line must follow the field it places".to_string(),
        )
    }

    /// `A` or `O` in position 7 on the first line of a condition.
    pub fn continuation_without_start(line: usize) -> Diagnostic {
        err(
            "DDS6109",
            line,
            "A or O in position 7 continues a condition, but no condition line precedes it"
                .to_string(),
        )
    }

    // Keywords.

    /// A keyword this compiler refuses.
    pub fn rejected(line: usize, kw: &str, why: &str) -> Diagnostic {
        err(
            "DDS6202",
            line,
            format!("Keyword {kw} is not supported: {why}"),
        )
    }

    /// `CAnn`/`CFnn` outside 01-24.
    pub fn cmd_key_range(line: usize, kw: &str) -> Diagnostic {
        err(
            "DDS6205",
            line,
            format!("Command key {kw} is not valid; keys are CA01-CA24 and CF01-CF24"),
        )
    }

    /// A response indicator where the keyword takes none, or none where
    /// one is needed.
    pub fn response_indicator(line: usize, kw: &str, why: &str) -> Diagnostic {
        err(
            "DDS6206",
            line,
            format!("Keyword {kw}: response indicator {why}"),
        )
    }

    /// Option indicators on a keyword that takes none.
    pub fn no_option_indicators(line: usize, kw: &str) -> Diagnostic {
        err(
            "DDS6207",
            line,
            format!("Keyword {kw} cannot be conditioned by option indicators"),
        )
    }

    /// `CAnn` and `CFnn` with the same number.
    pub fn same_key_twice(line: usize, kw: &str, other: &str) -> Diagnostic {
        err(
            "DDS6208",
            line,
            format!("{kw} and {other} name the same key; use one of them"),
        )
    }

    /// A keyword that needs another one present.
    pub fn requires(line: usize, kw: &str, what: &str) -> Diagnostic {
        err("DDS6209", line, format!("Keyword {kw} requires {what}"))
    }

    /// A keyword this compiler accepts and stores without acting on.
    pub fn stored_only(line: usize, kw: &str) -> Diagnostic {
        Diagnostic::new(
            "DDS6210",
            Severity::Info,
            Some(line),
            format!("Keyword {kw} is kept with the file and has no effect yet"),
        )
    }

    // File level.

    /// `DSPSIZ` parameters are wrong.
    pub fn bad_dspsiz(line: usize, why: &str) -> Diagnostic {
        err("DDS6301", line, format!("DSPSIZ is not valid: {why}"))
    }

    /// `MSGLOC` outside the display.
    pub fn bad_msgloc(line: usize, n: u32, rows: u8) -> Diagnostic {
        err(
            "DDS6303",
            line,
            format!("MSGLOC line {n} is outside the display of {rows} lines"),
        )
    }

    /// No `R` line at all.
    pub fn no_record_format() -> Diagnostic {
        Diagnostic::new(
            "DDS6305",
            Severity::Severe,
            None,
            "A display file needs at least one record format",
        )
    }

    /// A record format name used twice.
    pub fn duplicate_record(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6306",
            line,
            format!("Record format {name} is already defined"),
        )
    }

    /// A specification type a display file has no use for.
    pub fn spec_not_in_dspf(line: usize, what: &str) -> Diagnostic {
        err(
            "DDS6307",
            line,
            format!("A {what} specification is not valid in a display file"),
        )
    }

    // Records.

    /// A subfile record without its control record.
    pub fn sfl_without_ctl(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6401",
            line,
            format!("Subfile record {name} must be followed by a record with SFLCTL({name})"),
        )
    }

    /// `SFLCTL` naming something other than the subfile record before it.
    pub fn sflctl_target(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6402",
            line,
            format!("SFLCTL({name}) must name the SFL record immediately before this one"),
        )
    }

    /// `SFLSIZ` or `SFLPAG` missing.
    pub fn sfl_size_missing(line: usize, kw: &str) -> Diagnostic {
        err(
            "DDS6403",
            line,
            format!("A subfile control record needs {kw}"),
        )
    }

    /// `SFLSIZ`/`SFLPAG` out of range or inconsistent.
    pub fn sfl_size(line: usize, why: &str) -> Diagnostic {
        err("DDS6404", line, format!("Subfile size is not valid: {why}"))
    }

    /// The subfile page runs past the display.
    pub fn sfl_page_fit(line: usize, name: &str, last: u32, rows: u8, size: &str) -> Diagnostic {
        err(
            "DDS6405",
            line,
            format!(
                "Subfile {name}: the page ends on line {last}, past the {rows} lines of {size}"
            ),
        )
    }

    /// A subfile keyword on the wrong record.
    pub fn sfl_keyword_site(line: usize, kw: &str, where_: &str) -> Diagnostic {
        err(
            "DDS6406",
            line,
            format!("Keyword {kw} belongs on the {where_} record"),
        )
    }

    /// `WINDOW` parameters are wrong.
    pub fn bad_window(line: usize, why: &str) -> Diagnostic {
        err("DDS6407", line, format!("WINDOW is not valid: {why}"))
    }

    /// The window runs past the display.
    pub fn window_fit(line: usize, name: &str, size: &str) -> Diagnostic {
        err(
            "DDS6408",
            line,
            format!("Window {name} with its border and attributes does not fit the display {size}"),
        )
    }

    /// A record named by `WINDOW`, `MNUBARDSP` or the like that is not there.
    pub fn record_not_found(line: usize, kw: &str, name: &str, what: &str) -> Diagnostic {
        err(
            "DDS6409",
            line,
            format!("Keyword {kw}: {name} is not a {what} of this file"),
        )
    }

    /// A field outside its window.
    pub fn outside_window(line: usize, name: &str, lines: u8, cols: u8) -> Diagnostic {
        err(
            "DDS6410",
            line,
            format!("Field {name} lies outside the window of {lines} lines by {cols} columns"),
        )
    }

    /// A field named by a record keyword that the record does not have.
    pub fn field_not_found(line: usize, kw: &str, name: &str) -> Diagnostic {
        err(
            "DDS6411",
            line,
            format!("Keyword {kw}: field {name} is not in this record format"),
        )
    }

    /// A field named by a keyword with the wrong shape.
    pub fn field_shape(line: usize, kw: &str, name: &str, want: &str) -> Diagnostic {
        err(
            "DDS6412",
            line,
            format!("Keyword {kw}: field {name} must be {want}"),
        )
    }

    /// A record format with no fields.
    pub fn empty_record(line: usize, name: &str) -> Diagnostic {
        warn(
            "DDS6414",
            line,
            format!("Record format {name} has no fields"),
        )
    }

    /// A keyword a subfile record cannot carry.
    pub fn not_in_subfile(line: usize, kw: &str) -> Diagnostic {
        err(
            "DDS6415",
            line,
            format!("Keyword {kw} is not allowed on a subfile record"),
        )
    }

    /// Control record fields on the subfile's lines.
    pub fn ctl_in_page(line: usize, name: &str, row: u8) -> Diagnostic {
        err(
            "DDS6416",
            line,
            format!("Field {name} on line {row} lies within the subfile page"),
        )
    }

    /// A menu bar or pull-down record that is malformed.
    pub fn menu(line: usize, why: &str) -> Diagnostic {
        err("DDS6417", line, format!("Menu bar is not valid: {why}"))
    }

    // Fields.

    /// Position 35 is not a display data type.
    pub fn bad_data_type(line: usize, ch: char) -> Diagnostic {
        err(
            "DDS6501",
            line,
            format!("Data type {ch:?} is not valid in a display file"),
        )
    }

    /// Position 38 is not a display usage.
    pub fn bad_usage(line: usize, ch: char) -> Diagnostic {
        err(
            "DDS6502",
            line,
            format!("Usage {ch:?} is not valid; use blank, O, I, B, H, M or P"),
        )
    }

    /// Length missing where nothing supplies it.
    pub fn length_required(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6503",
            line,
            format!("Field {name}: length is required in positions 30-34"),
        )
    }

    /// Length outside the type's range.
    pub fn bad_length(line: usize, name: &str, len: u32, ty: char, max: u32) -> Diagnostic {
        err(
            "DDS6504",
            line,
            format!("Field {name}: length {len} is not valid for type {ty}; allowed 1 to {max}"),
        )
    }

    /// Decimals on a type without them, or past the length.
    pub fn bad_decimals(line: usize, name: &str, why: &str) -> Diagnostic {
        err(
            "DDS6505",
            line,
            format!("Field {name}: decimal positions {why}"),
        )
    }

    /// A constant with a name or attributes.
    pub fn constant_attributes(line: usize) -> Diagnostic {
        err(
            "DDS6506",
            line,
            "A constant field takes no name, length, type, decimals or usage".to_string(),
        )
    }

    /// A keyword the field's usage does not allow.
    pub fn keyword_usage(line: usize, kw: &str, name: &str, want: &str) -> Diagnostic {
        err(
            "DDS6507",
            line,
            format!("Keyword {kw} on field {name} needs usage {want}"),
        )
    }

    /// Two fields share screen positions.
    pub fn overlap(line: usize, name: &str, other: &str, size: &str) -> Diagnostic {
        err(
            "DDS6508",
            line,
            format!(
                "Field {name} overlaps field {other} on {size}; a field needs the position before it for its attribute"
            ),
        )
    }

    /// A position off the display.
    pub fn off_display(line: usize, name: &str, why: &str, size: &str) -> Diagnostic {
        err("DDS6509", line, format!("Field {name}: {why} on {size}"))
    }

    /// A field with no name that is not a constant.
    pub fn unnamed_field(line: usize) -> Diagnostic {
        err(
            "DDS6510",
            line,
            "A field needs a name in positions 19-28 unless it is a constant".to_string(),
        )
    }

    /// `REFFLD` or `R` names a field nobody has.
    pub fn ref_not_found(line: usize, field: &str) -> Diagnostic {
        err(
            "DDS6512",
            line,
            format!("Referenced field {field} was not found"),
        )
    }

    /// A keyword value outside its list.
    pub fn bad_value(line: usize, kw: &str, value: &str, allowed: &str) -> Diagnostic {
        err(
            "DDS6513",
            line,
            format!("Keyword {kw}: {value} is not valid; use {allowed}"),
        )
    }

    /// A keyword the field's type does not allow.
    pub fn keyword_type(line: usize, kw: &str, name: &str, ty: char) -> Diagnostic {
        err(
            "DDS6514",
            line,
            format!("Keyword {kw} on field {name} is not valid for type {ty}"),
        )
    }

    /// A keyword on a field that is not displayed.
    pub fn needs_display(line: usize, kw: &str, name: &str) -> Diagnostic {
        err(
            "DDS6515",
            line,
            format!("Keyword {kw} on field {name} needs a displayed field"),
        )
    }

    /// `DFT` longer than the field.
    pub fn dft_length(line: usize, name: &str, len: usize, field: u32) -> Diagnostic {
        err(
            "DDS6516",
            line,
            format!(
                "Field {name}: the DFT value of {len} characters is longer than the field ({field})"
            ),
        )
    }

    /// The same field name twice in a record.
    pub fn duplicate_field(line: usize, name: &str) -> Diagnostic {
        err(
            "DDS6517",
            line,
            format!("Field {name} is already defined in this record format"),
        )
    }

    /// A field longer than the display.
    pub fn too_long(line: usize, name: &str, len: u32, size: &str) -> Diagnostic {
        err(
            "DDS6518",
            line,
            format!("Field {name} of {len} positions does not fit the display {size}"),
        )
    }

    /// A subfile control record with lines both above and below its
    /// subfile's page: a record holds the lines from its first to its
    /// last.
    pub fn ctl_spans_page(line: usize, name: &str, first: u32, last: u32) -> Diagnostic {
        err(
            "DDS6520",
            line,
            format!("Record {name} has fields above and below its subfile's lines {first}-{last}; its lines overlap them"),
        )
    }

    /// A keyword that is only taken under an option indicator.
    pub fn needs_indicator(line: usize, kw: &str) -> Diagnostic {
        err(
            "DDS6519",
            line,
            format!("Keyword {kw} needs an option indicator"),
        )
    }
}
