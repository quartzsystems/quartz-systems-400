// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `OVRDBF`: what a job says about a file before a program opens it.

use dds::FileRef;
use qsname::{MemberSpec, Name};

use crate::file::Wait;
use crate::record::Value;

/// `POSITION`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Position {
    /// `*START`.
    Start,
    /// `*END`.
    End,
    /// `*RRN n`: the first read returns record `n`.
    Rrn(u64),
    /// `*KEYB`: the first read returns the first record whose key is at
    /// or above the values.
    KeyBefore(Vec<Value>),
    /// `*KEYBE`: the first read returns the last record whose key is at
    /// or below the values.
    KeyBeforeEqual(Vec<Value>),
    /// `*KEYA`: the first read returns the first record whose key is
    /// above the values.
    KeyAfter(Vec<Value>),
    /// `*KEYAE`: the first read returns the record equal to the values,
    /// else the first above.
    KeyAfterEqual(Vec<Value>),
}

/// The default activation group, where command-level overrides live.
pub const DFTACTGRP: &str = "*DFTACTGRP";

/// Whether an override issued in `scope` reaches an open from
/// activation group `opener`.
#[must_use]
pub fn reaches(scope: &str, opener: &str) -> bool {
    scope == "*JOB" || scope == DFTACTGRP || scope == opener
}

/// One override. Every `None` means "as the file or program has it".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Override {
    /// `FILE`: the name the program opens.
    pub file: Name,
    /// `TOFILE`.
    pub tofile: Option<FileRef>,
    /// `MBR`.
    pub member: Option<MemberSpec>,
    /// `POSITION`.
    pub position: Option<Position>,
    /// `SEQONLY(*YES n)` as `Some(n)`, `SEQONLY(*NO)` as `Some(0)`.
    pub seqonly: Option<u32>,
    /// `NBRRCDS`.
    pub nbrrcds: Option<u32>,
    /// `SHARE`.
    pub share: Option<bool>,
    /// `WAITRCD`.
    pub wait_rcd: Option<Wait>,
    /// `LVLCHK`.
    pub lvlchk: Option<bool>,
    /// `INHWRT`.
    pub inhwrt: Option<bool>,
    /// `SECURE(*YES)`: overrides from higher call levels are ignored.
    /// Kept; overrides come from command level until programs run
    /// commands in place (`MIGRATE(phase7)`).
    pub secure: bool,
    /// `OVRSCOPE`: `*JOB`, or the activation group that issued it
    /// (`*ACTGRPDFN`). One issued from the default activation group
    /// (command level) reaches every open in the job.
    pub scope: String,
}

impl Override {
    /// An override of `file` saying nothing else yet.
    #[must_use]
    pub fn new(file: Name) -> Self {
        Self {
            file,
            tofile: None,
            member: None,
            position: None,
            seqonly: None,
            nbrrcds: None,
            share: None,
            wait_rcd: None,
            lvlchk: None,
            inhwrt: None,
            secure: false,
            scope: DFTACTGRP.into(),
        }
    }

    /// `TOFILE(lib/file)`.
    #[must_use]
    pub fn tofile(mut self, to: FileRef) -> Self {
        self.tofile = Some(to);
        self
    }

    /// `MBR(name)`.
    #[must_use]
    pub fn member(mut self, m: MemberSpec) -> Self {
        self.member = Some(m);
        self
    }

    /// `POSITION(...)`.
    #[must_use]
    pub fn position(mut self, p: Position) -> Self {
        self.position = Some(p);
        self
    }

    /// `SHARE(*YES)` or `SHARE(*NO)`.
    #[must_use]
    pub fn share(mut self, yes: bool) -> Self {
        self.share = Some(yes);
        self
    }

    /// `INHWRT(*YES)` or `*NO`.
    #[must_use]
    pub fn inhwrt(mut self, yes: bool) -> Self {
        self.inhwrt = Some(yes);
        self
    }

    /// `LVLCHK(*YES)` or `*NO`.
    #[must_use]
    pub fn lvlchk(mut self, yes: bool) -> Self {
        self.lvlchk = Some(yes);
        self
    }

    /// `WAITRCD`.
    #[must_use]
    pub fn wait_rcd(mut self, w: Wait) -> Self {
        self.wait_rcd = Some(w);
        self
    }

    /// `SEQONLY(*YES n)`.
    #[must_use]
    pub fn seqonly(mut self, n: u32) -> Self {
        self.seqonly = Some(n);
        self
    }

    /// `NBRRCDS(n)`.
    #[must_use]
    pub fn nbrrcds(mut self, n: u32) -> Self {
        self.nbrrcds = Some(n);
        self
    }

    /// `SECURE(*YES)`.
    #[must_use]
    pub fn secure(mut self) -> Self {
        self.secure = true;
        self
    }
}
