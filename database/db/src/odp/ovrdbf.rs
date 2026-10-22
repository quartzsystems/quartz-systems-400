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
    /// `SECURE(*YES)`: overrides from lower call levels are not merged
    /// under this one.
    pub secure: bool,
    /// The call level that issued it: 0 at a command line, the depth of
    /// the program otherwise. A program's overrides go when it returns.
    pub level: u32,
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
            level: 0,
            scope: DFTACTGRP.into(),
        }
    }

    /// `OVRSCOPE(*JOB)` or the activation group that issued it.
    #[must_use]
    pub fn scope(mut self, scope: &str) -> Self {
        scope.clone_into(&mut self.scope);
        self
    }

    /// Issued at call level `level`.
    #[must_use]
    pub fn level(mut self, level: u32) -> Self {
        self.level = level;
        self
    }

    /// Take from `outer`, an override of a lower call level, what this
    /// one does not say.
    pub fn merge_under(&mut self, outer: &Self) {
        fn fill<T: Clone>(mine: &mut Option<T>, theirs: Option<&T>) {
            if mine.is_none() {
                *mine = theirs.cloned();
            }
        }
        fill(&mut self.tofile, outer.tofile.as_ref());
        fill(&mut self.member, outer.member.as_ref());
        fill(&mut self.position, outer.position.as_ref());
        fill(&mut self.seqonly, outer.seqonly.as_ref());
        fill(&mut self.nbrrcds, outer.nbrrcds.as_ref());
        fill(&mut self.share, outer.share.as_ref());
        fill(&mut self.wait_rcd, outer.wait_rcd.as_ref());
        fill(&mut self.lvlchk, outer.lvlchk.as_ref());
        fill(&mut self.inhwrt, outer.inhwrt.as_ref());
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
