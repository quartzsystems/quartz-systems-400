// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The syntax tree both source forms parse into. A fixed-form
//! specification and its free-form declaration give the same nodes:
//! a definition keeps its column entries (`Fixed`) or its type keyword
//! among the keywords, and semantic analysis reads either. Every
//! calculation is an `Op` with IBM's operand roles (factor 1, factor
//! 2, result, extended factor 2).

use crate::lex::Tok;

/// A place in the source: the listing's statement (line) number and
/// the column, both 1-based; column 0 when unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pos {
    /// Listing line number.
    pub line: u32,
    /// Column in that line.
    pub col: u16,
}

/// A whole compile unit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Module {
    /// Control keywords (`H` specs and `CTL-OPT`), in order.
    pub ctl: Vec<Keyword>,
    /// Global file declarations.
    pub files: Vec<FileDecl>,
    /// Global definitions.
    pub defs: Vec<Def>,
    /// Input specifications.
    pub inputs: Vec<Input>,
    /// The main procedure's calculations (cycle or linear main).
    pub calcs: Vec<Stmt>,
    /// Output specifications.
    pub outputs: Vec<Output>,
    /// Subprocedures.
    pub procs: Vec<Proc>,
    /// Compile-time data sections.
    pub ctdata: Vec<CtData>,
    /// Whether the member was `**FREE`.
    pub fully_free: bool,
}

/// `NAME(arg:arg...)` in a keyword area.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyword {
    /// Upper case.
    pub name: String,
    /// The arguments, split at top-level colons.
    pub args: Vec<Arg>,
    /// Whether parentheses were written (`CONST` vs `CONST()`).
    pub parens: bool,
    /// Where the keyword is.
    pub at: Pos,
}

/// One keyword argument: its tokens, and the expression they form
/// when they form one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arg {
    /// The tokens.
    pub toks: Vec<Tok>,
    /// The parsed expression.
    pub expr: Option<Expr>,
}

impl Keyword {
    /// The argument at `i`, if any.
    #[must_use]
    pub fn arg(&self, i: usize) -> Option<&Arg> {
        self.args.get(i)
    }
}

/// A file declaration (`F` spec or `DCL-F`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileDecl {
    /// The file name as written.
    pub name: String,
    /// The fixed-form column entries, for an `F` spec.
    pub fixed: Option<FileCols>,
    /// The keywords.
    pub kws: Vec<Keyword>,
    /// Where it is declared.
    pub at: Pos,
}

/// The column entries of an `F` spec, trimmed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileCols {
    /// Column 17: `I O U C`.
    pub ftype: String,
    /// Column 18: `P S R T F` or blank.
    pub designation: String,
    /// Column 19: `E`.
    pub eof: String,
    /// Column 20: `A`.
    pub addition: String,
    /// Column 21: `A D`.
    pub sequence: String,
    /// Column 22: `E F`.
    pub format: String,
    /// Columns 23-27.
    pub reclen: String,
    /// Column 28: `L`.
    pub limits: String,
    /// Columns 29-33.
    pub keylen: String,
    /// Column 34: `A D K P G T Z F`.
    pub rectype: String,
    /// Column 35: `I T`.
    pub org: String,
    /// Columns 36-42.
    pub device: String,
}

/// What a definition is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DefKind {
    /// A standalone field (`S`, `DCL-S`).
    #[default]
    Standalone,
    /// A named constant (`C`, `DCL-C`).
    Const,
    /// A data structure (`DS`, `DCL-DS`).
    Ds,
    /// A subfield of a data structure.
    Subfield,
    /// A prototype (`PR`, `DCL-PR`).
    Pr,
    /// A procedure interface (`PI`, `DCL-PI`).
    Pi,
    /// A parameter of a prototype or interface.
    Parm,
    /// An enumeration (`DCL-ENUM`).
    Enum,
}

/// The column entries of a `D` spec, trimmed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefCols {
    /// Column 22: `E` for externally described.
    pub ext: String,
    /// Column 23: `S` program status, `U` data area.
    pub dstype: String,
    /// Columns 26-32.
    pub from: String,
    /// Columns 33-39: the end position or the length.
    pub to: String,
    /// Column 40: the data type.
    pub ty: String,
    /// Columns 41-42: decimal positions.
    pub decimals: String,
}

/// A definition: standalone field, constant, data structure,
/// subfield, prototype, interface or parameter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Def {
    /// What it is.
    pub kind: DefKind,
    /// The name as written; `*N` for an unnamed one.
    pub name: String,
    /// The column entries, for a `D` spec.
    pub fixed: Option<DefCols>,
    /// The keywords; a free-form type (`PACKED(7:2)`) is one of them.
    pub kws: Vec<Keyword>,
    /// A constant's value (`DCL-C NAME value;` or `CONST(value)`).
    pub value: Option<Expr>,
    /// Subfields, parameters or enumeration constants.
    pub children: Vec<Def>,
    /// Where it is declared.
    pub at: Pos,
}

impl Def {
    /// A keyword by name.
    #[must_use]
    pub fn kw(&self, name: &str) -> Option<&Keyword> {
        self.kws.iter().find(|k| k.name == name)
    }
}

/// An indicator condition: negation and the indicator (`01`, `LR`,
/// `KA`, ...).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CondInd {
    /// `N` was written.
    pub not: bool,
    /// The indicator, upper case.
    pub ind: String,
}

/// Conditioning: alternatives (OR) of conjunctions (AND).
pub type Conditioning = Vec<Vec<CondInd>>;

/// A calculation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stmt {
    /// Where the statement starts.
    pub at: Pos,
    /// Control level (`L0`-`L9`, `LR`, `SR`), fixed form only.
    pub level: String,
    /// Conditioning indicators, fixed form only.
    pub cond: Conditioning,
    /// The operation.
    pub op: Op,
}

/// An operation with its operands.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Op {
    /// The operation code, upper case, without extenders (`EVAL`,
    /// `CHAIN`, `IFEQ`, `ADD`); the implicit free-form `EVAL` and
    /// `CALLP` are written out.
    pub code: String,
    /// The extender letters, upper case (`E`, `HN`, ...).
    pub ext: String,
    /// Factor 1, split at top-level colons.
    pub f1: Vec<Expr>,
    /// Factor 2, split at top-level colons.
    pub f2: Vec<Expr>,
    /// The result field.
    pub result: Vec<Expr>,
    /// The extended factor 2: an expression, or an assignment.
    pub xf2: Option<Expr>,
    /// Columns 64-68, the result's length when it is defined here.
    pub len: String,
    /// Columns 69-70.
    pub decimals: String,
    /// Resulting indicators: high, low, equal (columns 71-76).
    pub resind: [String; 3],
    /// Extra operands of free-form operations that have no factor
    /// slot (`FOR`'s `TO`/`BY`, `SORTA`'s field list...).
    pub more: Vec<(String, Expr)>,
}

/// A figurative constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fig {
    /// `*BLANK`, `*BLANKS`.
    Blanks,
    /// `*ZERO`, `*ZEROS`.
    Zeros,
    /// `*HIVAL`.
    Hival,
    /// `*LOVAL`.
    Loval,
    /// `*ON`.
    On,
    /// `*OFF`.
    Off,
    /// `*NULL`.
    Null,
    /// `*ALL'x..'`.
    All(String),
    /// `*ALLX'..'`.
    AllX(String),
    /// `*ALLG'..'`.
    AllG(String),
}

/// A literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lit {
    /// A numeric literal as written (`123`, `-5.20`, `1.5E3`).
    Num(String),
    /// A character literal (quotes undoubled).
    Char(String),
    /// `X'..'`: the hexadecimal digits.
    Hex(String),
    /// `G'..'`.
    Graph(String),
    /// `U'..'`: four hexadecimal digits per character.
    Ucs2(String),
    /// `D'..'`.
    Date(String),
    /// `T'..'`.
    Time(String),
    /// `Z'..'`.
    Timestamp(String),
}

/// A step after a name: an index or argument list, or a qualified
/// subfield.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// `(args)`.
    Args(Vec<Expr>),
    /// `.name`.
    Field(String),
}

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// `**`.
    Pow,
    /// `*`.
    Mul,
    /// `/`.
    Div,
    /// `+`.
    Add,
    /// `-`.
    Sub,
    /// `=`.
    Eq,
    /// `<>`.
    Ne,
    /// `<`.
    Lt,
    /// `>`.
    Gt,
    /// `<=`.
    Le,
    /// `>=`.
    Ge,
    /// `AND`.
    And,
    /// `OR`.
    Or,
    /// `IN` (`x IN %LIST(...)`, `x IN %RANGE(...)`, `FOR-EACH x IN ...`).
    In,
}

/// Unary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnOp {
    /// `-`.
    Neg,
    /// `+`.
    Plus,
    /// `NOT`.
    Not,
}

/// Assignment operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssignOp {
    /// `=`.
    Set,
    /// `+=`.
    Add,
    /// `-=`.
    Sub,
    /// `*=`.
    Mul,
    /// `/=`.
    Div,
    /// `**=`.
    Pow,
}

/// An expression.
#[derive(Clone, Debug, PartialEq)]
pub enum Expr {
    /// A name and its steps: `X`, `ARR(I)`, `DS.SUB`, `DS(2).SUB`,
    /// `PROC(A:B)`; the name upper case.
    Ref {
        /// The first name, upper case.
        name: String,
        /// The steps after it.
        steps: Vec<Step>,
        /// Where it is.
        at: Pos,
    },
    /// A special word: `*IN`, `*INLR`, `*IN01`, `*START`, `*OMIT`,
    /// `*ISO`, ... (upper case, with the star), with an index for
    /// `*IN(n)`.
    Special {
        /// The word.
        word: String,
        /// `*IN(n)`'s index.
        index: Option<Box<Expr>>,
        /// Where it is.
        at: Pos,
    },
    /// A built-in function: `%TRIM(X)`; the name upper case with `%`.
    Bif {
        /// The name.
        name: String,
        /// The arguments.
        args: Vec<Expr>,
        /// Where it is.
        at: Pos,
    },
    /// A literal.
    Lit(Lit, Pos),
    /// A figurative constant.
    Fig(Fig, Pos),
    /// A unary operation.
    Unary(UnOp, Box<Expr>, Pos),
    /// A binary operation.
    Binary(BinOp, Box<Expr>, Box<Expr>, Pos),
    /// An assignment (the extended factor 2 of `EVAL`, `EVALR`,
    /// `EVAL-CORR` and the implicit free-form `EVAL`).
    Assign(AssignOp, Box<Expr>, Box<Expr>, Pos),
    /// A parenthesized list `(a:b:c)`: a key list or a `%LIST`-like
    /// group.
    List(Vec<Expr>, Pos),
    /// Nothing written where an operand may be omitted (`f(a::c)`).
    Empty(Pos),
}

impl Expr {
    /// Where the expression is.
    #[must_use]
    pub fn pos(&self) -> Pos {
        match self {
            Self::Ref { at, .. } | Self::Special { at, .. } | Self::Bif { at, .. } => *at,
            Self::Lit(_, p)
            | Self::Fig(_, p)
            | Self::Unary(_, _, p)
            | Self::Binary(_, _, _, p)
            | Self::Assign(_, _, _, p)
            | Self::List(_, p)
            | Self::Empty(p) => *p,
        }
    }

    /// The bare name when the expression is a plain name.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Ref { name, steps, .. } if steps.is_empty() => Some(name),
            _ => None,
        }
    }
}

/// A subprocedure.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Proc {
    /// The name as written.
    pub name: String,
    /// The `P` spec / `DCL-PROC` keywords (`EXPORT`, `SERIALIZE`...).
    pub kws: Vec<Keyword>,
    /// The interface, when given.
    pub pi: Option<Def>,
    /// Local files.
    pub files: Vec<FileDecl>,
    /// Local definitions.
    pub defs: Vec<Def>,
    /// The calculations.
    pub calcs: Vec<Stmt>,
    /// Where it begins.
    pub at: Pos,
    /// Where it ends.
    pub end: Pos,
}

/// An input specification: a record line and its fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Input {
    /// File (program-described) or record format (external) name.
    pub name: String,
    /// Whether it is an external record line.
    pub external: bool,
    /// Sequence (columns 17-18).
    pub sequence: String,
    /// Number (column 19).
    pub number: String,
    /// Option (column 20).
    pub option: String,
    /// Record identifying indicator (columns 21-22).
    pub rid: String,
    /// Record identification codes: position, not, part, character;
    /// with the `AND`/`OR` lines that follow, as alternatives.
    pub codes: Vec<Vec<RecCode>>,
    /// The fields.
    pub fields: Vec<InputField>,
    /// Where it is.
    pub at: Pos,
}

/// One record identification code.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecCode {
    /// The position in the record.
    pub pos: u32,
    /// `N` was written.
    pub not: bool,
    /// `C` character, `Z` zone, `D` digit.
    pub part: char,
    /// The character.
    pub ch: char,
}

/// An input field line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputField {
    /// The field (or, external, the new name).
    pub name: String,
    /// External field name (externally described files).
    pub extname: String,
    /// Data attributes (columns 31-34), separator (35).
    pub attr: String,
    /// Date/time separator.
    pub sep: String,
    /// Data format (column 36).
    pub format: String,
    /// From position.
    pub from: String,
    /// To position.
    pub to: String,
    /// Decimal positions.
    pub decimals: String,
    /// Control level.
    pub level: String,
    /// Matching field.
    pub matching: String,
    /// Field record relation.
    pub relation: String,
    /// Field indicators: plus, minus, zero or blank.
    pub inds: [String; 3],
    /// Where it is.
    pub at: Pos,
}

/// An output record and its fields.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// File or record format name.
    pub name: String,
    /// `H D T E`.
    pub kind: String,
    /// `F` fetch overflow, `R` release, `ADD`, `DEL`.
    pub fetch: String,
    /// Conditioning (with `AND`/`OR` lines).
    pub cond: Conditioning,
    /// The `EXCEPT` name.
    pub except: String,
    /// Space before, space after, skip before, skip after.
    pub space: [String; 4],
    /// The fields.
    pub fields: Vec<OutputField>,
    /// Where it is.
    pub at: Pos,
}

/// An output field line.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OutputField {
    /// Conditioning.
    pub cond: Conditioning,
    /// The field name, `PAGE`, `*PLACE`, `UDATE`, `*ALL`..., or empty
    /// for a constant.
    pub field: String,
    /// The edit code.
    pub edit: String,
    /// `B` blank after.
    pub blank: bool,
    /// The end position (`+n` relative allowed), as written.
    pub end: String,
    /// The data format (column 52).
    pub format: String,
    /// The constant or edit word.
    pub constant: Option<Expr>,
    /// Where it is.
    pub at: Pos,
}

/// A compile-time data section (`**CTDATA name`, `**FTRANS`,
/// `**ALTSEQ`, or an unnamed `**`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtData {
    /// `CTDATA`, `FTRANS`, `ALTSEQ` or empty.
    pub kind: String,
    /// The array or table name for `**CTDATA`.
    pub name: String,
    /// The records, as written (columns 1-100).
    pub records: Vec<String>,
    /// Where the `**` line is.
    pub at: Pos,
}
