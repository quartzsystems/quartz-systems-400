// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The identifiers semantic analysis and code generation raise, by
//! meaning. The front end's are in `rpgfront::diag::ids`.

/// The identifiers.
pub mod ids {
    /// A name or indicator used but never declared.
    pub const NOT_DEFINED: &str = "RNF7030";
    /// An expression holds an operand that is not defined.
    pub const UNDEFINED_OPERAND: &str = "RNF7503";
    /// The name is defined more than once.
    pub const DUPLICATE: &str = "RNF2120";
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
    pub const BAD_FILE_OP: &str = "RNF7028";
    /// The built-in function is not known.
    pub const UNKNOWN_BIF: &str = "RNF7013";
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
    /// An index or element count out of range at compile time.
    pub const INDEX_RANGE: &str = "RNF7404";
}
