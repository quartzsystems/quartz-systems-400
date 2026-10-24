// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The identifiers semantic analysis and code generation raise, by
//! meaning. The front end's are in `rpgfront::diag::ids`.

/// The identifiers.
pub mod ids {
    /// A name or indicator used but never declared.
    pub const NOT_DEFINED: &str = "RNF7030";
    /// A subroutine named like something else.
    pub const SUBR_NAME_TAKEN: &str = "RNF5318";
    /// An operation allowed only inside a subroutine.
    pub const NOT_IN_SUBR: &str = "RNF5428";
    /// Factor 2 names something the operation cannot take.
    pub const BAD_FACTOR2: &str = "RNF7260";
    /// `%DECPOS` of something that is not a fixed-point number.
    pub const DECPOS_ARG: &str = "RNF0334";
    /// An expression holds an operand that is not defined.
    pub const UNDEFINED_OPERAND: &str = "RNF7503";
    /// The name is defined more than once.
    pub const DUPLICATE: &str = "RNF3316";
    /// A length or number of digits is not valid.
    pub const BAD_LENGTH: &str = "RNF0618";
    /// The data type is not valid.
    pub const BAD_TYPE: &str = "RNF3304";
    /// A keyword's value is not valid.
    pub const BAD_KEYWORD_VALUE: &str = "RNF3313";
    /// The name is not a data structure.
    pub const NOT_A_DS: &str = "RNF3349";
    /// The types of the operands do not match.
    pub const TYPE_MISMATCH: &str = "RNF7421";
    /// The target of an assignment is not valid.
    pub const NOT_ASSIGNABLE: &str = "RNF0955";
    /// An expression of this type is not allowed here.
    pub const BAD_EXPRESSION_TYPE: &str = "RNF7419";
    /// An operation's structure is unbalanced (ENDIF without IF...).
    pub const UNBALANCED: &str = "RNF5012";
    /// Half-adjust has nothing to round: the value has no more decimals
    /// than the target.
    pub const HALF_ADJUST_IDLE: &str = "RNF7057";
    /// An intermediate result keeps fewer decimals than its target.
    pub const DECIMALS_LOST: &str = "RNF7551";
    /// The initial value does not fit the field.
    pub const INZ_TOO_LONG: &str = "RNF3431";
    /// A global file of a module without a cycle is not closed for it.
    pub const CLOSE_EXPLICITLY: &str = "RNF7534";
    /// A constant divisor of zero.
    pub const DIVIDE_BY_ZERO: &str = "RNF7504";
    /// A calculation after the subroutines.
    pub const AFTER_ENDSR: &str = "RNF5005";
    /// The program has no way to end.
    pub const NO_END: &str = "RNF7023";
    /// A data structure has no subfields that count.
    pub const DS_NO_SUBFIELDS: &str = "RNF3530";
    /// An overflow indicator was assigned to a printer file.
    pub const OFLIND_ASSIGNED: &str = "RNF2318";
    /// A file is not defined.
    pub const FILE_NOT_DEFINED: &str = "RNF7021";
    /// The operation is not valid for the file.
    pub const BAD_FILE_OP: &str = "RNF5276";
    /// The built-in function is not known.
    pub const UNKNOWN_BIF: &str = "RNF0604";
    /// A built-in function's arguments are wrong.
    pub const BIF_ARGS: &str = "RNF7014";
    /// The prototype for a call is missing.
    pub const NO_PROTOTYPE: &str = "RNF5410";
    /// The number of parameters does not match the prototype.
    pub const PARM_COUNT: &str = "RNF7535";
    /// A parameter does not match its prototype.
    pub const PARM_TYPE: &str = "RNF7536";
    /// The external description of a file is not found.
    pub const EXT_NOT_FOUND: &str = "RNF2109";
    /// A feature this release does not compile.
    pub const UNSUPPORTED: &str = "RNF0201";
    /// An output record without space or skip entries spaces one after.
    pub const SPACE_DEFAULT: &str = "RNF6011";
    /// A record format no operation names.
    pub const FORMAT_UNUSED: &str = "RNF7066";
    /// A search argument unlike the first key field.
    pub const KEY_MISMATCH: &str = "RNF7078";
    /// RPG blocks the file's records.
    pub const BLOCKED: &str = "RNF7086";
    /// A display file with a separate indicator area.
    pub const INDARA: &str = "RNF7089";
    /// `EVAL-CORR` leaves out subfields without a namesake.
    pub const CORR_IGNORED: &str = "RNF7350";
    /// An index or element count out of range at compile time.
    pub const INDEX_RANGE: &str = "RNF7404";
    /// A packed or zoned field has more than 63 digits.
    pub const TOO_MANY_DIGITS: &str = "RNF0503";
    /// A binary decimal field has more than 9 digits.
    pub const BINDEC_DIGITS: &str = "RNF0504";
    /// More decimal positions than digits.
    pub const SCALE_OVER_DIGITS: &str = "RNF0512";
    /// An integer length other than 3, 5, 10 or 20 digits.
    pub const INT_DIGITS: &str = "RNF0533";
    /// A float length other than 4 or 8 bytes.
    pub const FLOAT_LEN: &str = "RNF3358";
    /// A numeric field of zero digits.
    pub const ZERO_DIGITS: &str = "RNF3596";
    /// A character length of zero or less.
    pub const ZERO_LEN: &str = "RNF3557";
    /// A character length above the largest.
    pub const LEN_TOO_BIG: &str = "RNF0231";
    /// `DIM` outside 1 to 16773104.
    pub const DIM_RANGE: &str = "RNF3452";
    /// A date format that is not valid.
    pub const BAD_DATFMT: &str = "RNF0612";
    /// Text skipped after an error.
    pub const TEXT_SKIPPED: &str = "RNF0607";
    /// The initial value's type is not the field's.
    pub const INZ_TYPE: &str = "RNF3430";
    /// A date literal that is not valid in its format.
    pub const BAD_DATE_LIT: &str = "RNF0305";
    /// A time literal that is not valid in its format.
    pub const BAD_TIME_LIT: &str = "RNF0306";
    /// A timestamp literal that is not valid.
    pub const BAD_TIMESTAMP_LIT: &str = "RNF0307";
    /// A definition keyword that is not known.
    pub const UNKNOWN_DEF_KEYWORD: &str = "RNF3308";
    /// A keyword given twice on one definition.
    pub const KEYWORD_TWICE: &str = "RNF3312";
    /// A keyword that excludes one before it.
    pub const KEYWORD_EXCLUDED: &str = "RNF3327";
    /// A constant without a value.
    pub const CONST_NO_VALUE: &str = "RNF3905";
    /// A keyword on a named constant.
    pub const CONST_KEYWORD: &str = "RNF3901";
    /// A control keyword that is not known.
    pub const UNKNOWN_CTL_KEYWORD: &str = "RNF1301";
    /// A control keyword given twice.
    pub const CTL_KEYWORD_TWICE: &str = "RNF1302";
    /// `DFTACTGRP`, `ACTGRP` or `USRPRF` when compiling a module.
    pub const CTL_NOT_FOR_MODULE: &str = "RNF1324";
    /// A definition without a type.
    pub const NO_TYPE: &str = "RNF3438";
    /// `POS` outside 1 to 16773104.
    pub const POS_RANGE: &str = "RNF3576";
    /// A subfield ends past the structure's length.
    pub const PAST_DS_LEN: &str = "RNF7301";
    /// `OVERLAY` names no subfield.
    pub const OVERLAY_TARGET: &str = "RNF3467";
    /// `OVERLAY(ds:n)` of the structure itself.
    pub const OVERLAY_SELF: &str = "RNF3599";
    /// `DIM` on a structure that is not qualified.
    pub const DS_DIM_UNQUALIFIED: &str = "RNF3545";
    /// A qualified name whose qualifier is not a qualified structure.
    pub const BAD_QUALIFIED: &str = "RNF7591";
    /// `LIKE`'s adjustment without a sign.
    pub const LIKE_SIGN: &str = "RNF3571";
    /// An interface parameter unlike its prototype's.
    pub const PI_PARM_DIFFERS: &str = "RNF3764";
    /// A file declared twice.
    pub const FILE_TWICE: &str = "RNF2026";
    /// A file's external description is not found.
    pub const FILE_EXT_NOT_FOUND: &str = "RNF2120";
    /// A `USAGE` the device does not allow.
    pub const USAGE_DEVICE: &str = "RNF2372";
    /// `RETURN` with a value from a procedure that returns none.
    pub const RETURN_VALUE_UNWANTED: &str = "RNF5412";
    /// `RETURN` without a value from a procedure that returns one.
    pub const RETURN_VALUE_MISSING: &str = "RNF5413";
    /// A procedure that returns a value has no `RETURN` with one.
    pub const NO_RETURN: &str = "RNF5415";
    /// The assigned value's type is not the target's.
    pub const ASSIGN_TYPE: &str = "RNF7416";
    /// A condition that is not a relational expression or indicator.
    pub const NOT_LOGICAL: &str = "RNF7423";
    /// Too few arguments for a built-in function.
    pub const BIF_TOO_FEW: &str = "RNF0316";
    /// Too many arguments for a built-in function.
    pub const BIF_TOO_MANY: &str = "RNF0317";
    /// `%ELEM` of something that has no elements.
    pub const ELEM_ARG: &str = "RNF0331";
    /// `%SUBST`'s second argument is not valid.
    pub const SUBST_START: &str = "RNF0362";
    /// A token not valid here (an unknown built-in function).
    pub const BAD_TOKEN: &str = "RNF0604";
    /// An `ENDyy` missing at the end of the calculations.
    pub const END_MISSING: &str = "RNF5177";
    /// An `ENDyy` without its group.
    pub const END_UNMATCHED: &str = "RNF5141";
    /// `LEAVE` or `ITER` outside a loop.
    pub const NOT_IN_LOOP: &str = "RNF5232";
    /// `WHEN` outside a `SELECT` group (or after `OTHER`).
    pub const WHEN_OUTSIDE: &str = "RNF5249";
    /// `ON-ERROR` outside a `MONITOR` group.
    pub const ON_ERROR_OUTSIDE: &str = "RNF5295";
    /// A subroutine defined twice.
    pub const SR_TWICE: &str = "RNF5054";
    /// A subroutine without `ENDSR` before the next `BEGSR`.
    pub const ENDSR_MISSING: &str = "RNF5178";
    /// `ENDSR` without `BEGSR`.
    pub const ENDSR_UNMATCHED: &str = "RNF5016";
    /// `EXSR` of a name that is not a subroutine.
    pub const NOT_A_SUBROUTINE: &str = "RNF7018";
}
