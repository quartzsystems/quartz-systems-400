// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The options of the three lists and what each does with an entry:
//! a command (substituted, `?` first to prompt it), or something the
//! driver does itself (a confirmation, a rename or text panel, the
//! next list, the compile, find string).

use crate::PdmError;
use crate::compile;
use crate::subst::{Context, Item, substitute};

/// The PDM list an option is typed on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListKind {
    /// Work with Libraries Using PDM.
    Library,
    /// Work with Objects Using PDM.
    Object,
    /// Work with Members Using PDM.
    Member,
}

impl ListKind {
    /// The `&B` letter.
    #[must_use]
    pub fn letter(self) -> char {
        match self {
            Self::Library => 'L',
            Self::Object => 'O',
            Self::Member => 'M',
        }
    }
}

/// One option of a list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PdmOption {
    /// The number typed.
    pub number: u8,
    /// The legend.
    pub legend: &'static str,
}

/// The library list's options, in legend order.
pub const LIBRARY: &[PdmOption] = &[
    PdmOption {
        number: 2,
        legend: "2=Change",
    },
    PdmOption {
        number: 3,
        legend: "3=Copy",
    },
    PdmOption {
        number: 4,
        legend: "4=Delete",
    },
    PdmOption {
        number: 5,
        legend: "5=Display",
    },
    PdmOption {
        number: 7,
        legend: "7=Rename",
    },
    PdmOption {
        number: 8,
        legend: "8=Display description",
    },
    PdmOption {
        number: 9,
        legend: "9=Save",
    },
    PdmOption {
        number: 10,
        legend: "10=Restore",
    },
    PdmOption {
        number: 12,
        legend: "12=Work with",
    },
    PdmOption {
        number: 13,
        legend: "13=Change text",
    },
];

/// The object list's options.
pub const OBJECT: &[PdmOption] = &[
    PdmOption {
        number: 2,
        legend: "2=Change",
    },
    PdmOption {
        number: 3,
        legend: "3=Copy",
    },
    PdmOption {
        number: 4,
        legend: "4=Delete",
    },
    PdmOption {
        number: 5,
        legend: "5=Display",
    },
    PdmOption {
        number: 7,
        legend: "7=Rename",
    },
    PdmOption {
        number: 8,
        legend: "8=Display description",
    },
    PdmOption {
        number: 9,
        legend: "9=Save",
    },
    PdmOption {
        number: 10,
        legend: "10=Restore",
    },
    PdmOption {
        number: 11,
        legend: "11=Move",
    },
    PdmOption {
        number: 12,
        legend: "12=Work with",
    },
    PdmOption {
        number: 13,
        legend: "13=Change text",
    },
    PdmOption {
        number: 15,
        legend: "15=Copy file",
    },
    PdmOption {
        number: 16,
        legend: "16=Run",
    },
    PdmOption {
        number: 18,
        legend: "18=Change using DFU",
    },
    PdmOption {
        number: 25,
        legend: "25=Find string",
    },
    PdmOption {
        number: 26,
        legend: "26=Create program",
    },
    PdmOption {
        number: 27,
        legend: "27=Create service program",
    },
    PdmOption {
        number: 34,
        legend: "34=Interactive source debugger",
    },
    PdmOption {
        number: 54,
        legend: "54=Compare",
    },
];

/// The member list's options.
pub const MEMBER: &[PdmOption] = &[
    PdmOption {
        number: 2,
        legend: "2=Edit",
    },
    PdmOption {
        number: 3,
        legend: "3=Copy",
    },
    PdmOption {
        number: 4,
        legend: "4=Delete",
    },
    PdmOption {
        number: 5,
        legend: "5=Display",
    },
    PdmOption {
        number: 6,
        legend: "6=Print",
    },
    PdmOption {
        number: 7,
        legend: "7=Rename",
    },
    PdmOption {
        number: 8,
        legend: "8=Display description",
    },
    PdmOption {
        number: 9,
        legend: "9=Save",
    },
    PdmOption {
        number: 13,
        legend: "13=Change text",
    },
    PdmOption {
        number: 14,
        legend: "14=Compile",
    },
    PdmOption {
        number: 15,
        legend: "15=Create module",
    },
    PdmOption {
        number: 16,
        legend: "16=Run procedure",
    },
    PdmOption {
        number: 17,
        legend: "17=Change using SDA",
    },
    PdmOption {
        number: 18,
        legend: "18=Change using DFU",
    },
    PdmOption {
        number: 19,
        legend: "19=Change using RLU",
    },
    PdmOption {
        number: 25,
        legend: "25=Find string",
    },
    PdmOption {
        number: 54,
        legend: "54=Compare",
    },
    PdmOption {
        number: 55,
        legend: "55=Merge",
    },
];

/// A list's options.
#[must_use]
pub fn options(list: ListKind) -> &'static [PdmOption] {
    match list {
        ListKind::Library => LIBRARY,
        ListKind::Object => OBJECT,
        ListKind::Member => MEMBER,
    }
}

/// The option typed, if the list has it.
#[must_use]
pub fn find(list: ListKind, typed: &str) -> Option<PdmOption> {
    let n: u8 = typed.trim().parse().ok()?;
    options(list).iter().copied().find(|o| o.number == n)
}

/// What an option does with an entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Run these commands in order (a leading `?` prompts one).
    Run(Vec<String>),
    /// Confirm, then run the commands (option 4).
    Confirm(Vec<String>),
    /// Ask for a new name, then run the template with `&W` (new name)
    /// substituted (option 7).
    Rename(String),
    /// Ask for new text, then run the template with `&W` the quoted
    /// text (option 13).
    ChangeText(String),
    /// Show the next list: objects of a library, members of a file.
    WorkWith(ListKind),
    /// Find string over a file's members (option 25).
    Find,
}

/// The command that changes an object of a type (option 2).
fn change(ty: &str, attr: &str) -> Option<&'static str> {
    Some(match (ty, attr) {
        ("*FILE", "PF") => "?CHGPF FILE(&L/&N)",
        ("*FILE", "LF") => "?CHGLF FILE(&L/&N)",
        ("*FILE", "DSPF") => "?CHGDSPF FILE(&L/&N)",
        ("*FILE", "PRTF") => "?CHGPRTF FILE(&L/&N)",
        ("*PGM", _) => "?CHGPGM PGM(&L/&N)",
        ("*SRVPGM", _) => "?CHGSRVPGM SRVPGM(&L/&N)",
        ("*CMD", _) => "?CHGCMD CMD(&L/&N)",
        ("*DTAARA", _) => "?CHGDTAARA DTAARA(&L/&N)",
        ("*JOBD", _) => "?CHGJOBD JOBD(&L/&N)",
        ("*JOBQ", _) => "?CHGJOBQ JOBQ(&L/&N)",
        ("*OUTQ", _) => "?CHGOUTQ OUTQ(&L/&N)",
        ("*MSGQ", _) => "?CHGMSGQ MSGQ(&L/&N)",
        ("*CLS", _) => "?CHGCLS CLS(&L/&N)",
        ("*MSGF", _) => "?CHGMSGF MSGF(&L/&N)",
        ("*LIB", _) => "?CHGLIB LIB(&N)",
        ("*USRPRF", _) => "?CHGUSRPRF USRPRF(&N)",
        ("*SBSD", _) => "?CHGSBSD SBSD(&L/&N)",
        _ => return None,
    })
}

/// The command that shows an object of a type (option 5).
fn display(ty: &str, attr: &str) -> &'static str {
    match (ty, attr) {
        ("*FILE", "PF" | "LF") => "DSPPFM FILE(&L/&N)",
        ("*FILE", _) => "DSPFD FILE(&L/&N)",
        ("*PGM", _) => "DSPPGM PGM(&L/&N)",
        ("*SRVPGM", _) => "DSPSRVPGM SRVPGM(&L/&N)",
        ("*MODULE", _) => "DSPMOD MODULE(&L/&N)",
        ("*CMD", _) => "DSPCMD CMD(&L/&N)",
        ("*DTAARA", _) => "DSPDTAARA DTAARA(&L/&N)",
        ("*JOBD", _) => "DSPJOBD JOBD(&L/&N)",
        ("*CLS", _) => "DSPCLS CLS(&L/&N)",
        ("*MSGF", _) => "WRKMSGD MSGF(&L/&N)",
        ("*MSGQ", _) => "DSPMSG MSGQ(&L/&N)",
        ("*SBSD", _) => "DSPSBSD SBSD(&L/&N)",
        ("*BNDDIR", _) => "DSPBNDDIR BNDDIR(&L/&N)",
        ("*OUTQ", _) => "WRKOUTQ OUTQ(&L/&N)",
        ("*JOBQ", _) => "WRKJOBQ JOBQ(&L/&N)",
        ("*AUTL", _) => "DSPAUTL AUTL(&N)",
        ("*USRPRF", _) => "DSPUSRPRF USRPRF(&N)",
        ("*JRNRCV", _) => "DSPJRNRCVA JRNRCV(&L/&N)",
        ("*DEVD", _) => "DSPDEVD DEVD(&N)",
        ("*LIB", _) => "DSPLIB LIB(&N)",
        _ => "DSPOBJD OBJ(&L/&N) OBJTYPE(&T)",
    }
}

/// The command that deletes an object of a type (option 4).
fn delete(ty: &str) -> String {
    let (cmd, kwd, qualified) = match ty {
        "*PGM" => ("DLTPGM", "PGM", true),
        "*SRVPGM" => ("DLTSRVPGM", "SRVPGM", true),
        "*MODULE" => ("DLTMOD", "MODULE", true),
        "*FILE" => ("DLTF", "FILE", true),
        "*DTAARA" => ("DLTDTAARA", "DTAARA", true),
        "*DTAQ" => ("DLTDTAQ", "DTAQ", true),
        "*MSGF" => ("DLTMSGF", "MSGF", true),
        "*MSGQ" => ("DLTMSGQ", "MSGQ", true),
        "*CMD" => ("DLTCMD", "CMD", true),
        "*BNDDIR" => ("DLTBNDDIR", "BNDDIR", true),
        "*JOBD" => ("DLTJOBD", "JOBD", true),
        "*JOBQ" => ("DLTJOBQ", "JOBQ", true),
        "*OUTQ" => ("DLTOUTQ", "OUTQ", true),
        "*CLS" => ("DLTCLS", "CLS", true),
        "*SBSD" => ("DLTSBSD", "SBSD", true),
        "*JRN" => ("DLTJRN", "JRN", true),
        "*JRNRCV" => ("DLTJRNRCV", "JRNRCV", true),
        "*PNLGRP" => ("DLTPNLGRP", "PNLGRP", true),
        "*MENU" => ("DLTMNU", "MENU", true),
        "*USRSPC" => ("DLTUSRSPC", "USRSPC", true),
        "*USRIDX" => ("DLTUSRIDX", "USRIDX", true),
        "*LIB" => ("DLTLIB", "LIB", false),
        "*USRPRF" => ("DLTUSRPRF", "USRPRF", false),
        "*AUTL" => ("DLTAUTL", "AUTL", false),
        "*DEVD" => ("DLTDEVD", "DEVD", false),
        _ => return "DLTOBJ OBJ(&L/&N) OBJTYPE(&T)".into(),
    };
    if qualified {
        format!("{cmd} {kwd}(&L/&N)")
    } else {
        format!("{cmd} {kwd}(&N)")
    }
}

/// Source types SDA, DFU and RLU work on.
fn for_tool(number: u8, ty: &str) -> Option<&'static str> {
    Some(match (number, ty) {
        (17, "DSPF" | "MNUDDS") => "STRSDA OPTION(1) SRCFILE(&L/&F) SRCMBR(&N)",
        (18, "PF" | "LF") => "UPDDTA FILE(&O/&N)",
        (19, "PRTF") => "STRRLU SRCFILE(&L/&F) SRCMBR(&N)",
        _ => return None,
    })
}

/// The commands for one template.
fn run(t: &str, item: &Item, ctx: &Context<'_>) -> Outcome {
    Outcome::Run(vec![substitute(t, item, ctx)])
}

/// Not for this type.
fn not_for(ctx: &Context<'_>, item: &Item) -> PdmError {
    PdmError::NotForType {
        option: ctx.option.to_string(),
        what: if ctx.list == ListKind::Member {
            "member type"
        } else {
            "object type"
        },
        ty: item.ty.clone(),
    }
}

/// What an option does with an entry of the list `ctx.list`.
///
/// # Errors
///
/// The list has no such option, it does not apply to the entry's
/// type, or the compiler is not installed.
#[allow(clippy::too_many_lines)] // one arm per option and list
pub fn outcome(item: &Item, ctx: &Context<'_>) -> Result<Outcome, PdmError> {
    let Some(opt) = find(ctx.list, ctx.option) else {
        return Err(PdmError::NoOption(ctx.option.trim().to_string()));
    };
    let n = opt.number;
    let ty = item.ty.trim().to_ascii_uppercase();
    let attr = item.attr.trim().to_ascii_uppercase();
    let sub = |t: &str| substitute(t, item, ctx);
    match ctx.list {
        ListKind::Library => Ok(match n {
            2 => run("?CHGLIB LIB(&N)", item, ctx),
            3 => run("?CPYLIB FROMLIB(&N)", item, ctx),
            4 => Outcome::Confirm(vec![sub("DLTLIB LIB(&N)")]),
            5 => run("DSPLIB LIB(&N)", item, ctx),
            7 => Outcome::Rename(sub("RNMOBJ OBJ(QSYS/&N) OBJTYPE(*LIB) NEWOBJ(&W)")),
            8 => run(
                "DSPOBJD OBJ(QSYS/&N) OBJTYPE(*LIB) DETAIL(*FULL)",
                item,
                ctx,
            ),
            9 => run("?SAVLIB LIB(&N)", item, ctx),
            10 => run("?RSTLIB SAVLIB(&N)", item, ctx),
            12 => Outcome::WorkWith(ListKind::Object),
            13 => Outcome::ChangeText(sub("CHGOBJD OBJ(QSYS/&N) OBJTYPE(*LIB) TEXT(&W)")),
            _ => return Err(PdmError::NoOption(ctx.option.trim().to_string())),
        }),
        ListKind::Object => Ok(match n {
            2 => run(
                change(&ty, &attr).ok_or_else(|| not_for(ctx, item))?,
                item,
                ctx,
            ),
            3 | 15 if ty == "*FILE" => run("?CPYF FROMFILE(&L/&N)", item, ctx),
            3 => run("?CRTDUPOBJ OBJ(&N) FROMLIB(&L) OBJTYPE(&T)", item, ctx),
            4 => Outcome::Confirm(vec![sub(&delete(&ty))]),
            5 => run(display(&ty, &attr), item, ctx),
            7 => Outcome::Rename(sub("RNMOBJ OBJ(&L/&N) OBJTYPE(&T) NEWOBJ(&W)")),
            8 => run("DSPOBJD OBJ(&L/&N) OBJTYPE(&T) DETAIL(*FULL)", item, ctx),
            9 => run("?SAVOBJ OBJ(&N) LIB(&L) OBJTYPE(&T)", item, ctx),
            10 => run("?RSTOBJ OBJ(&N) SAVLIB(&L) OBJTYPE(&T)", item, ctx),
            11 => run("?MOVOBJ OBJ(&L/&N) OBJTYPE(&T)", item, ctx),
            12 if ty == "*FILE"
                && matches!(
                    attr.as_str(),
                    "PF" | "LF" | "PF-SRC" | "PF-DTA" | "LF-DTA" | ""
                ) =>
            {
                Outcome::WorkWith(ListKind::Member)
            }
            12 => match ty.as_str() {
                "*MSGF" => run("WRKMSGD MSGF(&L/&N)", item, ctx),
                "*OUTQ" => run("WRKOUTQ OUTQ(&L/&N)", item, ctx),
                "*JOBQ" => run("WRKJOBQ JOBQ(&L/&N)", item, ctx),
                "*BNDDIR" => run("WRKBNDDIR BNDDIR(&L/&N)", item, ctx),
                "*JOBD" => run("WRKJOBD JOBD(&L/&N)", item, ctx),
                "*LIB" => Outcome::WorkWith(ListKind::Object),
                _ => return Err(not_for(ctx, item)),
            },
            13 => Outcome::ChangeText(sub("CHGOBJD OBJ(&L/&N) OBJTYPE(&T) TEXT(&W)")),
            16 if ty == "*PGM" => {
                let call = sub("CALL PGM(&L/&N)");
                Outcome::Run(vec![if ctx.defaults.batch_run {
                    compile::in_batch(&call, item, ctx)
                } else {
                    call
                }])
            }
            16 if ty == "*CMD" => run("?&L/&N", item, ctx),
            18 if ty == "*FILE" && matches!(attr.as_str(), "PF" | "LF" | "PF-DTA" | "LF-DTA") => {
                run("UPDDTA FILE(&L/&N)", item, ctx)
            }
            25 if ty == "*FILE" => Outcome::Find,
            26 if ty == "*MODULE" => run("?CRTPGM PGM(&O/&N) MODULE(&L/&N)", item, ctx),
            27 if ty == "*MODULE" => run("?CRTSRVPGM SRVPGM(&O/&N) MODULE(&L/&N)", item, ctx),
            34 if ty == "*PGM" => run("?STRDBG PGM(&L/&N)", item, ctx),
            54 if ty == "*FILE" => run("?CMPPFM NEWFILE(&L/&N)", item, ctx),
            _ => return Err(not_for(ctx, item)),
        }),
        ListKind::Member => Ok(match n {
            2 => run(
                "STRSEU SRCFILE(&L/&F) SRCMBR(&N) TYPE(&T) OPTION(2)",
                item,
                ctx,
            ),
            3 => run(
                "?CPYSRCF FROMFILE(&L/&F) TOFILE(&L/&F) FROMMBR(&N)",
                item,
                ctx,
            ),
            4 => Outcome::Confirm(vec![sub("RMVM FILE(&L/&F) MBR(&N)")]),
            5 => run(
                "STRSEU SRCFILE(&L/&F) SRCMBR(&N) TYPE(&T) OPTION(5)",
                item,
                ctx,
            ),
            6 => run(
                "STRSEU SRCFILE(&L/&F) SRCMBR(&N) TYPE(&T) OPTION(6)",
                item,
                ctx,
            ),
            7 => Outcome::Rename(sub("RNMM FILE(&L/&F) MBR(&N) NEWMBR(&W)")),
            8 => run("DSPFD FILE(&L/&F) TYPE(*MBR)", item, ctx),
            9 => run("?SAVOBJ OBJ(&F) LIB(&L) OBJTYPE(*FILE)", item, ctx),
            13 => Outcome::ChangeText(sub("CHGPFM FILE(&L/&F) MBR(&N) TEXT(&W)")),
            14 => Outcome::Run(compile::commands(item, ctx, false)?),
            15 => Outcome::Run(compile::commands(item, ctx, true)?),
            16 if ty == "REXX" => return Err(PdmError::NotInstalled(ty)),
            17..=19 => run(
                for_tool(n, &ty).ok_or_else(|| not_for(ctx, item))?,
                item,
                ctx,
            ),
            25 => Outcome::Find,
            54 => run("?CMPPFM NEWFILE(&L/&F) NEWMBR(&N)", item, ctx),
            55 => run("?MRGSRC TGTFILE(&L/&F) TGTMBR(&N)", item, ctx),
            _ => return Err(not_for(ctx, item)),
        }),
    }
}

/// An option's command with what was typed on the command line: the
/// parameters are added to its end.
#[must_use]
pub fn with_parameters(cmd: &str, parms: &str) -> String {
    let p = parms.trim();
    if p.is_empty() {
        cmd.to_string()
    } else {
        format!("{cmd} {p}")
    }
}

/// `&W` in a rename or change-text template.
#[must_use]
pub fn fill_w(template: &str, value: &str) -> String {
    template.replace("&W", value)
}

/// Text as a quoted CL string.
#[must_use]
pub fn quoted(text: &str) -> String {
    format!("'{}'", text.trim_end().replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults::Defaults;

    fn ctx<'a>(list: ListKind, option: &'a str, d: &'a Defaults) -> Context<'a> {
        Context {
            list,
            option,
            defaults: d,
            object_lib: "OBJ",
            user: "U",
        }
    }

    fn obj(ty: &str, attr: &str) -> Item {
        Item {
            library: "L1".into(),
            name: "N1".into(),
            ty: ty.into(),
            attr: attr.into(),
            ..Item::default()
        }
    }

    #[test]
    fn members() {
        let d = Defaults::default();
        let m = Item {
            library: "L".into(),
            file: "QCLSRC".into(),
            name: "M".into(),
            ty: "CLLE".into(),
            ..Item::default()
        };
        assert_eq!(
            outcome(&m, &ctx(ListKind::Member, "2", &d)),
            Ok(Outcome::Run(vec![
                "STRSEU SRCFILE(L/QCLSRC) SRCMBR(M) TYPE(CLLE) OPTION(2)".into()
            ]))
        );
        assert_eq!(
            outcome(&m, &ctx(ListKind::Member, "4", &d)),
            Ok(Outcome::Confirm(vec!["RMVM FILE(L/QCLSRC) MBR(M)".into()]))
        );
        assert!(matches!(
            outcome(&m, &ctx(ListKind::Member, "17", &d)),
            Err(PdmError::NotForType { .. })
        ));
        assert_eq!(
            outcome(&m, &ctx(ListKind::Member, "25", &d)),
            Ok(Outcome::Find)
        );
        assert_eq!(
            outcome(&m, &ctx(ListKind::Member, "99", &d)),
            Err(PdmError::NoOption("99".into()))
        );
        let Ok(Outcome::Rename(t)) = outcome(&m, &ctx(ListKind::Member, "7", &d)) else {
            panic!("rename");
        };
        assert_eq!(fill_w(&t, "NEW"), "RNMM FILE(L/QCLSRC) MBR(M) NEWMBR(NEW)");
    }

    #[test]
    fn objects_and_libraries() {
        let d = Defaults::default();
        assert_eq!(
            outcome(&obj("*PGM", "CLLE"), &ctx(ListKind::Object, "16", &d)),
            Ok(Outcome::Run(vec!["CALL PGM(L1/N1)".into()]))
        );
        assert_eq!(
            outcome(&obj("*FILE", "PF"), &ctx(ListKind::Object, "12", &d)),
            Ok(Outcome::WorkWith(ListKind::Member))
        );
        assert_eq!(
            outcome(&obj("*PGM", ""), &ctx(ListKind::Object, "4", &d)),
            Ok(Outcome::Confirm(vec!["DLTPGM PGM(L1/N1)".into()]))
        );
        assert!(outcome(&obj("*DTAQ", ""), &ctx(ListKind::Object, "2", &d)).is_err());
        assert_eq!(
            outcome(&obj("PROD", ""), &ctx(ListKind::Library, "12", &d)),
            Ok(Outcome::WorkWith(ListKind::Object))
        );
        assert_eq!(
            outcome(&obj("PROD", ""), &ctx(ListKind::Library, "5", &d)),
            Ok(Outcome::Run(vec!["DSPLIB LIB(N1)".into()]))
        );
        assert_eq!(
            with_parameters("DSPLIB LIB(X)", " OUTPUT(*PRINT) "),
            "DSPLIB LIB(X) OUTPUT(*PRINT)"
        );
        assert_eq!(quoted("it's"), "'it''s'");
    }
}
