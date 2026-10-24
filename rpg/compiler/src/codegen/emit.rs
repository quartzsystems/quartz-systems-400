// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The IR text of one procedure as it is built: its automatic
//! declarations and its blocks. An instruction after a terminator opens
//! a fresh block, so dead code after `LEAVE` or `RETURN` stays valid IR.

/// A procedure body under construction.
#[derive(Clone, Debug, Default)]
pub struct Body {
    /// `auto` and `static` declarations, without indentation.
    pub decls: Vec<String>,
    /// Block labels and instructions.
    pub lines: Vec<String>,
    /// The current block takes instructions.
    open: bool,
    /// Labels made.
    labels: u32,
    /// Temporaries made.
    temps: u32,
    /// Registers made.
    regs: u32,
    /// Temporaries that hold a quotient (temporary names restart in
    /// every procedure, so the set does too).
    pub quotients: std::collections::BTreeSet<String>,
}

impl Body {
    /// A body whose first block is `L0`.
    #[must_use]
    pub fn new() -> Self {
        let mut b = Self::default();
        b.lines.push("L0:".into());
        b.open = true;
        b
    }

    /// A new label name.
    pub fn label(&mut self) -> String {
        self.labels += 1;
        format!("B{}", self.labels)
    }

    /// A new register name.
    pub fn reg(&mut self) -> String {
        self.regs += 1;
        format!("%r{}", self.regs)
    }

    /// A new automatic temporary of an IR type.
    pub fn temp(&mut self, ir_type: &str) -> String {
        self.temps += 1;
        let name = format!("$t{}", self.temps);
        self.decls.push(format!("auto {name}: {ir_type}"));
        name
    }

    /// Append an instruction to the current block.
    pub fn ins(&mut self, s: impl Into<String>) {
        if !self.open {
            let l = self.label();
            self.lines.push(format!("{l}:"));
            self.open = true;
        }
        self.lines.push(format!("  {}", s.into()));
    }

    /// End the current block with a terminator.
    pub fn term(&mut self, s: impl Into<String>) {
        self.ins(s);
        self.open = false;
    }

    /// Start block `l`, falling into it from the current one.
    pub fn place(&mut self, l: &str) {
        if self.open {
            self.lines.push(format!("  br {l}"));
        }
        self.lines.push(format!("{l}:"));
        self.open = true;
    }

    /// Whether the current block is still open.
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// The procedure text.
    #[must_use]
    pub fn finish(mut self, header: &str, end: &str) -> String {
        if self.open {
            self.lines.push(format!("  {end}"));
        }
        let mut s = format!("{header} {{\n");
        for d in &self.decls {
            s.push_str("  ");
            s.push_str(d);
            s.push('\n');
        }
        for l in &self.lines {
            s.push_str(l);
            s.push('\n');
        }
        s.push_str("}\n");
        s
    }
}

/// A string as an IR literal.
#[must_use]
pub fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
