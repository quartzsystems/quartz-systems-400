// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Options 14 (compile) and 15 (create module): the command for a
//! member's source type, in batch when the defaults say so. RPG III,
//! COBOL and C types are known and refused until their compilers exist.

use crate::PdmError;
use crate::subst::{Context, Item, substitute};

/// What a source type compiles with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compiler {
    /// The commands, in order (a UIM menu is a panel group and then the
    /// menu over it).
    Cmds(&'static [&'static str]),
    /// Known, but the compiler (named) is not installed.
    NotInstalled(&'static str),
}

/// Option 14's compiler for a source type, `None` when the type has
/// none.
#[must_use]
pub fn compiler(srctype: &str) -> Option<Compiler> {
    use Compiler::{Cmds, NotInstalled};
    Some(match srctype.trim().to_ascii_uppercase().as_str() {
        "CLLE" => Cmds(&["CRTBNDCL PGM(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "CLP" | "CL" => Cmds(&["CRTCLPGM PGM(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "PF" => Cmds(&["CRTPF FILE(&O/&N) SRCFILE(&L/&F) SRCMBR(&N)"]),
        "LF" => Cmds(&["CRTLF FILE(&O/&N) SRCFILE(&L/&F) SRCMBR(&N)"]),
        "DSPF" | "MNUDDS" => Cmds(&["CRTDSPF FILE(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "PRTF" => Cmds(&["CRTPRTF FILE(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "CMD" => Cmds(&["CRTCMD CMD(&O/&N) PGM(*LIBL/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "PNLGRP" => Cmds(&["CRTPNLGRP PNLGRP(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "MENU" => Cmds(&[
            "CRTPNLGRP PNLGRP(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)",
            "CRTMNU MENU(&O/&N) TYPE(*UIM) PNLGRP(&O/&N) REPLACE(&R)",
        ]),
        "RPGLE" => Cmds(&["CRTBNDRPG PGM(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "SQLRPGLE" => {
            Cmds(&["CRTSQLRPGI OBJ(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) OBJTYPE(*PGM) REPLACE(&R)"])
        }
        "RPG" | "RPT" | "RPG38" | "RPT38" => NotInstalled("CRTRPGPGM"),
        "SQLRPG" => NotInstalled("CRTSQLRPG"),
        "CBLLE" => NotInstalled("CRTBNDCBL"),
        "SQLCBLLE" => NotInstalled("CRTSQLCBLI"),
        "CBL" | "CBL38" => NotInstalled("CRTCBLPGM"),
        "SQLCBL" => NotInstalled("CRTSQLCBL"),
        "C" => NotInstalled("CRTBNDC"),
        "CPP" | "CPPLE" => NotInstalled("CRTBNDCPP"),
        _ => return None,
    })
}

/// Option 15's module compiler for a source type.
#[must_use]
pub fn module_compiler(srctype: &str) -> Option<Compiler> {
    use Compiler::{Cmds, NotInstalled};
    Some(match srctype.trim().to_ascii_uppercase().as_str() {
        "CLLE" => Cmds(&["CRTCLMOD MODULE(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "RPGLE" => Cmds(&["CRTRPGMOD MODULE(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) REPLACE(&R)"]),
        "SQLRPGLE" => {
            Cmds(&["CRTSQLRPGI OBJ(&O/&N) SRCFILE(&L/&F) SRCMBR(&N) OBJTYPE(*MODULE) REPLACE(&R)"])
        }
        "CBLLE" => NotInstalled("CRTCBLMOD"),
        "SQLCBLLE" => NotInstalled("CRTSQLCBLI"),
        "C" => NotInstalled("CRTCMOD"),
        "CPP" | "CPPLE" => NotInstalled("CRTCPPMOD"),
        _ => return None,
    })
}

/// A command as a batch job: `SBMJOB` with the job named for the item
/// and the defaults' job description.
#[must_use]
pub fn in_batch(cmd: &str, item: &Item, ctx: &Context<'_>) -> String {
    let d = ctx.defaults;
    let jobd = if d.jobd == "*USRPRF" {
        "*USRPRF".to_string()
    } else {
        let lib = if d.jobd_lib.is_empty() {
            "*LIBL"
        } else {
            &d.jobd_lib
        };
        format!("{lib}/{}", d.jobd)
    };
    format!("SBMJOB CMD({cmd}) JOB({}) JOBD({jobd})", item.name)
}

/// The commands option 14 (`module` false) or 15 runs for a member,
/// substituted and, with `Compile in batch`, submitted.
///
/// # Errors
///
/// The type has no compiler, or its compiler is not installed.
pub fn commands(item: &Item, ctx: &Context<'_>, module: bool) -> Result<Vec<String>, PdmError> {
    let found = if module {
        module_compiler(&item.ty)
    } else {
        compiler(&item.ty)
    };
    match found {
        Some(Compiler::Cmds(list)) => Ok(list
            .iter()
            .map(|t| substitute(t, item, ctx))
            .map(|c| {
                if ctx.defaults.batch_compile {
                    in_batch(&c, item, ctx)
                } else {
                    c
                }
            })
            .collect()),
        Some(Compiler::NotInstalled(_)) => Err(PdmError::NotInstalled(item.ty.clone())),
        None => Err(PdmError::NotForType {
            option: ctx.option.to_string(),
            what: "member type",
            ty: item.ty.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ListKind;
    use crate::defaults::Defaults;

    fn member(ty: &str) -> Item {
        Item {
            library: "SRC".into(),
            file: "QSRC".into(),
            name: "M1".into(),
            ty: ty.into(),
            ..Item::default()
        }
    }

    #[test]
    fn dispatch() {
        let d = Defaults {
            batch_compile: false,
            ..Defaults::default()
        };
        let ctx = Context {
            list: ListKind::Member,
            option: "14",
            defaults: &d,
            object_lib: "OBJ",
            user: "U",
        };
        assert_eq!(
            commands(&member("clle"), &ctx, false).unwrap(),
            ["CRTBNDCL PGM(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) REPLACE(*NO)"]
        );
        assert_eq!(
            commands(&member("PF"), &ctx, false).unwrap(),
            ["CRTPF FILE(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1)"]
        );
        assert_eq!(commands(&member("MENU"), &ctx, false).unwrap().len(), 2);
        assert_eq!(
            commands(&member("rpgle"), &ctx, false).unwrap(),
            ["CRTBNDRPG PGM(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) REPLACE(*NO)"]
        );
        assert_eq!(
            commands(&member("SQLRPGLE"), &ctx, false).unwrap(),
            ["CRTSQLRPGI OBJ(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) OBJTYPE(*PGM) REPLACE(*NO)"]
        );
        assert_eq!(
            commands(&member("RPG"), &ctx, false),
            Err(PdmError::NotInstalled("RPG".into()))
        );
        // A copy member compiles only where it is copied.
        assert!(matches!(
            commands(&member("RPGLEINC"), &ctx, false),
            Err(PdmError::NotForType { .. })
        ));
        assert!(matches!(
            commands(&member("TXT"), &ctx, false),
            Err(PdmError::NotForType { .. })
        ));
        assert_eq!(
            commands(&member("CLLE"), &ctx, true).unwrap(),
            ["CRTCLMOD MODULE(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) REPLACE(*NO)"]
        );
        assert_eq!(
            commands(&member("RPGLE"), &ctx, true).unwrap(),
            ["CRTRPGMOD MODULE(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) REPLACE(*NO)"]
        );
        assert_eq!(
            commands(&member("SQLRPGLE"), &ctx, true).unwrap(),
            ["CRTSQLRPGI OBJ(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) OBJTYPE(*MODULE) REPLACE(*NO)"]
        );
        assert!(commands(&member("PF"), &ctx, true).is_err());
    }

    #[test]
    fn batch() {
        let d = Defaults {
            batch_compile: true,
            jobd: "QBATCH".into(),
            ..Defaults::default()
        };
        let ctx = Context {
            list: ListKind::Member,
            option: "14",
            defaults: &d,
            object_lib: "OBJ",
            user: "U",
        };
        assert_eq!(
            commands(&member("DSPF"), &ctx, false).unwrap(),
            [
                "SBMJOB CMD(CRTDSPF FILE(OBJ/M1) SRCFILE(SRC/QSRC) SRCMBR(M1) REPLACE(*NO)) \
              JOB(M1) JOBD(*LIBL/QBATCH)"
            ]
        );
    }
}
