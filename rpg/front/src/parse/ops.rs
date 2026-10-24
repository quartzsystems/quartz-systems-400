// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The operation codes: which exist, which take the extended factor 2
//! in fixed form, which free form allows, and how free-form operands
//! map onto the factor 1, factor 2 and result roles.

use crate::ast::{Expr, Op};

/// Operations written without a condition suffix.
const PLAIN: &[&str] = &[
    "ACQ", "ADD", "ADDDUR", "ALLOC", "BEGSR", "BITOFF", "BITON", "CALL", "CALLB", "CALLP", "CAT",
    "CHAIN", "CHECK", "CHECKR", "CLEAR", "CLOSE", "COMMIT", "COMP", "DEALLOC", "DEFINE", "DELETE",
    "DIV", "DO", "DOU", "DOW", "DSPLY", "DUMP", "ELSE", "ELSEIF", "END", "ENDCS", "ENDDO", "ENDFOR",
    "ENDIF", "ENDMON", "ENDSL", "ENDSR", "EVAL", "EVAL-CORR", "EVALR", "EXCEPT", "EXFMT", "EXSR",
    "EXTRCT", "FEOD", "FOR", "FORCE", "GOTO", "IF", "IN", "ITER", "KFLD", "KLIST", "LEAVE",
    "LEAVESR", "LOOKUP", "MHHZO", "MHLZO", "MLHZO", "MLLZO", "MONITOR", "MOVE", "MOVEA", "MOVEL",
    "MULT", "MVR", "NEXT", "OCCUR", "ON-ERROR", "ON-EXCP", "OPEN", "OTHER", "OUT", "PARM", "PLIST",
    "POST", "READ", "READC", "READE", "READP", "READPE", "REALLOC", "REL", "RESET", "RETURN",
    "ROLBK", "SCAN", "SELECT", "SETGT", "SETLL", "SETOFF", "SETON", "SHTDN", "SORTA", "SQRT", "SUB",
    "SUBDUR", "SUBST", "TAG", "TEST", "TESTB", "TESTN", "TESTZ", "TIME", "UNLOCK", "UPDATE", "WHEN",
    "WRITE", "XFOOT", "XLATE", "XML-INTO", "XML-SAX", "Z-ADD", "Z-SUB", "DATA-INTO", "DATA-GEN",
    "SND-MSG", "FOR-EACH", "ON-EXIT", "WHEN-IS", "WHEN-IN", "CAS",
];

/// Prefixes that take a condition suffix (`IFEQ`, `CABLT`, ...).
const COND_FAMILIES: &[&str] = &["IF", "DOU", "DOW", "WHEN", "AND", "OR", "CAS", "CAB"];

/// The condition suffixes.
pub const SUFFIXES: &[&str] = &["EQ", "NE", "GT", "LT", "GE", "LE"];

/// Operations whose fixed-form factor 2 is extended (columns 36-80).
const EXTENDED: &[&str] = &[
    "CALLP", "DOU", "DOW", "EVAL", "EVAL-CORR", "EVALR", "FOR", "IF", "ELSEIF", "ON-ERROR",
    "ON-EXCP", "RETURN", "SORTA", "WHEN", "XML-INTO", "XML-SAX", "DATA-INTO", "DATA-GEN", "SND-MSG",
    "FOR-EACH", "WHEN-IS", "WHEN-IN", "ON-EXIT",
];

/// Operations free form does not allow.
const FIXED_ONLY: &[&str] = &[
    "ADD", "ADDDUR", "ALLOC", "BITOFF", "BITON", "CALL", "CALLB", "CAT", "CHECK", "CHECKR", "COMP",
    "DEFINE", "DIV", "DO", "END", "ENDCS", "EXTRCT", "GOTO", "KFLD", "KLIST", "LOOKUP", "MHHZO",
    "MHLZO", "MLHZO", "MLLZO", "MOVE", "MOVEA", "MOVEL", "MULT", "MVR", "OCCUR", "PARM", "PLIST",
    "REALLOC", "SCAN", "SETOFF", "SETON", "SHTDN", "SQRT", "SUB", "SUBDUR", "SUBST", "TAG", "TESTB",
    "TESTN", "TESTZ", "TIME", "XFOOT", "XLATE", "Z-ADD", "Z-SUB", "CAS",
];

/// Operations fixed form does not allow.
const FREE_ONLY: &[&str] = &["FOR-EACH", "SND-MSG", "WHEN-IS", "WHEN-IN", "ON-EXIT", "DATA-GEN"];

/// Split a written operation into its code and condition suffix:
/// `IFEQ` is (`IF`, `EQ`); `EVAL` is (`EVAL`, empty).
#[must_use]
pub fn split_suffix(code: &str) -> (&str, &str) {
    if PLAIN.contains(&code) {
        return (code, "");
    }
    for f in COND_FAMILIES {
        if let Some(sfx) = code.strip_prefix(f)
            && SUFFIXES.contains(&sfx)
        {
            return (f, sfx);
        }
    }
    (code, "")
}

/// Whether the operation (as written, suffix included) exists.
#[must_use]
pub fn known(code: &str) -> bool {
    let (base, sfx) = split_suffix(code);
    PLAIN.contains(&code) || !sfx.is_empty() && COND_FAMILIES.contains(&base)
}

/// Whether the fixed-form operation takes the extended factor 2.
#[must_use]
pub fn extended(code: &str) -> bool {
    EXTENDED.contains(&code)
}

/// Whether every extender letter is one RPG knows.
#[must_use]
pub fn extenders_known(ext: &str) -> bool {
    ext.chars().all(|c| "ADEHMNPRTZ".contains(c))
}

/// Whether the operation takes the extender letters; an operation not
/// listed takes any.
#[must_use]
pub fn extenders_allowed(code: &str, ext: &str) -> bool {
    let (base, _) = split_suffix(code);
    let allowed = match base {
        "ADD" | "SUB" | "MULT" | "DIV" | "Z-ADD" | "Z-SUB" | "SQRT" | "XFOOT" => "H",
        "EVAL" | "EVAL-CORR" | "RETURN" => "HMR",
        "EVALR" | "IF" | "DOU" | "DOW" | "WHEN" | "ELSEIF" | "FOR" => "MR",
        "CALLP" => "EMR",
        "CHAIN" | "READ" | "READE" | "READP" | "READPE" | "DEALLOC" => "EN",
        "CALLB" => "DE",
        "TEST" => "EDTZ",
        "CAT" | "MOVE" | "MOVEL" | "MOVEA" => "P",
        "SUBST" | "XLATE" => "EP",
        "SORTA" => "AD",
        "DUMP" => "A",
        "ACQ" | "ADDDUR" | "ALLOC" | "CALL" | "CHECK" | "CHECKR" | "CLOSE" | "COMMIT" | "DELETE"
        | "DSPLY" | "EXFMT" | "EXTRCT" | "FEOD" | "IN" | "NEXT" | "OCCUR" | "OPEN" | "OUT" | "POST" | "READC"
        | "REALLOC" | "REL" | "ROLBK" | "SCAN" | "SETGT" | "SETLL" | "SUBDUR" | "UNLOCK" | "UPDATE" | "WRITE" => "E",
        _ => return true,
    };
    ext.chars().all(|c| allowed.contains(c))
}

/// Whether the fixed-form operation must have a factor 1.
#[must_use]
pub fn needs_factor1(code: &str) -> bool {
    let (base, sfx) = split_suffix(code);
    !sfx.is_empty()
        || matches!(
            base,
            "BEGSR" | "CHAIN" | "CHECK" | "CHECKR" | "COMP" | "DEFINE" | "KLIST" | "LOOKUP" | "PLIST" | "SCAN"
                | "SETGT" | "SETLL" | "TAG"
        )
}

/// Whether the fixed-form operation must leave factor 1 blank.
#[must_use]
pub fn no_factor1(code: &str) -> bool {
    matches!(
        code,
        "BITOFF" | "BITON" | "CALLP" | "CLOSE" | "DEALLOC" | "DOU" | "DOW" | "ELSE" | "ELSEIF" | "END" | "ENDCS"
            | "ENDDO" | "ENDFOR" | "ENDIF" | "ENDMON" | "ENDSL" | "EVAL" | "EVALR" | "EXCEPT" | "EXFMT" | "EXSR"
            | "FEOD" | "FOR" | "GOTO" | "IF" | "ITER" | "LEAVE" | "LEAVESR" | "MONITOR" | "MOVEA" | "MVR"
            | "OPEN" | "OTHER" | "READ" | "READC" | "READP" | "RETURN" | "SELECT" | "SETOFF" | "SETON" | "SORTA"
            | "SQRT" | "TESTB" | "TESTN" | "TESTZ" | "TIME" | "UNLOCK" | "UPDATE" | "WHEN" | "WRITE" | "XFOOT"
            | "Z-ADD" | "Z-SUB"
    )
}

/// Whether free form refuses the operation.
#[must_use]
pub fn fixed_only(code: &str) -> bool {
    let (base, sfx) = split_suffix(code);
    !sfx.is_empty() || FIXED_ONLY.contains(&base)
}

/// Whether fixed form refuses the operation.
#[must_use]
pub fn free_only(code: &str) -> bool {
    FREE_ONLY.contains(&code)
}

/// Whether the operation's free-form operand is one expression (the
/// extended factor 2), possibly an assignment.
#[must_use]
pub fn free_takes_expression(code: &str) -> bool {
    matches!(
        code,
        "IF" | "ELSEIF" | "DOU" | "DOW" | "WHEN" | "RETURN" | "CALLP" | "SORTA" | "ON-ERROR"
            | "ON-EXCP" | "WHEN-IS" | "WHEN-IN" | "XML-INTO" | "XML-SAX" | "DATA-INTO" | "DATA-GEN"
            | "SND-MSG" | "ON-EXIT"
    )
}

/// Put free-form operands into their roles. `ops` are the operands in
/// order; `special` tells whether an operand is a given special word.
pub fn assign_free_roles(op: &mut Op, ops: Vec<Expr>) {
    let n = ops.len();
    let mut it = ops.into_iter();
    let is_word = |e: &Expr, w: &[&str]| matches!(e, Expr::Special { word, index: None, .. } if w.contains(&word.as_str()));
    match op.code.as_str() {
        // search-arg name [result]
        "CHAIN" | "READE" | "READPE" | "SETLL" | "SETGT" => {
            op.f1 = it.next().into_iter().collect();
            op.f2 = it.next().into_iter().collect();
            op.result = it.collect();
        }
        // name [result]
        "READ" | "READC" | "READP" | "WRITE" | "UPDATE" | "EXFMT" => {
            op.f2 = it.next().into_iter().collect();
            op.result = it.collect();
        }
        // [search-arg] name
        "DELETE" | "ACQ" | "REL" | "POST" | "NEXT" => {
            if n >= 2 {
                op.f1 = it.next().into_iter().collect();
            }
            op.f2 = it.next().into_iter().collect();
            op.result = it.collect();
        }
        // [*LOCK] name
        "IN" | "OUT" => {
            let v: Vec<Expr> = it.collect();
            for e in v {
                if is_word(&e, &["*LOCK"]) {
                    op.f1.push(e);
                } else {
                    op.f2.push(e);
                }
            }
        }
        // [*NOKEY] [*ALL] name
        "CLEAR" | "RESET" => {
            for e in it {
                if is_word(&e, &["*NOKEY"]) {
                    op.f1.push(e);
                } else if is_word(&e, &["*ALL"]) && op.result.is_empty() {
                    op.f2.push(e);
                } else {
                    op.result.push(e);
                }
            }
        }
        // [format] field
        "TEST" => {
            let v: Vec<Expr> = it.collect();
            let mut v = v.into_iter().rev();
            op.result = v.next().into_iter().collect();
            op.f1 = v.collect();
        }
        // message [queue [response]]
        "DSPLY" => {
            op.f1 = it.next().into_iter().collect();
            op.f2 = it.next().into_iter().collect();
            op.result = it.collect();
        }
        "BEGSR" | "DUMP" | "COMMIT" => op.f1 = it.collect(),
        "DEALLOC" => op.result = it.collect(),
        "ENDSR" => op.f2 = it.collect(),
        _ => op.f2 = it.collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suffixes_and_sets() {
        assert_eq!(split_suffix("IFEQ"), ("IF", "EQ"));
        assert_eq!(split_suffix("CABLE"), ("CAB", "LE"));
        assert_eq!(split_suffix("DOU"), ("DOU", ""));
        assert_eq!(split_suffix("ORGT"), ("OR", "GT"));
        assert!(known("WHENNE"));
        assert!(!known("IFXX"));
        assert!(fixed_only("MOVEL"));
        assert!(fixed_only("IFEQ"));
        assert!(!fixed_only("CHAIN"));
        assert!(extended("EVAL"));
        assert!(!extended("CHAIN"));
    }
}
