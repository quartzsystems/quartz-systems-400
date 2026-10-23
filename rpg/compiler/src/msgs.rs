// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiler's message texts, by identifier: Quartz's wording for
//! IBM's identifiers. `&1` takes the diagnostic's first value.

use rpgfront::Diag;

/// The texts.
const TEXTS: &[(&str, &str)] = &[
    ("RNF0001", "The compiler could not generate the program: &1."),
    ("RNF0023", "Copy member &1 was not found."),
    ("RNF0024", "Copy member &1 is already being copied, or copies nest too deep."),
    ("RNF0036", "A conditional group has no /ENDIF."),
    ("RNF0037", "The directive is not recognized."),
    ("RNF0039", "/ELSE, /ELSEIF or /ENDIF has no matching /IF."),
    ("RNF0201", "&1 is not valid here."),
    ("RNF0202", "A parenthesis is missing."),
    ("RNF0203", "&1 is not a valid keyword."),
    ("RNF0257", "The specification type is out of sequence or not valid."),
    ("RNF0267", "A character literal has no closing apostrophe; one is assumed."),
    ("RNF0280", "A continued name has no continuation."),
    ("RNF0308", "The float literal is not valid."),
    ("RNF0618", "The length or number of digits for &1 is not valid."),
    ("RNF0955", "This item cannot be the target of an assignment."),
    ("RNF2109", "No external description was found for file &1."),
    ("RNF2120", "&1 is defined more than once."),
    ("RNF2318", "No overflow indicator given for the printer file; *INOA is used, with a skip to line 6 on overflow."),
    ("RNF3304", "The data type &1 is not valid."),
    ("RNF3313", "The value of keyword &1 is not valid."),
    ("RNF3349", "&1 is not a data structure."),
    ("RNF3530", "The data structure has no subfields."),
    ("RNF5012", "&1 does not match an open operation."),
    ("RNF5013", "Operation code &1 is not valid."),
    ("RNF5034", "A declaration group is not ended, or ended where none is open."),
    ("RNF5346", "Operation &1 is not allowed in free form."),
    ("RNF5347", "An assignment operator is expected."),
    ("RNF5358", "&1 is not expected here."),
    ("RNF5377", "The expression should end here."),
    ("RNF5410", "No prototype is defined for the call to &1."),
    ("RNF5501", "Free-form specifications do not take a comment area."),
    ("RNF5507", "The free-form statement has no ending semicolon."),
    ("RNF7013", "The built-in function &1 is not known."),
    ("RNF7014", "The arguments of &1 are not valid."),
    ("RNF7021", "File &1 is not defined."),
    ("RNF7023", "The program has no way to end."),
    ("RNF7028", "The operation is not valid for file &1."),
    ("RNF7030", "&1 is not defined."),
    ("RNF7404", "An index or position is out of range."),
    ("RNF7419", "An expression of this type is not allowed here."),
    ("RNF7421", "The types of the operands do not match."),
    ("RNF7535", "The number of parameters for &1 does not match its prototype."),
    ("RNF7536", "A parameter does not match its prototype."),
];

/// The text of a diagnostic, its values filled in.
#[must_use]
pub fn text(d: &Diag) -> String {
    let t = TEXTS
        .iter()
        .find(|(id, _)| *id == d.msgid)
        .map_or("Diagnostic &1.", |(_, t)| *t);
    let mut s = t.to_string();
    for (i, a) in d.args.iter().enumerate().rev() {
        s = s.replace(&format!("&{}", i + 1), a);
    }
    s.replace("&1", "").replace("  ", " ")
}

/// The text of an identifier for the message summary (no values).
#[must_use]
pub fn summary_text(id: &str) -> String {
    TEXTS
        .iter()
        .find(|(i, _)| *i == id)
        .map_or_else(|| "Diagnostic.".to_string(), |(_, t)| t.replace("&1 ", "").replace("&1", "the item"))
}

/// Every identifier with its text (`&1` unfilled), for the check that
/// `QRPGLEMSG` says the same (`runtime/msgf/tests/rpg_texts.rs`).
#[must_use]
pub fn texts() -> &'static [(&'static str, &'static str)] {
    TEXTS
}
