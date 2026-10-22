// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The command definition: what `CMD`, `PARM`, `ELEM`, `QUAL`, `DEP`
//! and `PMTCTL` statements describe, as the analyzer and the prompter
//! read it. The statement keywords are IBM's and the compatibility
//! interface; the structures here are ours.

use ipc::{wire_enum, wire_struct};

/// A qualified object name as a definition names it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Qual {
    /// The object.
    pub name: String,
    /// The library: a name, `*LIBL` or `*CURLIB`.
    pub lib: String,
}

wire_struct!(Qual { name, lib });

impl Qual {
    /// `lib/name`, `*LIBL` when the library is empty.
    #[must_use]
    pub fn new(name: &str, lib: &str) -> Self {
        Self {
            name: name.to_ascii_uppercase(),
            lib: if lib.is_empty() {
                "*LIBL".into()
            } else {
                lib.to_ascii_uppercase()
            },
        }
    }

    /// Parse `LIB/NAME` or `NAME`.
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.split_once('/') {
            Some((l, n)) => Self::new(n, l),
            None => Self::new(s, "*LIBL"),
        }
    }

    /// `LIB/NAME`.
    #[must_use]
    pub fn text(&self) -> String {
        format!("{}/{}", self.lib, self.name)
    }
}

/// `ALLOW`: where a command may run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Allow(pub u16);

impl Allow {
    /// `*BATCH`.
    pub const BATCH: u16 = 1 << 0;
    /// `*INTERACT`.
    pub const INTERACT: u16 = 1 << 1;
    /// `*BPGM`.
    pub const BPGM: u16 = 1 << 2;
    /// `*IPGM`.
    pub const IPGM: u16 = 1 << 3;
    /// `*BREXX`.
    pub const BREXX: u16 = 1 << 4;
    /// `*IREXX`.
    pub const IREXX: u16 = 1 << 5;
    /// `*EXEC`.
    pub const EXEC: u16 = 1 << 6;
    /// `*BMOD`.
    pub const BMOD: u16 = 1 << 7;
    /// `*IMOD`.
    pub const IMOD: u16 = 1 << 8;
    /// `*ALL`.
    pub const ALL: u16 = 0x1FF;

    /// The flag of a value, if it is one.
    #[must_use]
    pub fn flag(v: &str) -> Option<u16> {
        Some(match v {
            "*BATCH" => Self::BATCH,
            "*INTERACT" => Self::INTERACT,
            "*BPGM" => Self::BPGM,
            "*IPGM" => Self::IPGM,
            "*BREXX" => Self::BREXX,
            "*IREXX" => Self::IREXX,
            "*EXEC" => Self::EXEC,
            "*BMOD" => Self::BMOD,
            "*IMOD" => Self::IMOD,
            "*ALL" => Self::ALL,
            _ => return None,
        })
    }

    /// Whether a flag is set.
    #[must_use]
    pub fn has(self, flag: u16) -> bool {
        self.0 & flag != 0
    }

    /// The values set, in IBM's order.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        if self.0 == Self::ALL {
            return vec!["*ALL"];
        }
        let all = [
            (Self::BATCH, "*BATCH"),
            (Self::INTERACT, "*INTERACT"),
            (Self::BPGM, "*BPGM"),
            (Self::IPGM, "*IPGM"),
            (Self::BREXX, "*BREXX"),
            (Self::IREXX, "*IREXX"),
            (Self::EXEC, "*EXEC"),
            (Self::BMOD, "*BMOD"),
            (Self::IMOD, "*IMOD"),
        ];
        all.iter()
            .filter(|(f, _)| self.has(*f))
            .map(|(_, n)| *n)
            .collect()
    }
}

impl ipc::Wire for Allow {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut ipc::Dec<'_>) -> std::result::Result<Self, ipc::WireError> {
        Ok(Self(u16::get(d)?))
    }
}

/// `MODE`: in which job modes a command may run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Mode(pub u8);

impl Mode {
    /// `*PROD`.
    pub const PROD: u8 = 1;
    /// `*DEBUG`.
    pub const DEBUG: u8 = 2;
    /// `*SERVICE`.
    pub const SERVICE: u8 = 4;
    /// `*ALL`.
    pub const ALL: u8 = 7;

    /// The flag of a value, if it is one.
    #[must_use]
    pub fn flag(v: &str) -> Option<u8> {
        Some(match v {
            "*PROD" => Self::PROD,
            "*DEBUG" => Self::DEBUG,
            "*SERVICE" => Self::SERVICE,
            "*ALL" => Self::ALL,
            _ => return None,
        })
    }

    /// Whether a flag is set.
    #[must_use]
    pub fn has(self, flag: u8) -> bool {
        self.0 & flag != 0
    }

    /// The values set.
    #[must_use]
    pub fn names(self) -> Vec<&'static str> {
        if self.0 == Self::ALL {
            return vec!["*ALL"];
        }
        [
            (Self::PROD, "*PROD"),
            (Self::DEBUG, "*DEBUG"),
            (Self::SERVICE, "*SERVICE"),
        ]
        .iter()
        .filter(|(f, _)| self.has(*f))
        .map(|(_, n)| *n)
        .collect()
    }
}

impl ipc::Wire for Mode {
    fn put(&self, out: &mut Vec<u8>) {
        self.0.put(out);
    }
    fn get(d: &mut ipc::Dec<'_>) -> std::result::Result<Self, ipc::WireError> {
        Ok(Self(u8::get(d)?))
    }
}

/// `THDSAFE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThdSafe {
    /// `*NO`.
    #[default]
    No,
    /// `*COND`.
    Cond,
    /// `*YES`.
    Yes,
}

wire_enum!(ThdSafe { 0 => No, 1 => Cond, 2 => Yes });

/// A parameter's `TYPE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ParmType {
    /// `*CHAR`.
    #[default]
    Char,
    /// `*NAME`.
    Name,
    /// `*GENERIC`.
    Generic,
    /// `*DEC`.
    Dec,
    /// `*LGL`.
    Lgl,
    /// `*INT2`.
    Int2,
    /// `*INT4`.
    Int4,
    /// `*UINT2`.
    Uint2,
    /// `*UINT4`.
    Uint4,
    /// `*DATE`.
    Date,
    /// `*TIME`.
    Time,
    /// `*VARNAME`.
    VarName,
    /// `*CMD`.
    Cmd,
    /// `*CMDSTR`.
    CmdStr,
    /// `*PNAME`.
    PName,
    /// `*SNAME`.
    SName,
    /// `*CNAME`.
    CName,
    /// `*X`.
    X,
    /// `*ZEROELEM`.
    ZeroElem,
    /// `*NULL`.
    Null,
    /// A list of elements: the `ELEM` statements under the label.
    Elem {
        /// The label.
        label: String,
    },
    /// A qualified name: the `QUAL` statements under the label.
    Qual {
        /// The label.
        label: String,
    },
}

wire_enum!(ParmType {
    0 => Char, 1 => Name, 2 => Generic, 3 => Dec, 4 => Lgl, 5 => Int2, 6 => Int4,
    7 => Uint2, 8 => Uint4, 9 => Date, 10 => Time, 11 => VarName, 12 => Cmd, 13 => CmdStr,
    14 => PName, 15 => SName, 16 => CName, 17 => X, 18 => ZeroElem, 19 => Null,
    20 => Elem { label }, 21 => Qual { label }
});

impl ParmType {
    /// Parse the `TYPE` value; a label names an `ELEM` or `QUAL` group
    /// (which, the group says).
    #[must_use]
    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "*CHAR" => Self::Char,
            "*NAME" => Self::Name,
            "*GENERIC" => Self::Generic,
            "*DEC" => Self::Dec,
            "*LGL" => Self::Lgl,
            "*INT2" => Self::Int2,
            "*INT4" => Self::Int4,
            "*UINT2" => Self::Uint2,
            "*UINT4" => Self::Uint4,
            "*DATE" => Self::Date,
            "*TIME" => Self::Time,
            "*VARNAME" => Self::VarName,
            "*CMD" => Self::Cmd,
            "*CMDSTR" => Self::CmdStr,
            "*PNAME" => Self::PName,
            "*SNAME" => Self::SName,
            "*CNAME" => Self::CName,
            "*X" => Self::X,
            "*ZEROELEM" => Self::ZeroElem,
            "*NULL" => Self::Null,
            l if !l.starts_with('*') && !l.is_empty() => Self::Elem {
                label: l.to_string(),
            },
            _ => return None,
        })
    }

    /// The value as the definition writes it.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Char => "*CHAR".into(),
            Self::Name => "*NAME".into(),
            Self::Generic => "*GENERIC".into(),
            Self::Dec => "*DEC".into(),
            Self::Lgl => "*LGL".into(),
            Self::Int2 => "*INT2".into(),
            Self::Int4 => "*INT4".into(),
            Self::Uint2 => "*UINT2".into(),
            Self::Uint4 => "*UINT4".into(),
            Self::Date => "*DATE".into(),
            Self::Time => "*TIME".into(),
            Self::VarName => "*VARNAME".into(),
            Self::Cmd => "*CMD".into(),
            Self::CmdStr => "*CMDSTR".into(),
            Self::PName => "*PNAME".into(),
            Self::SName => "*SNAME".into(),
            Self::CName => "*CNAME".into(),
            Self::X => "*X".into(),
            Self::ZeroElem => "*ZEROELEM".into(),
            Self::Null => "*NULL".into(),
            Self::Elem { label } | Self::Qual { label } => label.clone(),
        }
    }

    /// Whether the type is a kind of name (passed as `*CHAR`, checked
    /// by the name rules).
    #[must_use]
    pub fn is_name(&self) -> bool {
        matches!(
            self,
            Self::Name | Self::Generic | Self::PName | Self::SName | Self::CName
        )
    }

    /// Whether the type is passed as character data.
    #[must_use]
    pub fn is_char(&self) -> bool {
        matches!(
            self,
            Self::Char
                | Self::Name
                | Self::Generic
                | Self::PName
                | Self::SName
                | Self::CName
                | Self::VarName
                | Self::Cmd
                | Self::CmdStr
                | Self::Date
                | Self::Time
                | Self::Lgl
                | Self::X
        )
    }

    /// Whether the type is a group.
    #[must_use]
    pub fn is_group(&self) -> bool {
        matches!(self, Self::Elem { .. } | Self::Qual { .. })
    }

    /// The default `LEN` of the type, when it has one.
    #[must_use]
    pub fn default_len(&self) -> Option<Len> {
        Some(match self {
            Self::Char | Self::PName | Self::CmdStr | Self::Cmd => Len::chars(32),
            Self::Name | Self::Generic | Self::SName | Self::CName => Len::chars(10),
            Self::VarName => Len::chars(11),
            Self::Dec => Len { len: 15, dec: 5 },
            Self::Lgl | Self::X => Len::chars(1),
            Self::Int2 | Self::Uint2 => Len::chars(2),
            Self::Int4 | Self::Uint4 => Len::chars(4),
            Self::Date => Len::chars(7),
            Self::Time => Len::chars(6),
            _ => return None,
        })
    }

    /// The largest `LEN` the type allows.
    #[must_use]
    pub fn max_len(&self) -> u32 {
        match self {
            Self::Char | Self::Cmd | Self::CmdStr | Self::PName | Self::X => 5000,
            Self::Dec => 24,
            Self::VarName => 11,
            _ => 10,
        }
    }
}

/// `LEN`: a length, with decimal positions for `*DEC`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Len {
    /// Characters, or digits for `*DEC`.
    pub len: u32,
    /// Decimal positions (`*DEC`).
    pub dec: u8,
}

wire_struct!(Len { len, dec });

impl Len {
    /// A character length.
    #[must_use]
    pub const fn chars(n: u32) -> Self {
        Self { len: n, dec: 0 }
    }
}

/// A relational operator (`REL`, `RANGE`, `DEP`, `PMTCTL`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RelOp {
    /// `*EQ`.
    #[default]
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
    /// `*NG` (not greater).
    Ng,
    /// `*NL` (not less).
    Nl,
}

wire_enum!(RelOp { 0 => Eq, 1 => Ne, 2 => Gt, 3 => Lt, 4 => Ge, 5 => Le, 6 => Ng, 7 => Nl });

impl RelOp {
    /// Parse `*EQ` and the rest.
    #[must_use]
    pub fn parse(v: &str) -> Option<Self> {
        Some(match v {
            "*EQ" => Self::Eq,
            "*NE" => Self::Ne,
            "*GT" => Self::Gt,
            "*LT" => Self::Lt,
            "*GE" => Self::Ge,
            "*LE" => Self::Le,
            "*NG" => Self::Ng,
            "*NL" => Self::Nl,
            _ => return None,
        })
    }

    /// The operator's text.
    #[must_use]
    pub fn text(self) -> &'static str {
        match self {
            Self::Eq => "*EQ",
            Self::Ne => "*NE",
            Self::Gt => "*GT",
            Self::Lt => "*LT",
            Self::Ge => "*GE",
            Self::Le => "*LE",
            Self::Ng => "*NG",
            Self::Nl => "*NL",
        }
    }

    /// Whether an ordering satisfies the operator.
    #[must_use]
    pub fn holds(self, o: std::cmp::Ordering) -> bool {
        use std::cmp::Ordering::{Equal, Greater, Less};
        match self {
            Self::Eq => o == Equal,
            Self::Ne => o != Equal,
            Self::Gt => o == Greater,
            Self::Lt => o == Less,
            Self::Ge | Self::Nl => o != Less,
            Self::Le | Self::Ng => o != Greater,
        }
    }
}

/// A `SPCVAL` or `SNGVAL` entry: what the user types and what the
/// processor receives (`None`: the same).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Special {
    /// The value typed.
    pub from: String,
    /// The replacement, as source text (`'abc'`, `X'00'`, `10`).
    pub to: Option<String>,
}

wire_struct!(Special { from, to });

/// `FILE`: how a file parameter is used.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileUse {
    /// `*NO`.
    #[default]
    No,
    /// `*IN`.
    In,
    /// `*OUT`.
    Out,
    /// `*UPD`.
    Upd,
    /// `*INOUT`.
    InOut,
    /// `*UNSPFD`.
    Unspfd,
}

wire_enum!(FileUse { 0 => No, 1 => In, 2 => Out, 3 => Upd, 4 => InOut, 5 => Unspfd });

/// `VARY`: how a varying-length value is passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vary {
    /// `*YES *INT2`: a two-byte length before the data.
    Int2,
    /// `*YES *INT4`.
    Int4,
}

wire_enum!(Vary { 0 => Int2, 1 => Int4 });

/// `CASE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Case {
    /// `*MONO`: folded to upper case.
    #[default]
    Mono,
    /// `*MIXED`: kept as typed.
    Mixed,
}

wire_enum!(Case { 0 => Mono, 1 => Mixed });

/// `DSPINPUT`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DspInput {
    /// `*YES`.
    #[default]
    Yes,
    /// `*PROMPT`: shown on the prompt only.
    Prompt,
    /// `*NO`: never shown.
    No,
}

wire_enum!(DspInput { 0 => Yes, 1 => Prompt, 2 => No });

/// `LISTDSPL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ListDspl {
    /// `*LEN`: as many fields as the list length.
    #[default]
    Len,
    /// A number of fields.
    N {
        /// The count.
        n: u16,
    },
}

wire_enum!(ListDspl { 0 => Len, 1 => N { n } });

/// `CHOICE`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Choice {
    /// `*VALUES`: from `VALUES`, `SPCVAL`, `SNGVAL` and `RSTD`.
    #[default]
    Values,
    /// `*NONE`.
    None,
    /// `*PGM`: `CHOICEPGM` supplies the text.
    Pgm,
    /// Text, or a message identifier (`MSGID`-style).
    Text {
        /// The text.
        text: String,
    },
}

wire_enum!(Choice { 0 => Values, 1 => None, 2 => Pgm, 3 => Text { text } });

/// `PMTCTL` on a parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PmtCtlRef {
    /// `*PMTRQS`: shown after F10.
    PmtRqs,
    /// The labelled `PMTCTL` statement.
    Label {
        /// The label.
        label: String,
    },
}

wire_enum!(PmtCtlRef { 0 => PmtRqs, 1 => Label { label } });

/// `INLPMTLEN`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InlPmtLen {
    /// `*CALC`.
    #[default]
    Calc,
    /// `*PWD`.
    Pwd,
    /// A length.
    N {
        /// The length.
        n: u16,
    },
}

wire_enum!(InlPmtLen { 0 => Calc, 1 => Pwd, 2 => N { n } });

/// `PASSVAL`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PassVal {
    /// `*DFT`.
    #[default]
    Dft,
    /// `*NULL`.
    Null,
}

wire_enum!(PassVal { 0 => Dft, 1 => Null });

/// A `PARM`, `ELEM` or `QUAL` statement. Elements and qualifiers have
/// no keyword.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // IBM's attribute set is flags
pub struct Parm {
    /// `KWD`, empty for an element or a qualifier.
    pub kwd: String,
    /// `TYPE`.
    pub ty: ParmType,
    /// `LEN`.
    pub len: Option<Len>,
    /// `RTNVAL(*YES)`.
    pub rtnval: bool,
    /// `RSTD(*YES)`.
    pub rstd: bool,
    /// `DFT`, as source text.
    pub dft: Option<String>,
    /// `VALUES`.
    pub values: Vec<String>,
    /// `REL`.
    pub rel: Option<(RelOp, String)>,
    /// `RANGE`.
    pub range: Option<(String, String)>,
    /// `SPCVAL`.
    pub spcval: Vec<Special>,
    /// `SNGVAL`.
    pub sngval: Vec<Special>,
    /// `MIN`.
    pub min: u16,
    /// `MAX`.
    pub max: u16,
    /// `ALWUNPRT(*YES)`.
    pub alwunprt: bool,
    /// `ALWVAR(*YES)`.
    pub alwvar: bool,
    /// `PGM(*YES)`.
    pub pgm: bool,
    /// `DTAARA(*YES)`.
    pub dtaara: bool,
    /// `FILE`.
    pub file: FileUse,
    /// `FULL(*YES)`.
    pub full: bool,
    /// `EXPR(*YES)`.
    pub expr: bool,
    /// `VARY`.
    pub vary: Option<Vary>,
    /// `PASSATR(*YES)`.
    pub passatr: bool,
    /// `PASSVAL`.
    pub passval: PassVal,
    /// `CASE`.
    pub case: Case,
    /// `LISTDSPL`.
    pub listdspl: ListDspl,
    /// `CHOICE`.
    pub choice: Choice,
    /// `CHOICEPGM`.
    pub choicepgm: Option<Qual>,
    /// `PMTCTL`.
    pub pmtctl: Option<PmtCtlRef>,
    /// `PMTCTLPGM`.
    pub pmtctlpgm: Option<Qual>,
    /// `KEYPARM(*YES)`.
    pub keyparm: bool,
    /// `PROMPT`: the text and the order.
    pub prompt: Option<(String, Option<u16>)>,
    /// `CONSTANT`, as source text.
    pub constant: Option<String>,
    /// `DSPINPUT`.
    pub dspinput: DspInput,
    /// `INLPMTLEN`.
    pub inlpmtlen: Option<InlPmtLen>,
    /// The elements or qualifiers of a group type, in order.
    pub items: Vec<Parm>,
}

impl Default for Parm {
    fn default() -> Self {
        Self {
            kwd: String::new(),
            ty: ParmType::Char,
            len: None,
            rtnval: false,
            rstd: false,
            dft: None,
            values: Vec::new(),
            rel: None,
            range: None,
            spcval: Vec::new(),
            sngval: Vec::new(),
            min: 0,
            max: 1,
            alwunprt: false,
            alwvar: true,
            pgm: false,
            dtaara: false,
            file: FileUse::No,
            full: false,
            expr: false,
            vary: None,
            passatr: false,
            passval: PassVal::Dft,
            case: Case::Mono,
            listdspl: ListDspl::Len,
            choice: Choice::Values,
            choicepgm: None,
            pmtctl: None,
            pmtctlpgm: None,
            keyparm: false,
            prompt: None,
            constant: None,
            dspinput: DspInput::Yes,
            inlpmtlen: None,
            items: Vec::new(),
        }
    }
}

wire_struct!(Parm {
    kwd,
    ty,
    len,
    rtnval,
    rstd,
    dft,
    values,
    rel,
    range,
    spcval,
    sngval,
    min,
    max,
    alwunprt,
    alwvar,
    pgm,
    dtaara,
    file,
    full,
    expr,
    vary,
    passatr,
    passval,
    case,
    listdspl,
    choice,
    choicepgm,
    pmtctl,
    pmtctlpgm,
    keyparm,
    prompt,
    constant,
    dspinput,
    inlpmtlen,
    items
});

impl Parm {
    /// The effective `LEN`: the statement's or the type's default.
    #[must_use]
    pub fn length(&self) -> Len {
        self.len
            .or_else(|| self.ty.default_len())
            .unwrap_or_default()
    }

    /// Whether the parameter is a list (`MAX` above one).
    #[must_use]
    pub fn is_list(&self) -> bool {
        self.max > 1
    }

    /// Whether a value is required (`MIN` above zero).
    #[must_use]
    pub fn required(&self) -> bool {
        self.min > 0
    }

    /// The prompt text, or the keyword when there is none.
    #[must_use]
    pub fn prompt_text(&self) -> &str {
        self.prompt
            .as_ref()
            .map_or(self.kwd.as_str(), |(t, _)| t.as_str())
    }
}

/// How `DEP` names a parameter or a condition on one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepItem {
    /// `&KWD`: the parameter was specified.
    Specified {
        /// The keyword.
        kwd: String,
    },
    /// `(&KWD *EQ value)`.
    Cond {
        /// The keyword.
        kwd: String,
        /// The operator.
        op: RelOp,
        /// The value, as source text.
        value: String,
    },
}

wire_enum!(DepItem { 0 => Specified { kwd }, 1 => Cond { kwd, op, value } });

/// `CTL` of a `DEP`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepCtl {
    /// `*ALWAYS`.
    Always,
    /// A parameter or a condition.
    Item {
        /// The item.
        item: DepItem,
    },
}

wire_enum!(DepCtl { 0 => Always, 1 => Item { item } });

/// `NBRTRUE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NbrTrue {
    /// `*ALL`.
    #[default]
    All,
    /// `*NONE`.
    None,
    /// `(*EQ n)`, `(*GT n)`...
    Count {
        /// The operator.
        op: RelOp,
        /// The count.
        n: u16,
    },
}

wire_enum!(NbrTrue { 0 => All, 1 => None, 2 => Count { op, n } });

/// A `DEP` statement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dep {
    /// `CTL`.
    pub ctl: DepCtl,
    /// `PARM`.
    pub parms: Vec<DepItem>,
    /// `NBRTRUE`.
    pub nbrtrue: NbrTrue,
    /// `MSGID`: the diagnostic when the dependency fails; `CPD0061`-
    /// style default.
    pub msgid: Option<String>,
}

wire_struct!(Dep {
    ctl,
    parms,
    nbrtrue,
    msgid
});

/// `LGLREL` between `PMTCTL` statements of one label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LglRel {
    /// `*AND`.
    #[default]
    And,
    /// `*OR`.
    Or,
}

wire_enum!(LglRel { 0 => And, 1 => Or });

/// One `PMTCTL` statement: the controlling parameter, the conditions
/// and how many must hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PmtCtl {
    /// The label (the first statement of a chain carries it).
    pub label: String,
    /// `CTL`: the controlling keyword.
    pub ctl: String,
    /// `COND`.
    pub conds: Vec<(RelOp, String)>,
    /// `NBRTRUE`.
    pub nbrtrue: NbrTrue,
    /// `LGLREL` to the statement before it in the chain.
    pub lglrel: Option<LglRel>,
}

wire_struct!(PmtCtl {
    label,
    ctl,
    conds,
    nbrtrue,
    lglrel
});

/// A command definition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdDef {
    /// The command name (from `CRTCMD`).
    pub name: String,
    /// `PROMPT`: the title of the prompt display.
    pub prompt: String,
    /// `TEXT` of the object.
    pub text: String,
    /// `ALLOW`.
    pub allow: Allow,
    /// `MODE`.
    pub mode: Mode,
    /// `MAXPOS`; `None` for `*NONE` (no limit).
    pub maxpos: Option<u8>,
    /// `PGM`: the command processing program.
    pub cpp: Qual,
    /// `VLDCKR`.
    pub vldckr: Option<Qual>,
    /// `PMTOVRPGM`.
    pub pmtovrpgm: Option<Qual>,
    /// `PMTFILE`.
    pub pmtfile: Option<Qual>,
    /// `MSGF`.
    pub msgf: Qual,
    /// `PRDLIB`.
    pub prdlib: Option<String>,
    /// `CURLIB`.
    pub curlib: Option<String>,
    /// `HLPPNLGRP`.
    pub hlppnlgrp: Option<Qual>,
    /// `HLPID`.
    pub hlpid: Option<String>,
    /// `THDSAFE`.
    pub thdsafe: ThdSafe,
    /// `ALLOW(*LIMITED)`-style: `CRTCMD ... ALWLMTUSR(*YES)`.
    pub alwlmtusr: bool,
    /// The parameters, in definition order.
    pub parms: Vec<Parm>,
    /// The dependencies.
    pub deps: Vec<Dep>,
    /// The prompt controls, in statement order.
    pub pmtctls: Vec<PmtCtl>,
}

impl Default for CmdDef {
    fn default() -> Self {
        Self {
            name: String::new(),
            prompt: String::new(),
            text: String::new(),
            allow: Allow(Allow::ALL),
            mode: Mode(Mode::ALL),
            maxpos: None,
            cpp: Qual::new("*REXX", "*LIBL"),
            vldckr: None,
            pmtovrpgm: None,
            pmtfile: None,
            msgf: Qual::new("QCPFMSG", "*LIBL"),
            prdlib: None,
            curlib: None,
            hlppnlgrp: None,
            hlpid: None,
            thdsafe: ThdSafe::No,
            alwlmtusr: false,
            parms: Vec::new(),
            deps: Vec::new(),
            pmtctls: Vec::new(),
        }
    }
}

wire_struct!(CmdDef {
    name,
    prompt,
    text,
    allow,
    mode,
    maxpos,
    cpp,
    vldckr,
    pmtovrpgm,
    pmtfile,
    msgf,
    prdlib,
    curlib,
    hlppnlgrp,
    hlpid,
    thdsafe,
    alwlmtusr,
    parms,
    deps,
    pmtctls
});

impl CmdDef {
    /// The parameter of a keyword.
    #[must_use]
    pub fn parm(&self, kwd: &str) -> Option<&Parm> {
        self.parms.iter().find(|p| p.kwd == kwd)
    }

    /// The index of a keyword's parameter.
    #[must_use]
    pub fn index(&self, kwd: &str) -> Option<usize> {
        self.parms.iter().position(|p| p.kwd == kwd)
    }

    /// The `PMTCTL` chain of a label.
    #[must_use]
    pub fn pmtctl(&self, label: &str) -> Vec<&PmtCtl> {
        self.pmtctls.iter().filter(|p| p.label == label).collect()
    }
}
