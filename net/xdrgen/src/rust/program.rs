// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Programs: their numbers as constants and a module per program with
//! a module per version, holding the procedure table and the typed
//! `Args` and `Res` of every procedure.

use std::collections::HashSet;

use super::names::ident;
use super::{W, rust_type};
use crate::resolve::ir::{RProc, RProgram, Shape};

/// Writes program `p`.
pub fn write(w: &mut W, p: &RProgram) {
    let file = w.file.clone();
    let rt = w.rt.clone();
    w.line(&format!("/// The program `{}` (`{file}`).", p.name));
    w.line(&format!(
        "pub const {}: u32 = {};",
        ident(&p.name),
        p.number
    ));
    let mut seen = HashSet::new();
    for v in &p.versions {
        w.line(&format!("/// Version `{}` of `{}`.", v.name, p.name));
        w.line(&format!(
            "pub const {}: u32 = {};",
            ident(&v.name),
            v.number
        ));
        for pr in &v.procs {
            if seen.insert(pr.name.clone()) {
                w.line(&format!("/// The procedure `{}` of `{}`.", pr.name, p.name));
                w.line(&format!(
                    "pub const {}: u32 = {};",
                    ident(&pr.name),
                    pr.number
                ));
            }
        }
    }
    w.line(&format!("/// The procedures of `{}`, by version.", p.name));
    w.line(&format!("pub mod {} {{", ident(&p.name)));
    w.line("    /// The program number.");
    w.line(&format!("    pub const NUMBER: u32 = {};", p.number));
    let vs: Vec<String> = p.versions.iter().map(|v| v.number.to_string()).collect();
    w.line("    /// The versions described.");
    w.line(&format!(
        "    pub const VERSIONS: &[u32] = &[{}];",
        vs.join(", ")
    ));
    for v in &p.versions {
        w.line(&format!("    /// Version `{}` ({}).", v.name, v.number));
        w.line(&format!("    pub mod {} {{", ident(&v.name)));
        w.line("        /// The version number.");
        w.line(&format!("        pub const NUMBER: u32 = {};", v.number));
        w.line("        /// The procedures: number and name.");
        w.line("        pub const PROCEDURES: &[(u32, &str)] = &[");
        for pr in &v.procs {
            w.line(&format!("            ({}, \"{}\"),", pr.number, pr.name));
        }
        w.line("        ];");
        body(
            w,
            &rt,
            p.number,
            v.number,
            &v.procs,
            "Args",
            "arguments",
            |pr| pr.arg.as_ref(),
        );
        body(
            w,
            &rt,
            p.number,
            v.number,
            &v.procs,
            "Res",
            "results",
            |pr| pr.result.as_ref(),
        );
        w.line("    }");
    }
    w.line("}");
}

/// The enum of every procedure's arguments or results.
#[allow(clippy::too_many_arguments)]
fn body(
    w: &mut W,
    rt: &str,
    prog: u32,
    vers: u32,
    procs: &[RProc],
    name: &str,
    what: &str,
    pick: impl Fn(&RProc) -> Option<&Shape>,
) {
    w.line(&format!("        /// A call's {what}, by procedure."));
    w.line("        #[derive(Debug, Clone, PartialEq, Eq)]");
    w.line(&format!("        pub enum {name} {{"));
    for pr in procs {
        if let Some(s) = pick(pr) {
            w.line(&format!("            /// `{}` ({}).", pr.name, pr.number));
            w.line(&format!(
                "            {}({}),",
                ident(&pr.name),
                rust_type(s, "super::super::")
            ));
        } else {
            w.line(&format!(
                "            /// `{}` ({}): void.",
                pr.name, pr.number
            ));
            w.line(&format!("            {},", ident(&pr.name)));
        }
    }
    w.line("        }");
    w.line(&format!("        impl {rt}::ProcBody for {name} {{"));
    w.line(&format!("            const PROGRAM: u32 = {prog};"));
    w.line(&format!("            const VERSION: u32 = {vers};"));
    w.line("            fn procedure(&self) -> u32 {");
    w.line("                match self {");
    for pr in procs {
        let pat = if pick(pr).is_some() { "(..)" } else { "" };
        w.line(&format!(
            "                    Self::{}{pat} => {},",
            ident(&pr.name),
            pr.number
        ));
    }
    w.line("                }");
    w.line("            }");
    // A version whose procedures all have void here never reads or writes.
    let (e, d) = if procs.iter().any(|p| pick(p).is_some()) {
        ("e", "d")
    } else {
        ("_e", "_d")
    };
    w.line(&format!(
        "            fn encode_body(&self, {e}: &mut {rt}::Encoder) -> {rt}::Result<()> {{"
    ));
    w.line("                match self {");
    for pr in procs {
        match pick(pr) {
            Some(_) => w.line(&format!(
                "                    Self::{}(v) => {rt}::Xdr::encode(v, e),",
                ident(&pr.name)
            )),
            None => w.line(&format!(
                "                    Self::{} => Ok(()),",
                ident(&pr.name)
            )),
        }
    }
    w.line("                }");
    w.line("            }");
    w.line(&format!(
        "            fn decode_body(procedure: u32, {d}: &mut {rt}::Decoder<'_>) -> {rt}::Result<Option<Self>> {{"
    ));
    w.line("                Ok(Some(match procedure {");
    for pr in procs {
        match pick(pr) {
            Some(_) => w.line(&format!(
                "                    {} => Self::{}({rt}::Xdr::decode(d)?),",
                pr.number,
                ident(&pr.name)
            )),
            None => w.line(&format!(
                "                    {} => Self::{},",
                pr.number,
                ident(&pr.name)
            )),
        }
    }
    w.line("                    _ => return Ok(None),");
    w.line("                }))");
    w.line("            }");
    w.line("        }");
}
