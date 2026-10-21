// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message the compiler sends, one function each. IDs: 01xx source
//! and columns, 02xx keyword syntax, 1xxx file semantics, 3xxx references.

use std::fmt::Display;

use super::{Diagnostic, Severity};

fn at(id: &'static str, sev: Severity, line: usize, text: String) -> Diagnostic {
    Diagnostic::new(id, sev, Some(line), text)
}

fn err(id: &'static str, line: usize, text: String) -> Diagnostic {
    at(id, Severity::Error, line, text)
}

fn warn(id: &'static str, line: usize, text: String) -> Diagnostic {
    at(id, Severity::Warning, line, text)
}

// Source and columns.

/// Position 6 holds something other than blank or `A`.
pub fn form_type(line: usize, ch: char) -> Diagnostic {
    err(
        "DDS0101",
        line,
        format!("Position 6 must be blank or A; found {ch:?}"),
    )
}

/// A continuation sign ends a line and no continuation line follows.
pub fn continuation_expected(line: usize) -> Diagnostic {
    err(
        "DDS0103",
        line,
        "Line ends with a continuation sign but no continuation line follows".to_string(),
    )
}

/// Positions 30-34 are not a number.
pub fn bad_length(line: usize, text: &str) -> Diagnostic {
    err(
        "DDS0105",
        line,
        format!("Length {text:?} in positions 30-34 is not numeric"),
    )
}

/// Positions 36-37 are not a number.
pub fn bad_decimals(line: usize, text: &str) -> Diagnostic {
    err(
        "DDS0106",
        line,
        format!("Decimal positions {text:?} in positions 36-37 are not numeric"),
    )
}

/// Position 17 is not `R`, `K`, `J`, `S`, `O` or blank.
pub fn bad_name_type(line: usize, ch: char) -> Diagnostic {
    err(
        "DDS0107",
        line,
        format!("Name type {ch:?} in position 17 is not valid; use R, K, J, S, O or blank"),
    )
}

/// Positions 19-28 do not hold a valid name.
pub fn bad_name(line: usize, name: &str, why: impl Display) -> Diagnostic {
    err(
        "DDS0108",
        line,
        format!("Name {name} in positions 19-28 is not valid: {why}"),
    )
}

/// Position 29 is not blank or `R`.
pub fn bad_reference_col(line: usize, ch: char) -> Diagnostic {
    err(
        "DDS0109",
        line,
        format!("Position 29 must be blank or R; found {ch:?}"),
    )
}

/// A character with no CP037 encoding in dev-side text.
pub fn unmappable(line: usize, col: usize, ch: char) -> Diagnostic {
    warn(
        "DDS0110",
        line,
        format!("Character {ch:?} in column {col} has no CP037 encoding; replaced by a blank"),
    )
}

/// A tab in dev-side text.
pub fn tab(line: usize, col: usize) -> Diagnostic {
    warn(
        "DDS0111",
        line,
        format!("Tab character in column {col} replaced by a blank; align columns with blanks"),
    )
}

/// Position 35 is not a database data type.
pub fn bad_data_type(line: usize, ch: char) -> Diagnostic {
    err(
        "DDS0112",
        line,
        format!("Data type {ch:?} in position 35 is not valid for a database file"),
    )
}

/// Positions 39-44 are used.
pub fn location_not_allowed(line: usize) -> Diagnostic {
    err(
        "DDS0113",
        line,
        "Positions 39-44 must be blank in a database file".to_string(),
    )
}

/// Positions 8-16 are used.
pub fn conditioning_ignored(line: usize) -> Diagnostic {
    warn(
        "DDS0114",
        line,
        "Conditioning in positions 8-16 is ignored in a database file".to_string(),
    )
}

/// An `R`, `K`, `J`, `S` or `O` line without a name.
pub fn name_missing(line: usize, what: &str) -> Diagnostic {
    err(
        "DDS0115",
        line,
        format!("A {what} specification needs a name in positions 19-28"),
    )
}

/// A field line without a name.
pub fn field_name_missing(line: usize) -> Diagnostic {
    err(
        "DDS0116",
        line,
        "Field specification without a name in positions 19-28".to_string(),
    )
}

/// Length, type, decimals or usage on a line that takes none.
pub fn attributes_not_allowed(line: usize, what: &str) -> Diagnostic {
    err(
        "DDS0117",
        line,
        format!("Positions 29-38 must be blank on a {what} specification"),
    )
}

// Keyword syntax.

/// Something other than a keyword name in the function columns.
pub fn keyword_expected(line: usize, ch: char) -> Diagnostic {
    err("DDS0201", line, format!("Keyword expected; found {ch:?}"))
}

/// A quoted string never closes.
pub fn unterminated_quote(line: usize, kw: &str) -> Diagnostic {
    err(
        "DDS0202",
        line,
        format!("Keyword {kw}: quoted string is not closed"),
    )
}

/// A parameter list never closes.
pub fn missing_paren(line: usize, kw: &str) -> Diagnostic {
    err(
        "DDS0203",
        line,
        format!("Keyword {kw}: right parenthesis missing"),
    )
}

/// A keyword is followed by something other than a blank.
pub fn unexpected_after(line: usize, kw: &str, ch: char) -> Diagnostic {
    err(
        "DDS0204",
        line,
        format!("Unexpected {ch:?} after keyword {kw}; a blank must follow"),
    )
}

/// Wrong number of parameters.
pub fn parm_count(line: usize, kw: &str, want: &str, got: usize) -> Diagnostic {
    err(
        "DDS0205",
        line,
        format!("Keyword {kw} takes {want} parameter(s); {got} given"),
    )
}

/// A parameter is malformed.
pub fn bad_parm(line: usize, kw: &str, parm: impl Display, why: impl Display) -> Diagnostic {
    err(
        "DDS0206",
        line,
        format!("Keyword {kw}: parameter {parm} is not valid: {why}"),
    )
}

/// A known keyword at the wrong level or in the wrong file type.
pub fn not_at_level(line: usize, kw: &str, level: &str, kind: &str) -> Diagnostic {
    err(
        "DDS0207",
        line,
        format!("Keyword {kw} is not allowed at the {level} level of a {kind} file"),
    )
}

/// The same keyword twice on one entry.
pub fn duplicate_keyword(line: usize, kw: &str) -> Diagnostic {
    err(
        "DDS0208",
        line,
        format!("Keyword {kw} is specified more than once"),
    )
}

/// Not a DDS keyword.
pub fn unknown_keyword(line: usize, kw: &str) -> Diagnostic {
    err("DDS0209", line, format!("Keyword {kw} is not known"))
}

/// Two keywords that exclude each other.
pub fn conflict(line: usize, kw: &str, other: &str) -> Diagnostic {
    err(
        "DDS0210",
        line,
        format!("Keyword {kw} cannot be used with {other}"),
    )
}

/// `X'...'` with bad digits.
pub fn bad_hex(line: usize, text: &str) -> Diagnostic {
    err(
        "DDS0211",
        line,
        format!("Hexadecimal literal X'{text}' is not valid"),
    )
}

// File semantics.

/// No `R` line at all.
pub fn no_record_format() -> Diagnostic {
    Diagnostic::new(
        "DDS1001",
        Severity::Severe,
        None,
        "No record format specification (R in position 17) in the source",
    )
}

/// A second `R` line in a physical file.
pub fn second_format(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1002",
        line,
        format!("A physical file has one record format; format {name} is not allowed"),
    )
}

/// A field name used twice in a format.
pub fn duplicate_field(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1003",
        line,
        format!("Field {name} is already defined in this record format"),
    )
}

/// No length where one is needed.
pub fn length_required(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1004",
        line,
        format!("Field {name}: length is required in positions 30-34"),
    )
}

/// Length outside the type's range.
pub fn bad_field_length(line: usize, name: &str, len: u32, ty: char, range: &str) -> Diagnostic {
    err(
        "DDS1005",
        line,
        format!("Field {name}: length {len} is not valid for type {ty}; allowed {range}"),
    )
}

/// Decimal positions wrong for the type or length.
pub fn bad_field_decimals(line: usize, name: &str, why: &str) -> Diagnostic {
    err(
        "DDS1006",
        line,
        format!("Field {name}: decimal positions {why}"),
    )
}

/// Usage other than blank or `B`.
pub fn bad_usage(line: usize, name: &str, ch: char) -> Diagnostic {
    err(
        "DDS1007",
        line,
        format!("Field {name}: usage {ch:?} is not valid in a database file; use blank or B"),
    )
}

/// A key names no field of the format.
pub fn key_not_in_format(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1008",
        line,
        format!("Key field {name} is not a field of the record format"),
    )
}

/// The same key field twice.
pub fn duplicate_key(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1009",
        line,
        format!("Key field {name} is specified more than once"),
    )
}

/// A key-level keyword the field's type does not take.
pub fn key_keyword_type(line: usize, kw: &str, name: &str, ty: char) -> Diagnostic {
    err(
        "DDS1010",
        line,
        format!("Keyword {kw} on key field {name} is not valid for type {ty}"),
    )
}

/// `UNIQUE`, `FIFO`, `LIFO` or `FCFO` without keys.
pub fn needs_keys(line: usize, kw: &str) -> Diagnostic {
    err("DDS1011", line, format!("Keyword {kw} requires key fields"))
}

/// `J`, `S` or `O` in a physical file.
pub fn spec_not_in_pf(line: usize, what: &str) -> Diagnostic {
    err(
        "DDS1013",
        line,
        format!("A {what} specification is not allowed in a physical file"),
    )
}

/// A field-level keyword the field's type does not take.
pub fn keyword_type(line: usize, kw: &str, name: &str, ty: char) -> Diagnostic {
    err(
        "DDS1014",
        line,
        format!("Keyword {kw} on field {name} is not valid for type {ty}"),
    )
}

/// `DFT` value does not suit the field.
pub fn bad_default(line: usize, name: &str, why: &str) -> Diagnostic {
    err("DDS1015", line, format!("Field {name}: DFT value {why}"))
}

/// `VARLEN` allocation longer than the field.
pub fn varlen_alloc(line: usize, name: &str, alloc: u32, len: u32) -> Diagnostic {
    err(
        "DDS1017",
        line,
        format!("Field {name}: VARLEN allocated length {alloc} exceeds the field length {len}"),
    )
}

/// Record longer than the maximum.
pub fn record_too_long(line: usize, len: u32, max: u32) -> Diagnostic {
    err(
        "DDS1018",
        line,
        format!("Record length {len} exceeds the maximum of {max}"),
    )
}

/// `DFT(*NULL)` on a field that is not null-capable.
pub fn null_default_needs_alwnull(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1019",
        line,
        format!("Field {name}: DFT(*NULL) requires ALWNULL"),
    )
}

/// `DATSEP` or `TIMSEP` with a format whose separator is fixed.
pub fn separator_not_allowed(line: usize, kw: &str, fmt: &str) -> Diagnostic {
    err(
        "DDS1020",
        line,
        format!("Keyword {kw} is not valid with {fmt}, whose separator is fixed"),
    )
}

/// A field or key before the first `R`.
pub fn spec_before_format(line: usize, what: &str) -> Diagnostic {
    err(
        "DDS1022",
        line,
        format!("A {what} specification must follow a record format specification"),
    )
}

/// A field after the `K` lines.
pub fn field_after_keys(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1023",
        line,
        format!("Field {name} follows the key specifications; fields come first"),
    )
}

/// More fields than a format may have.
pub fn too_many_fields(line: usize, max: usize) -> Diagnostic {
    err(
        "DDS1024",
        line,
        format!("The record format has more than {max} fields"),
    )
}

/// Key longer than the maximum.
pub fn key_too_long(line: usize, bytes: u32, max: u32) -> Diagnostic {
    err(
        "DDS1025",
        line,
        format!("Key length {bytes} exceeds the maximum of {max} bytes"),
    )
}

/// More key fields than allowed.
pub fn too_many_keys(line: usize, max: usize) -> Diagnostic {
    err("DDS1026", line, format!("More than {max} key fields"))
}

/// Fields under an `R` line that uses `FORMAT`.
pub fn fields_with_format(line: usize) -> Diagnostic {
    err(
        "DDS1027",
        line,
        "Field specifications are not allowed when the record format uses FORMAT".to_string(),
    )
}

/// `K` lines in a file that uses `REFACCPTH`.
pub fn keys_with_refaccpth(line: usize) -> Diagnostic {
    err(
        "DDS1028",
        line,
        "Key specifications are not allowed when the file uses REFACCPTH".to_string(),
    )
}

/// `NOALTSEQ` in a file without `ALTSEQ`.
pub fn noaltseq_without_altseq(line: usize, name: &str) -> Diagnostic {
    warn(
        "DDS1029",
        line,
        format!("NOALTSEQ on key field {name} has no effect; the file has no ALTSEQ"),
    )
}

/// A date or time length that disagrees with its format.
pub fn datetime_length(line: usize, name: &str, given: u32, want: u32, fmt: &str) -> Diagnostic {
    err(
        "DDS1030",
        line,
        format!("Field {name}: length {given} does not match {fmt}, which is {want}"),
    )
}

/// A record format without fields.
pub fn empty_format(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS1031",
        line,
        format!("Record format {name} has no fields"),
    )
}

/// A single-precision float with more than 9 digits.
pub fn float_precision(line: usize, name: &str, len: u32) -> Diagnostic {
    err(
        "DDS1032",
        line,
        format!("Field {name}: length {len} exceeds 9 digits; specify FLTPCN(*DOUBLE)"),
    )
}

// References.

/// The referenced field is not in the referenced file.
pub fn ref_field_not_found(line: usize, field: &str, file: impl Display) -> Diagnostic {
    err(
        "DDS3001",
        line,
        format!("Referenced field {field} not found in {file}"),
    )
}

/// The referenced file does not exist.
pub fn ref_file_not_found(line: usize, file: impl Display) -> Diagnostic {
    err("DDS3002", line, format!("Reference file {file} not found"))
}

/// `R` in position 29 with nothing naming a file.
pub fn ref_without_file(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS3003",
        line,
        format!("Field {name}: R in position 29 but neither REF nor REFFLD names a file"),
    )
}

/// The referenced file has no format of that name.
pub fn ref_format_not_found(line: usize, file: impl Display, fmt: &str) -> Diagnostic {
    err(
        "DDS3005",
        line,
        format!("Record format {fmt} not found in {file}"),
    )
}

/// `REFFLD(... *SRC)` names a field not defined earlier.
pub fn src_ref_not_found(line: usize, name: &str, field: &str) -> Diagnostic {
    err(
        "DDS3006",
        line,
        format!(
            "Field {name}: *SRC reference to {field}, which is not defined earlier in this source"
        ),
    )
}

/// The resolver failed.
pub fn ref_error(line: usize, file: impl Display, e: impl Display) -> Diagnostic {
    err("DDS3007", line, format!("Reference to {file} failed: {e}"))
}

/// `REFACCPTH` names a file without keys.
pub fn refaccpth_no_keys(line: usize, file: impl Display) -> Diagnostic {
    err(
        "DDS3008",
        line,
        format!("REFACCPTH: {file} has no access path to copy"),
    )
}

// Logical files.

/// An `R` line without `PFILE` or `JFILE`.
pub fn lf_needs_pfile(line: usize) -> Diagnostic {
    err(
        "DDS4001",
        line,
        "A record format of a logical file needs PFILE or JFILE".to_string(),
    )
}

/// A field the based-on file does not have.
pub fn base_field_not_found(line: usize, field: &str, file: impl Display) -> Diagnostic {
    err("DDS4003", line, format!("Field {field} is not in {file}"))
}

/// `JFILE` with fewer than two files.
pub fn jfile_count(line: usize) -> Diagnostic {
    err(
        "DDS4004",
        line,
        "JFILE names at least two files".to_string(),
    )
}

/// A `J` line in a format without `JFILE`.
pub fn join_spec_not_in_join(line: usize) -> Diagnostic {
    err(
        "DDS4005",
        line,
        "A join specification needs a record format with JFILE".to_string(),
    )
}

/// `JREF` names no `JFILE` file.
pub fn jref_unknown(line: usize, parm: impl Display) -> Diagnostic {
    err(
        "DDS4006",
        line,
        format!("JREF({parm}) names no file of JFILE"),
    )
}

/// A field in several `JFILE` files without `JREF`.
pub fn ambiguous_field(line: usize, field: &str) -> Diagnostic {
    err(
        "DDS4007",
        line,
        format!("Field {field} is in more than one JFILE file; specify JREF"),
    )
}

/// A select/omit field that is not in the format.
pub fn selomit_field_not_found(line: usize, field: &str) -> Diagnostic {
    err(
        "DDS4009",
        line,
        format!("Select/omit field {field} is not in the record format"),
    )
}

/// An `S` or `O` line without a test.
pub fn selomit_needs_test(line: usize) -> Diagnostic {
    err(
        "DDS4010",
        line,
        "A select/omit specification needs VALUES, RANGE, COMP or ALL".to_string(),
    )
}

/// `ALL` with a field name or other keywords.
pub fn all_alone(line: usize) -> Diagnostic {
    err(
        "DDS4011",
        line,
        "ALL takes no field name and no other keyword".to_string(),
    )
}

/// An AND condition with no `S` or `O` before it.
pub fn condition_without_group(line: usize) -> Diagnostic {
    err(
        "DDS4012",
        line,
        "An AND condition must follow a select or omit specification".to_string(),
    )
}

/// A second `R` in a join logical file.
pub fn join_one_format(line: usize) -> Diagnostic {
    err(
        "DDS4013",
        line,
        "A join logical file has one record format".to_string(),
    )
}

/// `SST` positions outside the field.
pub fn sst_range(line: usize, name: &str, field: &str, len: u32) -> Diagnostic {
    err(
        "DDS4015",
        line,
        format!("Field {name}: SST positions are outside {field}, which is {len} long"),
    )
}

/// `CONCAT` of a field whose type cannot be concatenated.
pub fn concat_type(line: usize, name: &str, field: &str, ty: char) -> Diagnostic {
    err(
        "DDS4016",
        line,
        format!("Field {name}: CONCAT cannot include {field} of type {ty}"),
    )
}

/// Keyed and arrival formats in one file.
pub fn keyed_mix(line: usize, fmt: &str) -> Diagnostic {
    err(
        "DDS4017",
        line,
        format!("Record format {fmt}: every format of a keyed logical file needs key fields"),
    )
}

/// A join key not from the primary file.
pub fn join_key_not_primary(line: usize, name: &str) -> Diagnostic {
    err(
        "DDS4018",
        line,
        format!("Key field {name} is not from the primary file of the join"),
    )
}

/// `K *NONE` with other keys.
pub fn none_with_keys(line: usize) -> Diagnostic {
    err(
        "DDS4019",
        line,
        "K *NONE cannot be combined with key fields".to_string(),
    )
}

/// Wrong number of `J` lines for the `JFILE` list.
pub fn join_count(line: usize, want: usize, got: usize) -> Diagnostic {
    err(
        "DDS4021",
        line,
        format!("{want} join specification(s) needed for the JFILE files; {got} given"),
    )
}

/// A `K` line after `S` or `O`.
pub fn key_after_selomit(line: usize) -> Diagnostic {
    err(
        "DDS4022",
        line,
        "Key specifications must come before select/omit specifications".to_string(),
    )
}

/// `SST` of a field whose type cannot be substringed.
pub fn sst_source_type(line: usize, name: &str, field: &str, ty: char) -> Diagnostic {
    err(
        "DDS4024",
        line,
        format!("Field {name}: SST cannot use {field} of type {ty}"),
    )
}

/// `JOIN` missing with more than two files.
pub fn join_required(line: usize) -> Diagnostic {
    err(
        "DDS4025",
        line,
        "JOIN is required when JFILE names more than two files".to_string(),
    )
}

/// A `J` line without `JFLD`.
pub fn jfld_required(line: usize) -> Diagnostic {
    err(
        "DDS4026",
        line,
        "A join specification needs JFLD".to_string(),
    )
}

/// `JDFTVAL` in a file that is not a join logical file.
pub fn jdftval_without_join(line: usize) -> Diagnostic {
    err(
        "DDS4027",
        line,
        "JDFTVAL requires a join logical file".to_string(),
    )
}

// Printer files.

/// Positions 39-44 do not hold a line and a position.
pub fn bad_location(line: usize, text: &str) -> Diagnostic {
    err(
        "DDS5001",
        line,
        format!("Positions 39-44 ({text:?}) must hold a line (39-41) and a position or +n (42-44)"),
    )
}

/// A data type a printer file cannot hold.
pub fn prtf_type(line: usize, ch: char) -> Diagnostic {
    err(
        "DDS5002",
        line,
        format!("Data type {ch} is not valid in a printer file; use A or S"),
    )
}

/// A usage a printer file cannot have.
pub fn prtf_usage(line: usize, ch: char) -> Diagnostic {
    err(
        "DDS5003",
        line,
        format!("Usage {ch} is not valid in a printer file; use blank, O or P"),
    )
}

/// A keyword's parameters are wrong.
pub fn prtf_parm(line: usize, kw: &str) -> Diagnostic {
    err(
        "DDS5004",
        line,
        format!("The parameters of {kw} are not valid"),
    )
}

/// A keyword is accepted and has no effect yet.
pub fn prtf_ignored(line: usize, kw: &str) -> Diagnostic {
    warn(
        "DDS5005",
        line,
        format!("{kw} is accepted and does not change the output"),
    )
}

/// A printer file with no record format.
pub fn prtf_no_formats() -> Diagnostic {
    Diagnostic::new(
        "DDS5006",
        Severity::Error,
        None,
        "A printer file needs at least one record format",
    )
}

/// A field or constant without a location.
pub fn prtf_no_location(line: usize) -> Diagnostic {
    err(
        "DDS5007",
        line,
        "A field or constant needs a position in positions 42-44".to_string(),
    )
}

/// `REFFLD` or `R` names a field nobody has.
pub fn prtf_ref(line: usize, field: &str) -> Diagnostic {
    err(
        "DDS5008",
        line,
        format!("Referenced field {field} was not found"),
    )
}

/// Conditioning indicators are malformed.
pub fn prtf_cond(line: usize, text: &str) -> Diagnostic {
    err(
        "DDS5009",
        line,
        format!(
            "Positions 8-16 ({text:?}) must hold up to three indicators, each N or blank and 01-99"
        ),
    )
}
