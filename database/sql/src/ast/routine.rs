// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Routines: `CREATE PROCEDURE`, `CREATE FUNCTION`, `CALL`, `DROP` of
//! either, and the SQL PL statements of a routine body.

use super::{Expr, Ident, ObjectName, Statement, TypeSpec};

/// `IN`, `OUT` or `INOUT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParmMode {
    /// Passed in.
    In,
    /// Passed back.
    Out,
    /// Both.
    InOut,
}

impl ParmMode {
    /// The catalog's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::In => "IN",
            Self::Out => "OUT",
            Self::InOut => "INOUT",
        }
    }
}

/// One parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Parm {
    /// Its mode (functions: always `IN`).
    pub mode: ParmMode,
    /// Its name; external routines may leave it out.
    pub name: Option<Ident>,
    /// Its type.
    pub ty: TypeSpec,
}

/// Procedure or function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoutineKind {
    /// `CREATE PROCEDURE`.
    Procedure,
    /// `CREATE FUNCTION`.
    Function,
}

/// `LANGUAGE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Language {
    /// SQL (the body is SQL PL).
    Sql,
    /// ILE RPG.
    Rpgle,
    /// RPG/400.
    Rpg,
    /// ILE CL.
    Cl,
    /// C.
    C,
    /// COBOL.
    Cobol,
    /// ILE COBOL.
    Cobolle,
    /// Java.
    Java,
    /// PL/I.
    Pli,
}

impl Language {
    /// The catalog's word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Sql => "SQL",
            Self::Rpgle => "RPGLE",
            Self::Rpg => "RPG",
            Self::Cl => "CL",
            Self::C => "C",
            Self::Cobol => "COBOL",
            Self::Cobolle => "COBOLLE",
            Self::Java => "JAVA",
            Self::Pli => "PLI",
        }
    }
}

/// `PARAMETER STYLE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParmStyle {
    /// `SQL`: arguments, indicators, SQLSTATE, names and the message.
    Sql,
    /// `GENERAL`: the arguments only.
    General,
    /// `GENERAL WITH NULLS`: the arguments and an indicator array.
    GeneralNulls,
    /// `DB2GENERAL` (Java).
    Db2General,
    /// `JAVA`.
    Java,
    /// `DB2SQL`: `SQL` with the call type and scratchpad.
    Db2Sql,
}

impl ParmStyle {
    /// The catalog's word (`SYSROUTINES.PARAMETER_STYLE`).
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Sql => "SQL",
            Self::General => "GENERAL",
            Self::GeneralNulls => "GENERAL WITH NULLS",
            Self::Db2General => "DB2GENERAL",
            Self::Java => "JAVA",
            Self::Db2Sql => "DB2SQL",
        }
    }
}

/// `NO SQL`, `CONTAINS SQL`, `READS SQL DATA`, `MODIFIES SQL DATA`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SqlAccess {
    /// `NO SQL`.
    NoSql,
    /// `CONTAINS SQL`.
    Contains,
    /// `READS SQL DATA`.
    Reads,
    /// `MODIFIES SQL DATA`.
    Modifies,
}

impl SqlAccess {
    /// The catalog's word (`SQL_DATA_ACCESS`).
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::NoSql => "NONE",
            Self::Contains => "CONTAINS",
            Self::Reads => "READS",
            Self::Modifies => "MODIFIES",
        }
    }
}

/// The clauses after the parameters, as written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the routine's clauses
pub struct RoutineOptions {
    /// `SPECIFIC name`.
    pub specific: Option<ObjectName>,
    /// `LANGUAGE`.
    pub language: Option<Language>,
    /// `PARAMETER STYLE`.
    pub style: Option<ParmStyle>,
    /// `EXTERNAL NAME`, as written (`LIB/PGM`, `LIB/SRVPGM(PROC)`).
    pub external: Option<String>,
    /// `EXTERNAL` given (with or without a name).
    pub is_external: bool,
    /// `DETERMINISTIC` (`Some(true)`) or `NOT DETERMINISTIC`.
    pub deterministic: Option<bool>,
    /// The SQL data access.
    pub access: Option<SqlAccess>,
    /// `DYNAMIC RESULT SETS n`.
    pub result_sets: u16,
    /// `COMMIT ON RETURN YES`.
    pub commit_on_return: bool,
    /// `PROGRAM TYPE SUB`.
    pub program_sub: bool,
    /// `RETURNS NULL ON NULL INPUT` (false: `CALLED ON NULL INPUT`).
    pub null_input: bool,
    /// `NO EXTERNAL ACTION`.
    pub no_external_action: bool,
    /// `FENCED` (false) or `NOT FENCED`.
    pub not_fenced: bool,
    /// `ALLOW PARALLEL` (false: `DISALLOW PARALLEL`).
    pub parallel: bool,
    /// `FINAL CALL`.
    pub final_call: bool,
    /// `SCRATCHPAD n`.
    pub scratchpad: Option<u32>,
    /// `DBINFO`.
    pub dbinfo: bool,
    /// `SET OPTION` values, as written.
    pub set_options: Vec<(String, String)>,
    /// `NEW SAVEPOINT LEVEL`.
    pub new_savepoint_level: bool,
    /// `INHERIT SPECIAL REGISTERS` (true) or `DEFAULT SPECIAL REGISTERS`.
    pub default_registers: bool,
    /// `STATEMENT DETERMINISTIC`, `CARDINALITY`, `NOT SECURED` and the
    /// other clauses kept for the catalogs only, as written.
    pub other: Vec<String>,
}

/// What a function returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Returns {
    /// A scalar, `CAST FROM` the external type when given.
    Scalar(TypeSpec, Option<TypeSpec>),
    /// `RETURNS TABLE (column type, ...)`.
    Table(Vec<(Ident, TypeSpec)>),
}

/// A routine's body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutineBody {
    /// SQL PL: one statement, usually a compound one.
    Sql(Box<PlStmt>),
    /// An external program or service program procedure.
    External,
    /// `SOURCE name [(types)]`, a sourced function.
    Sourced(ObjectName, Option<Vec<TypeSpec>>),
}

/// `CREATE [OR REPLACE] PROCEDURE|FUNCTION`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreateRoutine {
    /// Which.
    pub kind: RoutineKind,
    /// `OR REPLACE`.
    pub replace: bool,
    /// The routine's name.
    pub name: ObjectName,
    /// Its parameters.
    pub parms: Vec<Parm>,
    /// For a function, what it returns.
    pub returns: Option<Returns>,
    /// The clauses.
    pub options: RoutineOptions,
    /// The body.
    pub body: RoutineBody,
    /// The routine body as written (`SYSROUTINES.ROUTINE_DEFINITION`).
    pub body_text: String,
    /// The whole statement as written.
    pub text: String,
}

/// `CALL name (args)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Call {
    /// The procedure.
    pub name: ObjectName,
    /// The arguments.
    pub args: Vec<Expr>,
}

/// `DROP PROCEDURE|FUNCTION|SPECIFIC ...`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DropRoutine {
    /// Which.
    pub kind: RoutineKind,
    /// `SPECIFIC`: the name is a specific name.
    pub specific: bool,
    /// The name.
    pub name: ObjectName,
    /// The signature, when given.
    pub signature: Option<Vec<TypeSpec>>,
}

/// `SIGNAL` or `RESIGNAL`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signal {
    /// `SQLSTATE [VALUE] 'xxxxx'` or a condition name; `None` for a
    /// bare `RESIGNAL`.
    pub state: Option<SignalValue>,
    /// `SET MESSAGE_TEXT = expr`.
    pub message: Option<PlExpr>,
    /// `RESIGNAL`.
    pub resignal: bool,
}

/// The condition a `SIGNAL` raises.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SignalValue {
    /// A literal SQLSTATE.
    State(String),
    /// A declared condition name.
    Condition(Ident),
    /// A variable holding the SQLSTATE.
    Variable(PlExpr),
}

/// An expression inside a routine, with its text: the compiler
/// rewrites the text with the variables as parameter markers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlExpr {
    /// The expression as written.
    pub text: String,
    /// As parsed.
    pub expr: Expr,
}

/// An SQL statement inside a routine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlSql {
    /// The statement as written, without an `INTO` clause.
    pub text: String,
    /// As parsed.
    pub stmt: Statement,
    /// `SELECT|VALUES ... INTO` targets.
    pub into: Vec<Ident>,
}

/// A handler's condition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HandlerCond {
    /// `SQLEXCEPTION`.
    Exception,
    /// `SQLWARNING`.
    Warning,
    /// `NOT FOUND`.
    NotFound,
    /// `SQLSTATE 'xxxxx'`.
    State(String),
    /// A declared condition.
    Name(Ident),
}

/// `CONTINUE`, `EXIT` or `UNDO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandlerKind {
    /// Carries on after the statement that raised the condition.
    Continue,
    /// Leaves the compound statement.
    Exit,
    /// Undoes the compound's changes, then leaves it.
    Undo,
}

/// `DECLARE ... HANDLER`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Handler {
    /// What it does when done.
    pub kind: HandlerKind,
    /// The conditions it handles.
    pub conds: Vec<HandlerCond>,
    /// Its statement.
    pub body: Box<PlStmt>,
}

/// Where a cursor's result set goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnTo {
    /// `WITH RETURN [TO CALLER]`.
    Caller,
    /// `WITH RETURN TO CLIENT`.
    Client,
}

/// A cursor declared in a compound statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlCursor {
    /// Its name.
    pub name: Ident,
    /// `WITH HOLD`.
    pub hold: bool,
    /// `SCROLL`.
    pub scroll: bool,
    /// `WITH RETURN`.
    pub returns: Option<ReturnTo>,
    /// The query, or the prepared statement it is for.
    pub query: CursorSource,
}

/// What a cursor reads.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // an AST
pub enum CursorSource {
    /// A query in the text.
    Query(PlSql),
    /// A statement prepared under that name.
    Statement(Ident),
}

/// A variable declaration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlVar {
    /// The names it declares.
    pub names: Vec<Ident>,
    /// Their type.
    pub ty: TypeSpec,
    /// An array type's name instead (`ty` unused).
    pub array: Option<super::ObjectName>,
    /// `DEFAULT expr` (`None`: null).
    pub default: Option<PlExpr>,
    /// `CONSTANT`.
    pub constant: bool,
}

/// `BEGIN [ATOMIC] ... END`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compound {
    /// `ATOMIC`.
    pub atomic: bool,
    /// Variables, in order.
    pub vars: Vec<PlVar>,
    /// `DECLARE name CONDITION FOR SQLSTATE 'xxxxx'`.
    pub conditions: Vec<(Ident, String)>,
    /// Statements prepared under these names (`DECLARE s STATEMENT`).
    pub statements: Vec<Ident>,
    /// Cursors.
    pub cursors: Vec<PlCursor>,
    /// Handlers.
    pub handlers: Vec<Handler>,
    /// The statements.
    pub body: Vec<PlStmt>,
}

/// One `SET` assignment: targets and the expressions or row source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assign {
    /// The variables assigned.
    pub targets: Vec<Ident>,
    /// The values, one per target; or one row query (`(SELECT ...)`
    /// or `VALUES`).
    pub source: AssignSource,
    /// `SET a[i] = v`: the index of the element assigned.
    pub element: Option<PlExpr>,
}

/// What a `SET` assigns from.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)] // an AST
pub enum AssignSource {
    /// One expression per target (`NULL` and `DEFAULT` among them).
    Exprs(Vec<PlExpr>),
    /// A row: a scalar fullselect giving one value per target.
    Row(PlSql),
}

/// `GET DIAGNOSTICS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GetDiag {
    /// `CONDITION n` / `EXCEPTION n`: the condition's items; `None` for
    /// the statement items.
    pub condition: Option<u32>,
    /// `target = ITEM`, in order.
    pub items: Vec<(Ident, String)>,
}

/// An SQL PL statement, with its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlStmt {
    /// `label:`.
    pub label: Option<Ident>,
    /// The line it starts on, in the routine body.
    pub line: u32,
    /// The statement.
    pub kind: PlKind,
}

/// The SQL PL statements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlKind {
    /// `BEGIN ... END`.
    Compound(Compound),
    /// An SQL statement (`SELECT INTO`, `INSERT`, DDL, `COMMIT`, ...).
    Sql(PlSql),
    /// `SET`.
    Set(Vec<Assign>),
    /// `IF ... [ELSEIF ...] [ELSE ...] END IF`.
    If {
        /// Conditions and their statements.
        branches: Vec<(PlExpr, Vec<PlStmt>)>,
        /// `ELSE`.
        otherwise: Option<Vec<PlStmt>>,
    },
    /// `CASE [operand] WHEN ... END CASE`.
    Case {
        /// The simple form's operand.
        operand: Option<PlExpr>,
        /// The `WHEN` values or conditions.
        whens: Vec<(PlExpr, Vec<PlStmt>)>,
        /// `ELSE`.
        otherwise: Option<Vec<PlStmt>>,
    },
    /// `LOOP ... END LOOP`.
    Loop(Vec<PlStmt>),
    /// `WHILE c DO ... END WHILE`.
    While(PlExpr, Vec<PlStmt>),
    /// `REPEAT ... UNTIL c END REPEAT`.
    Repeat(Vec<PlStmt>, PlExpr),
    /// `FOR v AS [c CURSOR FOR] query DO ... END FOR`.
    For {
        /// The loop's name.
        var: Ident,
        /// The cursor's name, when given.
        cursor: Option<Ident>,
        /// The query.
        query: PlSql,
        /// The statements.
        body: Vec<PlStmt>,
    },
    /// `LEAVE label`.
    Leave(Ident),
    /// `ITERATE label`.
    Iterate(Ident),
    /// `GOTO label`.
    Goto(Ident),
    /// `RETURN [expr]`.
    Return(Option<PlExpr>),
    /// `RETURN fullselect` of a table function.
    ReturnQuery(PlSql),
    /// `CALL`.
    Call {
        /// The procedure.
        name: ObjectName,
        /// The arguments as written.
        args: Vec<PlExpr>,
    },
    /// `SIGNAL` or `RESIGNAL`.
    Signal(Signal),
    /// `GET DIAGNOSTICS`.
    GetDiag(GetDiag),
    /// `OPEN c [USING ...]`.
    Open {
        /// The cursor.
        cursor: Ident,
        /// `USING` values.
        using: Vec<PlExpr>,
    },
    /// `FETCH [FROM] c INTO ...`.
    Fetch {
        /// The cursor.
        cursor: Ident,
        /// The orientation as written (`NEXT` when absent).
        orient: String,
        /// The targets.
        into: Vec<Ident>,
    },
    /// `CLOSE c`.
    Close(Ident),
    /// `PREPARE s FROM expr`.
    Prepare {
        /// The statement name.
        stmt: Ident,
        /// The text.
        from: PlExpr,
    },
    /// `EXECUTE s [INTO ...] [USING ...]`.
    Execute {
        /// The statement name.
        stmt: Ident,
        /// `INTO` targets.
        into: Vec<Ident>,
        /// `USING` values.
        using: Vec<PlExpr>,
    },
    /// `EXECUTE IMMEDIATE expr`.
    ExecImm(PlExpr),
    /// `SET RESULT SETS`: the cursors whose result sets return.
    SetResultSets(Vec<Ident>),
}
