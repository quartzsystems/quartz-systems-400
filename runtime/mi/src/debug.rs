// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The debug core: `STRDBG` makes a job run a program's debug variant
//! (`ILE.md` §2, section 5), whose statements call the runtime; the
//! debugger stops at breakpoints and steps, hands each stop to a hook
//! (the console, or a test's closure) and evaluates the variables the
//! stopped entry registered, formatted the way `EVAL` shows them.

use std::fmt::Write as _;

use codegen::abi::TypeDesc;
use qir::{Type, TypeClass};

use crate::job::Job;
use crate::space::{FatPtr, PtrKind};

/// Where the job stopped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stop {
    /// `LIB/NAME`.
    pub program: String,
    /// Procedure.
    pub procedure: String,
    /// Statement.
    pub stmt: u32,
    /// Call-stack depth.
    pub depth: u32,
}

/// What the hook decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    /// Run to the next breakpoint.
    Go,
    /// Stop at the next statement at this depth or above.
    StepOver,
    /// Stop at the next statement anywhere.
    StepInto,
}

/// Where the next stop is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// At a breakpoint only.
    None,
    /// At the next statement at or above this depth.
    Over(u32),
    /// At the next statement.
    Into,
}

/// What runs at a stop.
pub type Hook = Box<dyn FnMut(&mut Job, &Stop) -> Resume>;

/// A variable the stopped entry registered (`debug.var`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbgVar {
    /// Source name.
    pub name: String,
    /// Address.
    pub addr: u64,
    /// Type descriptor.
    pub desc: u64,
}

/// The job's debug state.
pub struct Debugger {
    /// Programs under debug, `LIB/NAME`.
    pub programs: Vec<String>,
    /// Breakpoints: program (`LIB/NAME` or `NAME`) and statement.
    pub breakpoints: Vec<(String, u32)>,
    /// Where the next stop is.
    pub step: Step,
    /// What runs at a stop.
    pub hook: Option<Hook>,
    /// Every stop so far.
    pub stops: Vec<Stop>,
}

impl Default for Debugger {
    fn default() -> Self {
        Self {
            programs: Vec::new(),
            breakpoints: Vec::new(),
            step: Step::None,
            hook: None,
            stops: Vec::new(),
        }
    }
}

impl Debugger {
    /// Whether statement `stmt` of `program` has a breakpoint.
    #[must_use]
    pub fn breaks_at(&self, program: &str, stmt: u32) -> bool {
        let short = program.rsplit('/').next().unwrap_or(program);
        self.breakpoints
            .iter()
            .any(|(p, s)| *s == stmt && (p == program || p == short))
    }
}

impl Job {
    /// `STRDBG`: run `lib/name` as its debug variant from its next
    /// call. A program with an entry on the stack cannot be debugged;
    /// one without a debug variant runs as it is, with no stops.
    pub fn strdbg(&mut self, name: &str, lib: &str) -> Result<String, String> {
        let (_, found) = self
            .act
            .source
            .program(name, lib, ile::PgmKind::Pgm)
            .ok_or_else(|| format!("{lib}/{name}: no such program"))?;
        let key = format!("{found}/{name}");
        if self
            .act
            .frames
            .iter()
            .any(|f| f.program.as_ref().is_some_and(|p| p.key == key))
        {
            return Err(format!("{key} is active on the call stack"));
        }
        let d = self.debug.get_or_insert_with(Debugger::default);
        if !d.programs.contains(&key) {
            d.programs.push(key.clone());
        }
        self.act.debug_programs.clone_from(&d.programs);
        self.act.loaded.remove(&key);
        Ok(key)
    }

    /// `ENDDBG`: every program runs its production code again.
    pub fn enddbg(&mut self) {
        if let Some(d) = self.debug.take() {
            for key in &d.programs {
                self.act.loaded.remove(key);
            }
        }
        self.act.debug_programs.clear();
    }

    /// `ADDBKP`: a breakpoint at a statement of a program.
    pub fn addbkp(&mut self, program: &str, stmt: u32) {
        let d = self.debug.get_or_insert_with(Debugger::default);
        if !d
            .breakpoints
            .iter()
            .any(|(p, s)| p == program && *s == stmt)
        {
            d.breakpoints.push((program.to_string(), stmt));
        }
    }

    /// `RMVBKP`: how many went.
    pub fn rmvbkp(&mut self, program: &str, stmt: Option<u32>) -> usize {
        let Some(d) = self.debug.as_mut() else {
            return 0;
        };
        let before = d.breakpoints.len();
        d.breakpoints
            .retain(|(p, s)| !(p == program && stmt.is_none_or(|n| n == *s)));
        before - d.breakpoints.len()
    }

    /// What runs at a stop.
    pub fn set_debug_hook(&mut self, hook: Hook) {
        self.debug.get_or_insert_with(Debugger::default).hook = Some(hook);
    }

    /// The variables the running entry registered, with their types.
    #[must_use]
    pub fn dspvar(&self) -> Vec<(String, String)> {
        let Some(f) = self.act.frames.last() else {
            return Vec::new();
        };
        f.debug_vars
            .iter()
            .map(|v| {
                let t = TypeDesc::from_word(v.desc.cast_signed())
                    .decode()
                    .map_or_else(|| "?".to_string(), |t| t.to_string());
                (v.name.clone(), t)
            })
            .collect()
    }

    /// `EVAL`: a variable of the running entry, `NAME`, `NAME:x`
    /// (hexadecimal) or `NAME:c` (characters).
    pub fn eval(&self, expr: &str) -> Result<String, String> {
        let (name, mode) = match expr.split_once(':') {
            Some((n, m)) => (n.trim(), m.trim().to_ascii_lowercase()),
            None => (expr.trim(), String::new()),
        };
        // A CL variable is spelled `&NAME`; its place is `$NAME`.
        let name = name.strip_prefix('&').unwrap_or(name);
        let f = self
            .act
            .frames
            .last()
            .ok_or_else(|| "no program is running".to_string())?;
        let v = f
            .debug_vars
            .iter()
            .rev()
            .find(|v| v.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| format!("{name} is not a variable of the stopped entry"))?;
        let t = TypeDesc::from_word(v.desc.cast_signed())
            .decode()
            .ok_or_else(|| format!("{name}: its type cannot be shown"))?;
        let len = usize::try_from(t.size()).unwrap_or(0);
        let bytes = crate::storage::view(v.addr, len);
        Ok(match mode.as_str() {
            "x" => hex_dump(bytes),
            "c" => format!("'{}'", ccsid::from_ebcdic(bytes)),
            _ => format_value(v, &t, bytes),
        })
    }
}

/// `EVAL`'s view of a value.
fn format_value(v: &DbgVar, t: &Type, bytes: &[u8]) -> String {
    match t.class() {
        TypeClass::Char | TypeClass::Graphic if !matches!(t, Type::VarChar(_)) => {
            format!("'{}'", ccsid::from_ebcdic(bytes))
        }
        TypeClass::Char | TypeClass::Ucs2 | TypeClass::Graphic | TypeClass::DateTime => {
            format!(
                "'{}'",
                crate::rtlib::display_text(v.addr.cast_signed(), v.desc.cast_signed())
            )
        }
        TypeClass::Int | TypeClass::UInt | TypeClass::Float | TypeClass::Decimal => {
            crate::rtlib::display_text(v.addr.cast_signed(), v.desc.cast_signed())
        }
        TypeClass::Ind => {
            if bytes.first() == Some(&0xF1) {
                "'1'".into()
            } else {
                "'0'".into()
            }
        }
        TypeClass::Ptr | TypeClass::ProcPtr => {
            let p = FatPtr::from_bytes(&bytes.try_into().unwrap_or([0; 16]));
            match p.kind() {
                Some(PtrKind::Null) | None => "SPP:*NULL".into(),
                Some(PtrKind::Space) => format!("SPP:{:08X}{:08X}", p.space, p.offset),
                Some(PtrKind::System) => format!("SYP:{:016X}", p.offset),
                Some(PtrKind::Procedure) => format!("PRP:{:016X}", p.offset),
            }
        }
        TypeClass::Aggregate => hex_dump(bytes),
    }
}

/// Hexadecimal lines of sixteen bytes with their characters.
#[must_use]
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::new();
    for (i, chunk) in bytes.chunks(16).enumerate() {
        let _ = write!(out, "{:04X}  ", i * 16);
        for (k, b) in chunk.iter().enumerate() {
            let _ = write!(out, "{b:02X}");
            if k % 4 == 3 {
                out.push(' ');
            }
        }
        let pad = (16 - chunk.len()) * 2 + (16 - chunk.len()) / 4;
        out.extend(std::iter::repeat_n(' ', pad));
        out.push_str("  *");
        for b in chunk {
            let c = ccsid::char_of(*b);
            out.push(if c.is_ascii_graphic() || c == ' ' {
                c
            } else {
                '.'
            });
        }
        out.push_str("*\n");
    }
    out.trim_end().to_string()
}
