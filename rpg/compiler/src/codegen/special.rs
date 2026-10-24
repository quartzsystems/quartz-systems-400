// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SPECIAL` files: each operation calls the program `PGMNAME` names
//! with the option (`O` open, `C` close, `R` read, `W` write, `U`
//! update, `D` delete), a status (`0` done, `1` end of file, `2`
//! error), an error code and the record (`_QRNX_SPECIAL`).

use rpgfront::ast::{Expr, Op};

use super::Gen;
use super::emit::quote;
use super::expr::Want;
use crate::files::File;
use crate::names::mangle;

impl Gen<'_> {
    /// Call a `SPECIAL` file's program for one option: the status
    /// register (0, 11 at the end, 1217 when the program cannot be
    /// called at open or close, 1299 otherwise).
    pub fn special_call(&mut self, f: &File, opt: char) -> String {
        let pgm = f.pgmname.clone().unwrap_or_default();
        let (lib, name) = match pgm.split_once('/') {
            Some((l, n)) => (l.to_string(), n.to_string()),
            None => ("*LIBL".to_string(), pgm.clone()),
        };
        self.rt_proc(
            "_QRNX_SPECIAL",
            "(const $pgm: char(10), const $lib: char(10), byval $opt: i32, $area: bytes(1) *VARSIZE, byval $len: i32) -> i32",
        );
        let n = mangle(&f.name);
        let r = self.b.reg();
        self.b.ins(format!(
            "{r} = callb _QRNX_SPECIAL({}, {}, {}, $rb_{n}, {})",
            quote(&format!("{:<10}", name.to_ascii_uppercase())),
            quote(&format!("{:<10}", lib.to_ascii_uppercase())),
            u32::from(opt),
            f.reclen.max(1)
        ));
        r
    }

    /// Open a `SPECIAL` file: its open flag set when the program did.
    pub fn special_open(&mut self, f: &File) -> String {
        let r = self.special_call(f, 'O');
        let po = self.open_flag(f);
        let ok = self.b.reg();
        self.b.ins(format!("{ok} = cmp.eq {r}, 0"));
        self.b.ins(format!("store {po}, {ok}"));
        r
    }

    /// Close a `SPECIAL` file that is open.
    pub fn special_close(&mut self, f: &File) {
        let po = self.open_flag(f);
        let open = self.b.reg();
        self.b.ins(format!("{open} = load {po}"));
        let (yes, no) = (self.b.label(), self.b.label());
        self.b.term(format!("brif {open}, {yes}, {no}"));
        self.b.place(&yes);
        let _ = self.special_call(f, 'C');
        self.b.ins(format!("store {po}, *OFF"));
        self.b.term(format!("br {no}"));
        self.b.place(&no);
    }

    /// `READ`, `WRITE`, `UPDATE`, `DELETE` of a `SPECIAL` file, with a
    /// result structure or the input and output specifications.
    pub fn special_op(&mut self, op: &Op, base: &str, f: &File) {
        let opt = match base {
            "READ" => 'R',
            "WRITE" => 'W',
            "UPDATE" => 'U',
            "DELETE" => 'D',
            _ => {
                self.err(crate::diag::ids::BAD_FILE_OP, 30, f.name.clone());
                return;
            }
        };
        let n = mangle(&f.name);
        // A result structure moves as its characters.
        let rec = match op.result.first() {
            Some(e) => self.expr(e, Want::default()).map(|v| {
                if matches!(v.ty, crate::types::Ty::Ds(_)) { format!("{}.all_", v.op) } else { v.op }
            }),
            None => None,
        };
        if opt != 'R'
            && let Some(r) = &rec
        {
            self.b.ins(format!("mov $rb_{n}, {r}"));
        }
        let st = self.special_call(f, opt);
        if opt == 'R' {
            let eof = self.b.reg();
            self.b.ins(format!("{eof} = cmp.eq {st}, 11"));
            self.b.ins(format!("store $eof_{n}, {eof}"));
            self.b.ins(format!("store $eof_, {eof}"));
            let got = self.b.label();
            let skip = self.b.label();
            let ok = self.b.reg();
            self.b.ins(format!("{ok} = cmp.eq {st}, 0"));
            self.b.term(format!("brif {ok}, {got}, {skip}"));
            self.b.place(&got);
            match &rec {
                Some(r) => self.b.ins(format!("mov {r}, $rb_{n}")),
                None => self.input_specs(f),
            }
            self.b.term(format!("br {skip}"));
            self.b.place(&skip);
        }
        self.db_result(op, base, &n, &st, &super::dbio::KeyArg::None);
        let by_file = matches!(op.f2.first(), Some(Expr::Ref { name, .. }) if *name == f.name);
        self.infds_opcode(f, base, by_file);
    }
}
