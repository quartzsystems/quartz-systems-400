// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Building an archive from operands: walk, rename, filter, encode.

use qsh::sys::UtilCtx;

use super::codec::{self, Format};
use super::gather::{Found, Gather, Links, member, member_name};
use super::member::Member;
use super::subst::{Subst, rename};
use crate::common::{complain, complain_sys};

/// How to build.
pub struct Build<'a> {
    /// The utility's name for messages.
    pub util: &'a str,
    /// The format.
    pub format: Format,
    /// How to walk.
    pub gather: Gather,
    /// `-s` expressions.
    pub subs: &'a [Subst],
    /// Write each name to standard error.
    pub verbose: bool,
    /// The archive file itself, never archived.
    pub skip: Option<String>,
}

impl Build<'_> {
    /// Encode every operand's tree after `bytes`. `keep(found, name)`
    /// decides whether a path goes in. The result is the exit status.
    pub fn add(
        &self,
        cx: &UtilCtx<'_>,
        ops: &[String],
        bytes: &mut Vec<u8>,
        keep: &mut dyn FnMut(&Found, &str) -> bool,
    ) -> i32 {
        let mut status = 0;
        let mut links = Links::default();
        for op in ops {
            self.gather.walk(cx, op, &mut |r| {
                match r {
                    Ok(found) => {
                        if let Some(s) = &self.skip
                            && cx.sys.realpath(&found.abs).is_ok_and(|r| &r == s)
                        {
                            return true;
                        }
                        let base = member_name(&found.name);
                        let (name, note) = rename(self.subs, &base);
                        if let Some(n) = note {
                            let _ = cx.err(&n);
                        }
                        if name.is_empty() || !keep(&found, &name) {
                            return true;
                        }
                        match member(cx, &found, &name, &mut links) {
                            Ok(m) => {
                                if !self.put(cx, &m, bytes) {
                                    status = 1;
                                }
                            }
                            Err(e) => {
                                complain_sys(cx, self.util, &found.name, e);
                                status = 1;
                            }
                        }
                    }
                    Err((p, e)) => {
                        complain_sys(cx, self.util, &p, e);
                        status = 1;
                    }
                }
                true
            });
        }
        status
    }

    /// Encode one member; `false` after a message when the format
    /// cannot hold it.
    pub fn put(&self, cx: &UtilCtx<'_>, m: &Member, bytes: &mut Vec<u8>) -> bool {
        match codec::encode(m, self.format) {
            Ok(b) => {
                bytes.extend(b);
                if self.verbose {
                    let _ = cx.err(&format!("{}\n", m.name));
                }
                true
            }
            Err(t) => {
                complain(cx, self.util, &format!("{}: {t}", m.name));
                false
            }
        }
    }
}
