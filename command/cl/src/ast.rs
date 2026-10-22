// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The statements and expressions of a CL program as the parser reads
//! them. Every statement keeps its source line and text for the
//! listing and the debug views.

use cmd::token::Tok;

/// A parsed program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Program {
    /// The statements, in source order.
    pub stmts: Vec<Stmt>,
}

/// One statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stmt {
    /// Statement number (1-based, in source order).
    pub n: usize,
    /// First source line.
    pub line: usize,
    /// The label before it.
    pub label: Option<String>,
    /// The source text, comments removed, continuations joined.
    pub text: String,
    /// What it is.
    pub kind: Kind,
}

/// A CL variable type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VarType {
    /// `*CHAR`.
    Char,
    /// `*DEC`.
    Dec,
    /// `*LGL`.
    Lgl,
    /// `*INT`.
    Int,
    /// `*UINT`.
    Uint,
    /// `*PTR`.
    Ptr,
}

/// `STG`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stg {
    /// `*AUTO`.
    Auto,
    /// `*DEFINED` over `DEFVAR(&VAR pos)`.
    Defined(String, u32),
    /// `*BASED` on `BASPTR(&PTR)`.
    Based(String),
}

/// A literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lit {
    /// `'text'`.
    Str(String),
    /// A number as written.
    Num(String),
    /// `X'..'`.
    Hex(Vec<u8>),
}

/// `DCL`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decl {
    /// The variable, upper case, without `&`.
    pub var: String,
    /// Its type.
    pub ty: VarType,
    /// `LEN`: characters or digits.
    pub len: Option<u32>,
    /// Decimal positions.
    pub dec: Option<u8>,
    /// `VALUE`.
    pub value: Option<Lit>,
    /// `STG`.
    pub stg: Stg,
}

/// `DCLF`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DclF {
    /// The file.
    pub file: String,
    /// Its library.
    pub lib: String,
    /// `RCDFMT`: the formats declared, `*ALL` when empty.
    pub rcdfmt: Vec<String>,
    /// `OPNID`.
    pub opnid: Option<String>,
}

/// An operator of an expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `+`.
    Add,
    /// `-`.
    Sub,
    /// `*`.
    Mul,
    /// `/`.
    Div,
    /// `*CAT`.
    Cat,
    /// `*BCAT`.
    Bcat,
    /// `*TCAT`.
    Tcat,
    /// `*EQ`.
    Eq,
    /// `*NE`.
    Ne,
    /// `*GT`.
    Gt,
    /// `*LT`.
    Lt,
    /// `*GE`.
    Ge,
    /// `*LE`.
    Le,
    /// `*NG`.
    Ng,
    /// `*NL`.
    Nl,
    /// `*AND`.
    And,
    /// `*OR`.
    Or,
}

/// An expression.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// A literal.
    Lit(Lit),
    /// `&VAR`.
    Var(String),
    /// `*ON`, `*OFF` or another unquoted word taken as text.
    Word(String),
    /// `a op b`.
    Bin(Op, Box<Expr>, Box<Expr>),
    /// `*NOT a`.
    Not(Box<Expr>),
    /// `-a`.
    Neg(Box<Expr>),
    /// `%NAME(args)`.
    Builtin(String, Vec<Expr>),
}

/// Where `CHGVAR` and the receivers put a value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// `&VAR`.
    Var(String),
    /// `%SST(&VAR start len)`.
    Sst(String, Box<Expr>, Box<Expr>),
    /// `%BIN(&VAR start len)`.
    Bin(String, Box<Expr>, Box<Expr>),
}

impl Target {
    /// The variable.
    #[must_use]
    pub fn var(&self) -> &str {
        match self {
            Self::Var(v) | Self::Sst(v, ..) | Self::Bin(v, ..) => v,
        }
    }
}

/// A `CALL` parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallParm {
    /// A variable, by reference.
    Var(String),
    /// A constant.
    Lit(Lit),
    /// `*N`.
    Omit,
    /// An expression, passed in a temporary.
    Expr(Expr),
}

/// A `CALLPRC` parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrcParm {
    /// The value.
    pub value: Expr,
    /// `*BYVAL`.
    pub byval: bool,
}

/// What a statement is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `PGM PARM(...)`.
    Pgm(Vec<String>),
    /// `ENDPGM`.
    EndPgm,
    /// `DCL`.
    Dcl(Decl),
    /// `DCLF`.
    DclF(DclF),
    /// `DCLPRCOPT`: the keywords as given.
    DclPrcOpt(Vec<(String, Vec<Tok>)>),
    /// `CHGVAR`.
    ChgVar(Target, Expr),
    /// `IF COND THEN`.
    If(Expr, Option<Box<Stmt>>),
    /// `ELSE CMD`.
    Else(Option<Box<Stmt>>),
    /// `DO`.
    Do,
    /// `ENDDO`.
    EndDo,
    /// `DOWHILE`.
    DoWhile(Expr),
    /// `DOUNTIL`.
    DoUntil(Expr),
    /// `DOFOR VAR FROM TO BY`.
    DoFor(String, Expr, Expr, Expr),
    /// `LEAVE CMDLBL`.
    Leave(Option<String>),
    /// `ITERATE CMDLBL`.
    Iterate(Option<String>),
    /// `SELECT`.
    Select,
    /// `WHEN COND THEN`.
    When(Expr, Option<Box<Stmt>>),
    /// `OTHERWISE CMD`.
    Otherwise(Option<Box<Stmt>>),
    /// `ENDSELECT`.
    EndSelect,
    /// `GOTO`.
    Goto(String),
    /// `CALL PGM PARM`: the program as a name or a variable, its
    /// library, the parameters.
    Call(CallTarget, Vec<CallParm>),
    /// `TFRCTL`.
    TfrCtl(CallTarget, Vec<CallParm>),
    /// `CALLPRC PRC PARM RTNVAL`.
    CallPrc(String, Vec<PrcParm>, Option<Target>),
    /// `RETURN`.
    Return,
    /// `MONMSG MSGID CMPDTA EXEC`.
    MonMsg(Vec<String>, Option<String>, Option<Box<Stmt>>),
    /// `SUBR`.
    Subr(String),
    /// `ENDSUBR RTNVAL`.
    EndSubr(Option<Expr>),
    /// `CALLSUBR SUBR RTNVAL`.
    CallSubr(String, Option<Target>),
    /// `RTNSUBR RTNVAL`.
    RtnSubr(Option<Expr>),
    /// `RCVF`, `SNDF`, `SNDRCVF`, `WAIT`, `ENDRCV`: the operation, the
    /// `OPNID`, the record format.
    FileIo(FileOp, Option<String>, Option<String>),
    /// `CLOSE OPNID`.
    Close(Option<String>),
    /// `RTVDTAARA DTAARA((lib/name (start len))) RTNVAR(&V)`.
    RtvDtaAra(DtaAra, Target),
    /// `CHGDTAARA DTAARA(...) VALUE(expr)`.
    ChgDtaAra(DtaAra, Expr),
    /// `DMPCLPGM`.
    DmpClPgm,
    /// Any other command, analyzed against its definition.
    Command(cmd::token::Tokens),
}

/// A display or database file operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOp {
    /// `RCVF`.
    Rcvf,
    /// `SNDF`.
    Sndf,
    /// `SNDRCVF`.
    SndRcvf,
    /// `WAIT`.
    Wait,
    /// `ENDRCV`.
    EndRcv,
}

/// How `CALL` names its program.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallTarget {
    /// `LIB/NAME`.
    Name(String, String),
    /// `&VAR` holding the name (10) or the name and library (20).
    Var(String),
}

/// A data area and the substring of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DtaAra {
    /// The name, or `*LDA`, `*GDA`, `*PDA`.
    pub name: String,
    /// The library.
    pub lib: String,
    /// `(start len)`.
    pub sub: Option<(Expr, Expr)>,
}
