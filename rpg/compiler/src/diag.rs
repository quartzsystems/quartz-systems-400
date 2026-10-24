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
    /// A fixed-form data type entry IBM does not know; blank is used.
    pub const BAD_TYPE_ENTRY: &str = "RNF3302";
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
    /// A result field that is not numeric where one must be.
    pub const NOT_NUMERIC_RESULT: &str = "RNF7044";
    /// `TIME`'s result field is not valid.
    pub const TIME_RESULT: &str = "RNF7056";
    /// `MOVE` into an indicator of something other than '0' or '1'.
    pub const IND_NOT_01: &str = "RNF5184";
    /// A structure moved into its own subfield, or the reverse.
    pub const OVERLAPPING: &str = "RNF7017";
    /// `MOVEA` between numeric arrays of another type or length.
    pub const MOVEA_TYPES: &str = "RNF7262";
    /// `EVALR` into a varying field.
    pub const EVALR_VARYING: &str = "RNF7565";
    /// A result field's length unlike its definition's.
    pub const LEN_DIFFERS: &str = "RNF0510";
    /// `SUBST`'s constant length reaches past its base.
    pub const SUBST_LEN_PAST: &str = "RNF5215";
    /// `SUBST`'s constant start is past its base.
    pub const SUBST_START_PAST: &str = "RNF5216";
    /// `LOOKUP`'s factor 2 is not an array or table.
    pub const LOOKUP_NOT_ARRAY: &str = "RNF5123";
    /// `LOOKUP`'s argument is not the elements' type and length.
    pub const LOOKUP_ARG_TYPE: &str = "RNF7021";
    /// A `TAG` name defined already.
    pub const TAG_TWICE: &str = "RNF5013";
    /// A `GOTO` or `CABxx` target that is not a label.
    pub const NOT_A_LABEL: &str = "RNF7043";
    /// `DIV` with a factor 2 of zero.
    pub const DIV_FACTOR2_ZERO: &str = "RNF5039";
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
    pub const EXT_NOT_FOUND: &str = "RNF3523";
    /// A feature this release does not compile.
    pub const UNSUPPORTED: &str = "RNF0201";
    /// An output record without space or skip entries spaces one after.
    pub const SPACE_DEFAULT: &str = "RNF6011";
    /// An input field past the file's record length; it is ignored.
    pub const PAST_RECORD: &str = "RNF4036";
    /// An `EXCEPT` name on an output record no `EXCEPT` operation uses.
    pub const EXCEPT_UNUSED: &str = "RNF7602";
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
    /// A time format that is not valid.
    pub const BAD_TIMFMT: &str = "RNF0614";
    /// `ACTGRP`'s parameter is not valid.
    pub const CTL_ACTGRP: &str = "RNF1317";
    /// `DATEDIT`'s parameter is not valid.
    pub const CTL_DATEDIT: &str = "RNF1008";
    /// `DECEDIT`'s parameter is not valid.
    pub const CTL_DECEDIT: &str = "RNF1303";
    /// `EXPROPTS`'s parameter is not valid.
    pub const CTL_EXPROPTS: &str = "RNF1311";
    /// `OPTION`'s parameter is not valid.
    pub const CTL_OPTION: &str = "RNF1331";
    /// `COPYRIGHT`'s parameter is not valid.
    pub const CTL_COPYRIGHT: &str = "RNF1306";
    /// `NOMAIN` when a program is created.
    pub const NOMAIN_PROGRAM: &str = "RNF1304";
    /// Main-procedure calculations in a module with `MAIN` or `NOMAIN`.
    pub const NO_MAIN_CALCS: &str = "RNF5418";
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
    /// A keyword not allowed after `ASCEND` or `DESCEND`.
    pub const AFTER_ASCEND: &str = "RNF3322";
    /// A keyword not allowed after `BASED`.
    pub const AFTER_BASED: &str = "RNF3323";
    /// A keyword not allowed after `OCCURS`.
    pub const AFTER_OCCURS: &str = "RNF3333";
    /// A keyword not allowed after `POS`.
    pub const AFTER_POS: &str = "RNF3575";
    /// A keyword without the parameter it needs.
    pub const NEEDS_PARM: &str = "RNF0603";
    /// A keyword a standalone field does not take.
    pub const NOT_FOR_FIELD: &str = "RNF3601";
    /// A keyword not allowed in the main source section.
    pub const NOT_IN_MAIN_SOURCE: &str = "RNF3760";
    /// A fixed-form keyword in a free-form declaration.
    pub const FIXED_ONLY_KEYWORD: &str = "RNF3558";
    /// `CTDATA` on something that is not an array.
    pub const CTDATA_NOT_ARRAY: &str = "RNF3422";
    /// `EXPORT` or `IMPORT` with a parameter that is not a name.
    pub const EXPORT_PARM: &str = "RNF3813";
    /// A varying field's prefix other than 2 or 4.
    pub const VARYING_PREFIX: &str = "RNF3561";
    /// A UCS-2 length of zero or less.
    pub const ZERO_LEN_UCS2: &str = "RNF3559";
    /// A graphic length of zero or less.
    pub const ZERO_LEN_GRAPH: &str = "RNF3564";
    /// Fractional seconds outside 0 to 12.
    pub const STAMP_PRECISION: &str = "RNF3415";
    /// `POINTER`'s parameter is not valid.
    pub const POINTER_PARM: &str = "RNF3568";
    /// `LIKE` names the definition itself.
    pub const LIKE_SELF: &str = "RNF3459";
    /// A qualified name not written as one.
    pub const BAD_SIMPLE_QUALIFIED: &str = "RNF0623";
    /// `OCCURS`'s parameter is not valid.
    pub const OCCURS_PARM: &str = "RNF3520";
    /// A second program status structure.
    pub const PSDS_TWICE: &str = "RNF3511";
    /// `INZ(*LIKEDS)` without `LIKEDS`.
    pub const INZ_LIKEDS: &str = "RNF3504";
    /// A subfield named twice in one structure.
    pub const SUBFIELD_TWICE: &str = "RNF3315";
    /// A template used where storage is needed.
    pub const TEMPLATE_USED: &str = "RNF0655";
    /// A constant index outside the array.
    pub const BAD_INDEX: &str = "RNF0530";
    /// An index on a name that is not an array.
    pub const NOT_ARRAY: &str = "RNF0528";
    /// A whole array where one element is needed.
    pub const ARRAY_NOT_INDEXED: &str = "RNF5343";
    /// A FOR's initial value that is not numeric.
    pub const FOR_INITIAL: &str = "RNF7567";
    /// A condition left blank.
    pub const NO_EXPRESSION: &str = "RNF5344";
    /// `OTHER` outside a `SELECT` group (or a second one).
    pub const OTHER_OUTSIDE: &str = "RNF5250";
    /// A `MONITOR` group without `ON-ERROR`.
    pub const NO_ON_ERROR: &str = "RNF5296";
    /// An `ON-ERROR` status code that is not valid.
    pub const ON_ERROR_STATUS: &str = "RNF5294";
    /// Factor 2 is missing.
    pub const NO_FACTOR2: &str = "RNF5023";
    /// The result field is missing.
    pub const NO_RESULT: &str = "RNF5030";
    /// `TEST` of a date, time or timestamp without (E) or an error
    /// indicator.
    pub const TEST_NO_ERROR: &str = "RNF5056";
    /// A result field the operation cannot take.
    pub const BAD_RESULT: &str = "RNF7261";
    /// `ENDSR`'s return point is not valid.
    pub const ENDSR_POINT: &str = "RNF5116";
    /// `SORTA` of something that is not an array.
    pub const SORTA_ARRAY: &str = "RNF5159";
    /// A call to a procedure with no return value inside an
    /// expression.
    pub const CALL_NO_VALUE: &str = "RNF5403";
    /// A built-in function's file or procedure is not defined.
    pub const BIF_NAME_UNDEFINED: &str = "RNF7451";
    /// An edit word that is not valid.
    pub const BAD_EDIT_WORD: &str = "RNF6029";
    /// A procedure keyword given twice.
    pub const PROC_KEYWORD_TWICE: &str = "RNF1506";
    /// `SERIALIZE` without `THREAD(*CONCURRENT)`.
    pub const SERIALIZE_NOT_CONCURRENT: &str = "RNF1519";
    /// A procedure interface named unlike its procedure.
    pub const PI_NAME_DIFFERS: &str = "RNF3767";
    /// `*INZSR` in a subprocedure.
    pub const INZSR_IN_PROC: &str = "RNF5425";
    /// A procedure defined twice.
    pub const PROC_TWICE: &str = "RNF1504";
    /// A keyword not valid for definitions in a procedure.
    pub const NOT_IN_PROC: &str = "RNF3757";
    /// A keyword not allowed after another on a prototype.
    pub const PR_EXCLUDED: &str = "RNF3588";
    /// `OPTIONS`'s parameter is not valid.
    pub const OPTIONS_PARM: &str = "RNF3744";
    /// A keyword not allowed after `VALUE`.
    pub const AFTER_VALUE: &str = "RNF3740";
    /// `OPTIONS(*OMIT)`, `*VARSIZE` or `*NULLIND` with `VALUE`.
    pub const OPTIONS_VALUE: &str = "RNF3753";
    /// A parameter after an `OPTIONS(*NOPASS)` one without it.
    pub const NOPASS_MISSING: &str = "RNF3756";
    /// A file keyword given twice.
    pub const FILE_KEYWORD_TWICE: &str = "RNF2416";
    /// A record length that is not valid.
    pub const BAD_RECLEN: &str = "RNF2007";
    /// A file type that is not I, O, U or C.
    pub const BAD_FILE_TYPE: &str = "RNF2003";
    /// A file format that is not F or E.
    pub const BAD_FILE_FORMAT: &str = "RNF2006";
    /// A device that is not known; DISK is used.
    pub const BAD_DEVICE: &str = "RNF2013";
    /// A file keyword that is not known.
    pub const UNKNOWN_FILE_KEYWORD: &str = "RNF2023";
    /// A file designation that is not valid; blank is used.
    pub const BAD_DESIGNATION: &str = "RNF2079";
    /// A keyword a printer file does not take.
    pub const NOT_FOR_PRINTER: &str = "RNF2403";
    /// `USAGE`'s parameter is not valid.
    pub const USAGE_PARM: &str = "RNF2370";
    /// `WRITE` of a program-described file without a structure.
    pub const WRITE_NO_DS: &str = "RNF5191";
    /// The operation is not allowed for the file.
    pub const OP_NOT_FOR_FILE: &str = "RNF5202";
    /// A length entry that is not valid.
    pub const BAD_LEN_ENTRY: &str = "RNF3402";
    /// A to-position before the from-position.
    pub const TO_BEFORE_FROM: &str = "RNF3716";
    /// A built-in function's parameter that is not valid.
    pub const BIF_PARM: &str = "RNF0336";
    /// `%DEC`'s digits are not valid.
    pub const DEC_DIGITS: &str = "RNF0328";
    /// `%DEC`'s decimal positions are not valid.
    pub const DEC_SCALE: &str = "RNF0329";
    /// `%TRIM`'s parameter is not valid.
    pub const TRIM_PARM: &str = "RNF0371";
    /// `%SUBST`'s constant start is past the string.
    pub const SUBST_START_LONG: &str = "RNF0364";
    /// `%CHAR`'s first parameter is not valid.
    pub const CHAR_PARM: &str = "RNF0386";
    /// A duration built-in function's parameter is not a number.
    pub const DURATION_PARM: &str = "RNF0579";
    /// `%LOOKUPLT`, `LE`, `GT`, `GE` of an array without `ASCEND` or
    /// `DESCEND`.
    pub const LOOKUP_UNSEQUENCED: &str = "RNF0592";
    /// A duration of years, months or days with a time.
    pub const TIME_DURATION: &str = "RNF7585";
    /// `%SUBST`'s length is too big.
    pub const SUBST_LEN: &str = "RNF0365";
    /// `%SCAN`'s first parameter is not character.
    pub const SCAN_PARM: &str = "RNF0345";
    /// `%REPLACE`'s first parameter is not valid.
    pub const REPLACE_PARM: &str = "RNF0381";
    /// `%LOOKUP`'s second parameter is not an array.
    pub const LOOKUP_ARRAY: &str = "RNF0571";
    /// `%DIFF`'s first parameter is not valid.
    pub const DIFF_FIRST: &str = "RNF0580";
    /// `%DIFF`'s second parameter is not valid.
    pub const DIFF_SECOND: &str = "RNF0581";
    /// `%ADDR`'s parameter is not valid.
    pub const ADDR_PARM: &str = "RNF0341";
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
    /// A to-position past the structure's length; the from-position is
    /// used.
    pub const TO_PAST_DS: &str = "RNF3713";
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
    /// `EXTPROC`'s procedure name is not valid; the keyword is ignored.
    pub const EXTPROC_PARM: &str = "RNF3746";
    /// `EXTPGM`'s parameter is not valid; it is ignored.
    pub const EXTPGM_PARM: &str = "RNF3747";
    /// A prototype without `EXTPGM` in the default activation group.
    pub const NEEDS_NO_DFTACTGRP: &str = "RNF3788";
    /// The main interface's program is not the one being created.
    pub const MAIN_PGM_DIFFERS: &str = "RNF3776";
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
    /// An `ENDyy` for another group than the open one; `END` is assumed.
    pub const END_OUT_OF_SEQUENCE: &str = "RNF5248";
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
    /// `CASxx` of a name that is not a subroutine.
    pub const CAS_NOT_SUBROUTINE: &str = "RNF7027";
    /// A `CASxx` group without `ENDCS`; one is assumed.
    pub const ENDCS_MISSING: &str = "RNF5094";
    /// `EXSR` of a name that is not a subroutine.
    pub const NOT_A_SUBROUTINE: &str = "RNF7018";
}
