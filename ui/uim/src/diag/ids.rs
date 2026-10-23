// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Every message the compiler sends, one function each. IDs: 01xx the
//! source and the lexer, 02xx tag structure, 03xx attributes, 1xxx
//! element rules, 2xxx name references, 3xxx panel rules.

use std::fmt::Display;

use super::{Diagnostic, Severity};

fn at(id: &'static str, sev: Severity, line: usize, text: String) -> Diagnostic {
    Diagnostic::new(id, sev, Some(line), text)
}

fn warn(id: &'static str, line: usize, text: String) -> Diagnostic {
    at(id, Severity::Warning, line, text)
}

fn err(id: &'static str, line: usize, text: String) -> Diagnostic {
    at(id, Severity::Error, line, text)
}

fn severe(id: &'static str, line: usize, text: String) -> Diagnostic {
    at(id, Severity::Severe, line, text)
}

// The source and the lexer.

/// `.IM` names a member the resolver does not have.
pub fn imbed_not_found(line: usize, name: &str) -> Diagnostic {
    severe(
        "UIM0101",
        line,
        format!("Imbedded member {name} was not found; the .IM line is skipped"),
    )
}

/// `.IM` nests deeper than allowed or imbeds itself.
pub fn imbed_too_deep(line: usize, name: &str) -> Diagnostic {
    severe(
        "UIM0102",
        line,
        format!("Imbedded member {name} nests too deeply or imbeds itself; skipped"),
    )
}

/// The source ends inside a tag.
pub fn tag_not_closed(line: usize, tag: &str) -> Diagnostic {
    err(
        "UIM0103",
        line,
        format!("Tag :{tag} is not closed by a period before the end of the source"),
    )
}

/// A quoted attribute value runs to the end of the source.
pub fn quote_not_closed(line: usize, tag: &str, attr: &str) -> Diagnostic {
    err(
        "UIM0104",
        line,
        format!("The quoted value of {attr} on :{tag} is not closed"),
    )
}

/// A tab in the source.
pub fn tab(line: usize) -> Diagnostic {
    warn(
        "UIM0105",
        line,
        "Tab character replaced by a blank".to_string(),
    )
}

/// A line starts with a control word other than `.*` or `.IM`.
pub fn control_word(line: usize, word: &str) -> Diagnostic {
    err(
        "UIM0106",
        line,
        format!("Control word {word} is not known; only .* and .IM are"),
    )
}

/// A character inside a tag that starts neither an attribute nor the period.
pub fn stray_in_tag(line: usize, tag: &str, ch: char) -> Diagnostic {
    err(
        "UIM0107",
        line,
        format!("Character {ch:?} inside :{tag} is not part of an attribute; skipped"),
    )
}

/// `.IM` without a member name.
pub fn imbed_name_missing(line: usize) -> Diagnostic {
    severe("UIM0108", line, ".IM needs a member name".to_string())
}

// Tag structure.

/// A tag the language does not have.
pub fn unknown_tag(line: usize, tag: &str) -> Diagnostic {
    err("UIM0201", line, format!("Tag :{tag} is not known; skipped"))
}

/// An end tag with no open element to close.
pub fn end_without_start(line: usize, tag: &str) -> Diagnostic {
    err(
        "UIM0202",
        line,
        format!("End tag :{tag} has no open element to close"),
    )
}

/// An element is still open where its parent ends.
pub fn not_closed(line: usize, tag: &str, by: &str) -> Diagnostic {
    err(
        "UIM0203",
        line,
        format!("Element :{tag} is not closed before {by}"),
    )
}

/// A tag where its parent does not allow it.
pub fn not_allowed_here(line: usize, tag: &str, parent: &str) -> Diagnostic {
    err(
        "UIM0204",
        line,
        format!("Tag :{tag} is not allowed inside :{parent}"),
    )
}

/// Text where the open element takes none.
pub fn text_not_allowed(line: usize, parent: &str) -> Diagnostic {
    err(
        "UIM0205",
        line,
        format!("Text is not allowed inside :{parent}"),
    )
}

/// The source has no `:PNLGRP.`.
pub fn no_pnlgrp() -> Diagnostic {
    Diagnostic::new(
        "UIM0206",
        Severity::Terminal,
        None,
        "The source has no :PNLGRP tag; nothing to compile",
    )
}

/// More than one `:PNLGRP.`.
pub fn second_pnlgrp(line: usize) -> Diagnostic {
    err(
        "UIM0207",
        line,
        "A second :PNLGRP tag; only one panel group per source".to_string(),
    )
}

/// An inline tag is still open when its text ends.
pub fn inline_not_closed(line: usize, tag: &str) -> Diagnostic {
    err(
        "UIM0208",
        line,
        format!("Highlight tag :{tag} is not closed before its text ends"),
    )
}

// Attributes.

/// An attribute the tag does not have.
pub fn unknown_attr(line: usize, tag: &str, attr: &str) -> Diagnostic {
    warn(
        "UIM0301",
        line,
        format!("Attribute {attr} is not known on :{tag}; ignored"),
    )
}

/// A required attribute is missing.
pub fn attr_missing(line: usize, tag: &str, attr: &str) -> Diagnostic {
    err(
        "UIM0302",
        line,
        format!("Tag :{tag} needs the {attr} attribute"),
    )
}

/// An attribute value the attribute does not take.
pub fn bad_attr(line: usize, tag: &str, attr: &str, value: &str, why: impl Display) -> Diagnostic {
    err(
        "UIM0303",
        line,
        format!("Value {value:?} of {attr} on :{tag} is not valid: {why}"),
    )
}

/// An attribute given twice.
pub fn attr_repeated(line: usize, tag: &str, attr: &str) -> Diagnostic {
    warn(
        "UIM0304",
        line,
        format!("Attribute {attr} is repeated on :{tag}; the last value is used"),
    )
}

/// A flag attribute written without a value where one is needed.
pub fn attr_needs_value(line: usize, tag: &str, attr: &str) -> Diagnostic {
    err(
        "UIM0305",
        line,
        format!("Attribute {attr} on :{tag} needs a value"),
    )
}

// Element rules.

/// A name defined twice.
pub fn duplicate_name(line: usize, what: &str, name: &str) -> Diagnostic {
    err("UIM1001", line, format!("{what} {name} is already defined"))
}

/// A name that is not a UIM name.
pub fn bad_name(line: usize, what: &str, name: &str) -> Diagnostic {
    err(
        "UIM1002",
        line,
        format!(
            "{what} name {name:?} is not valid; up to 10 of letters, digits, _ @ # $, starting with a letter"
        ),
    )
}

/// A `BASETYPE` the language does not have.
pub fn bad_basetype(line: usize, text: &str, why: impl Display) -> Diagnostic {
    err(
        "UIM1003",
        line,
        format!("BASETYPE {text:?} is not valid: {why}"),
    )
}

/// A condition expression that does not parse.
pub fn bad_expr(line: usize, name: &str, why: impl Display) -> Diagnostic {
    err(
        "UIM1004",
        line,
        format!("Expression of condition {name} is not valid: {why}"),
    )
}

/// A truth table row with the wrong number of values.
pub fn tt_row_width(line: usize, got: usize, want: usize) -> Diagnostic {
    err(
        "UIM1005",
        line,
        format!("Truth table row has {got} values; CONDS names {want} conditions"),
    )
}

/// A list or menu option used twice.
pub fn duplicate_option(line: usize, option: &str) -> Diagnostic {
    err(
        "UIM1006",
        line,
        format!("Option {option} is already used in this area"),
    )
}

/// A menu option outside 1 to 999.
pub fn bad_menu_option(line: usize, option: &str) -> Diagnostic {
    err(
        "UIM1007",
        line,
        format!(
            "Menu option {option:?} must be a number from 1 to 999, or * on a menu with a command line"
        ),
    )
}

/// A list area without columns.
pub fn list_without_columns(line: usize) -> Diagnostic {
    err(
        "UIM1008",
        line,
        "List area has no :LISTCOL and no :LISTVIEW; it would show nothing".to_string(),
    )
}

/// A view names a column the list does not have.
pub fn view_column_unknown(line: usize, col: &str) -> Diagnostic {
    err(
        "UIM1009",
        line,
        format!("COLS names column {col}, which the list does not define"),
    )
}

/// An input field or column whose class has no width.
pub fn input_without_width(line: usize, var: &str, class: &str) -> Diagnostic {
    err(
        "UIM1010",
        line,
        format!(
            "Variable {var} is input but class {class} has no width; give the class a WIDTH or a sized BASETYPE"
        ),
    )
}

/// Area depths that do not fit the panel.
pub fn depth_exceeds_panel(line: usize, total: u32, available: u32) -> Diagnostic {
    err(
        "UIM1011",
        line,
        format!("Area depths add up to {total} lines; the panel has {available} for areas"),
    )
}

/// A key listed twice in a key list.
pub fn duplicate_key(line: usize, key: &str) -> Diagnostic {
    err(
        "UIM1012",
        line,
        format!("Key {key} is already in this key list"),
    )
}

/// A `*` menu option on a menu whose panel has no command line.
pub fn star_without_cmdline(line: usize) -> Diagnostic {
    err(
        "UIM1013",
        line,
        "Option * needs a command line on the panel".to_string(),
    )
}

/// A list action with neither an option nor an ENTER action.
pub fn listact_without_option(line: usize) -> Diagnostic {
    err(
        "UIM1014",
        line,
        ":LISTACT needs OPTION or ENTER".to_string(),
    )
}

/// A list with more views than `VIEWS` says, or `VIEWS` with no views.
pub fn views_mismatch(line: usize, got: usize, want: u32) -> Diagnostic {
    warn(
        "UIM1015",
        line,
        format!("The list has {got} :LISTVIEW tags; VIEWS says {want}"),
    )
}

/// An action that does not parse.
pub fn bad_action(line: usize, text: &str, why: impl Display) -> Diagnostic {
    err(
        "UIM1016",
        line,
        format!("Action {text:?} is not valid: {why}"),
    )
}

/// A key that does not parse.
pub fn bad_key(line: usize, text: &str) -> Diagnostic {
    err(
        "UIM1017",
        line,
        format!(
            "Key {text:?} is not valid; use ENTER, HELP, PRINT, HOME, PAGEUP, PAGEDOWN or F1 to F24"
        ),
    )
}

/// A `:DATAIX.` with no `:DATAI.` before it.
pub fn extender_without_item(line: usize) -> Diagnostic {
    err(
        "UIM1018",
        line,
        ":DATAIX must follow a :DATAI in the same group".to_string(),
    )
}

/// An option on a list action that is not a number or a single letter.
pub fn bad_list_option(line: usize, option: &str) -> Diagnostic {
    err(
        "UIM1019",
        line,
        format!("List option {option:?} must be a number from 1 to 99 or one letter"),
    )
}

// References.

/// A reference to a name of the given kind that nothing defines.
pub fn unresolved(line: usize, what: &str, name: &str) -> Diagnostic {
    let id = match what {
        "Class" => "UIM2001",
        "Variable" => "UIM2002",
        "List" => "UIM2003",
        "Condition" => "UIM2004",
        "Key list" => "UIM2005",
        "Help module" => "UIM2006",
        "Panel" => "UIM2007",
        "Variable record" => "UIM2008",
        "Truth table" => "UIM2009",
        _ => "UIM2000",
    };
    err(id, line, format!("{what} {name} is not defined"))
}

/// `&name.` in text that is neither a symbol nor a variable.
pub fn unknown_symbol(line: usize, name: &str) -> Diagnostic {
    err(
        "UIM2010",
        line,
        format!("&{name}. is neither a symbol nor a variable of this panel group"),
    )
}

/// An import of a name another group does not export.
pub fn import_not_exported(line: usize, name: &str, pnlgrp: &str) -> Diagnostic {
    err(
        "UIM2011",
        line,
        format!("Panel group {pnlgrp} does not export {name}"),
    )
}

// Panel rules.

/// A panel with no areas.
pub fn empty_panel(line: usize, name: &str) -> Diagnostic {
    warn("UIM3001", line, format!("Panel {name} has no areas"))
}

/// A panel with two command lines.
pub fn second_cmdline(line: usize) -> Diagnostic {
    err(
        "UIM3002",
        line,
        "A panel has at most one command line".to_string(),
    )
}

/// A panel that is neither a screen (24x80, 27x132, 24x132) nor a
/// pop-up window (5 to 25 rows, 20 to 128 columns).
pub fn bad_panel_size(line: usize, depth: u32, width: u32) -> Diagnostic {
    err(
        "UIM3003",
        line,
        format!(
            "Panel size {depth}x{width} is not a screen (24x80, 27x132 or 24x132) or a pop-up window (5 to 25 rows, 20 to 128 columns)"
        ),
    )
}

/// A help module with no text.
pub fn empty_help(line: usize, name: &str) -> Diagnostic {
    warn("UIM3004", line, format!("Help module {name} has no text"))
}
