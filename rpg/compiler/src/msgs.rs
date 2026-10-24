// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The compiler's message texts, by identifier: Quartz's wording for
//! IBM's identifiers. `&1` takes the diagnostic's first value.

use rpgfront::Diag;

/// The texts.
const TEXTS: &[(&str, &str)] = &[
    ("RNF5318", "The subroutine's name is already the name of something else; the subroutine is ignored."),
    ("RNF5428", "&1 is allowed only inside a subroutine."),
    ("RNF7260", "&1 cannot be the factor 2 of this operation."),
    ("RNF0334", "%DECPOS needs a decimal or integer value; &1 is not one."),
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
    ("RNF0599", "The format &1 is not valid for this built-in function."),
    ("RNF0618", "The length or number of digits for &1 is not valid."),
    ("RNF0955", "This item cannot be the target of an assignment."),
    ("RNF2109", "No external description was found for file &1."),
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
    ("RNF6011", "The record has no space or skip entries and no PRTCTL; it spaces one line after."),
    ("RNF7014", "The arguments of &1 are not valid."),
    ("RNF7021", "File &1 is not defined."),
    ("RNF7023", "The program has no way to end."),
    ("RNF7030", "&1 is not defined."),
    ("RNF7066", "Record format &1 is not named by any input or output operation."),
    ("RNF7078", "The search argument's length or type differs from the file's first key field."),
    ("RNF7086", "Records of the file are read in blocks; its INFDS changes only when a block moves."),
    ("RNF7089", "File &1 keeps its indicators in a separate area, which the program passes on each operation."),
    ("RNF7350", "&1 leaves out subfields that have no namesake in the other structure."),
    ("RNF7404", "An index or position is out of range."),
    ("RNF7419", "An expression of this type is not allowed here."),
    ("RNF7421", "The types of the operands do not match."),
    ("RNF7510", "The value &1 is too short for a complete date in its format."),
    ("RNF7519", "The value &1 is too short for a complete timestamp."),
    ("RNF7535", "The number of parameters for &1 does not match its prototype."),
    ("RNF7536", "A parameter does not match its prototype."),
    ("RNF0231", "The length given for &1 is above the largest allowed; the largest is used."),
    ("RNF0256", "A main-procedure statement after a procedure is ignored."),
    ("RNF0305", "&1 is not a valid date in the date format in effect."),
    ("RNF0306", "&1 is not a valid time in the time format in effect."),
    ("RNF0307", "&1 is not a valid timestamp."),
    ("RNF0312", "A right parenthesis is missing."),
    ("RNF0316", "&1 needs more arguments; the built-in function is ignored."),
    ("RNF0317", "&1 has more arguments than it takes."),
    ("RNF0331", "The argument of &1 has no elements; the built-in function is ignored."),
    ("RNF0362", "The start position of &1 must be numeric; the built-in function is ignored."),
    ("RNF0503", "&1 allows at most 63 digits; 63 is used."),
    ("RNF0504", "&1 allows at most 9 digits; 9 is used."),
    ("RNF0512", "&1 has more decimal positions than digits; none are used."),
    ("RNF0533", "&1 takes 3, 5, 10 or 20 digits; 10 is used."),
    ("RNF0604", "The built-in function &1 is not known; it is ignored."),
    ("RNF0607", "The rest of the entry is skipped after the error."),
    ("RNF0612", "The date format is not valid; *ISO is used."),
    ("RNF0637", "An operand is missing; the statement is ignored."),
    ("RNF1301", "Control keyword &1 is not known; it is ignored."),
    ("RNF1302", "Control keyword &1 is given more than once; it is ignored."),
    ("RNF1324", "Control keyword &1 is not allowed when a module is created; it is ignored."),
    ("RNF1502", "The procedure has no end; one is assumed."),
    ("RNF1509", "END-PROC has no procedure to end."),
    ("RNF2026", "File &1 is declared more than once; the declaration is ignored."),
    ("RNF2120", "No external description was found for file &1; the file is ignored."),
    ("RNF2372", "The USAGE value is not allowed for the file's device."),
    ("RNF3308", "&1 is not a definition keyword; it is ignored."),
    ("RNF3312", "Keyword &1 is given more than once; it is ignored."),
    ("RNF3316", "&1 is defined more than once."),
    ("RNF3327", "The keyword cannot be combined with &1; it is ignored."),
    ("RNF3358", "&1 takes a length of 4 or 8 bytes; 8 is used."),
    ("RNF3401", "The name is not valid; blanks are used."),
    ("RNF3430", "The initial value is not of the field's type; it is ignored."),
    ("RNF3438", "&1 has no data type and no LIKE; the definition is ignored."),
    ("RNF3452", "The DIM value must be from 1 to 16773104."),
    ("RNF3467", "The OVERLAY name is not a subfield of the structure; the keyword is ignored."),
    ("RNF3545", "DIM on a data structure needs QUALIFIED; the keyword is ignored."),
    ("RNF3551", "&1 has no group to end."),
    ("RNF3556", "The group has no &1; one is assumed."),
    ("RNF3557", "The length given for &1 must be at least 1; 1 is used."),
    ("RNF3571", "The LIKE adjustment must start with + or -; it is ignored."),
    ("RNF3576", "The POS value must be a number from 1 to 16773104."),
    ("RNF3596", "&1 needs at least one digit; 1 is used."),
    ("RNF3599", "A free-form subfield gives its place in its own structure with POS, not OVERLAY."),
    ("RNF3764", "Parameter &1 of the interface differs from the prototype's."),
    ("RNF3768", "The procedure already has an interface; this one is ignored."),
    ("RNF3901", "A named constant takes no keywords; the keyword is ignored."),
    ("RNF3905", "Constant &1 has no value; the definition is ignored."),
    ("RNF5016", "ENDSR has no subroutine to end."),
    ("RNF5054", "Subroutine &1 is defined more than once."),
    ("RNF5141", "The ending operation has no group to end."),
    ("RNF5177", "The group begun here has no ending operation; one is assumed at the end."),
    ("RNF5178", "The subroutine has no ENDSR before the next BEGSR; one is assumed."),
    ("RNF5232", "LEAVE and ITER are allowed only inside a loop; the statement is ignored."),
    ("RNF5249", "WHEN or OTHER is not inside an open SELECT group."),
    ("RNF5276", "File &1 is not valid for this operation; the statement is ignored."),
    ("RNF5295", "ON-ERROR is allowed only inside a MONITOR group."),
    ("RNF5412", "The procedure returns no value; the RETURN value is ignored."),
    ("RNF5413", "The procedure returns a value; RETURN needs one."),
    ("RNF5415", "Procedure &1 returns a value but has no RETURN with one."),
    ("RNF7018", "&1 is not a subroutine of this procedure."),
    ("RNF7301", "Subfield &1 ends past the length of its data structure."),
    ("RNF7416", "The value's type does not match the target of the assignment."),
    ("RNF7423", "A condition must be a comparison, a logical expression or an indicator."),
    ("RNF7591", "&1 is not a qualified data structure; it cannot qualify a name."),
    ("RNF0263", "A numeric entry is not right-aligned in its columns."),
    ("RNF3431", "The initial value does not fit the field."),
    ("RNF5005", "Calculations follow the subroutines."),
    ("RNF5508", "Something follows the end of the free-form statement."),
    ("RNF7031", "No reference is made to &1."),
    ("RNF7057", "Half adjust has no effect: the result has no more decimals than the target."),
    ("RNF7503", "An operand of the expression is not defined."),
    ("RNF7504", "A constant divisor is zero."),
    ("RNF7534", "A global file of a module without a main procedure is not closed for it."),
    ("RNF7551", "The intermediate result keeps fewer decimals than the target."),
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
