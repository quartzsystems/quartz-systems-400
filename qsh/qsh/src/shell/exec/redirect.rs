// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Redirections.

use crate::msg::Msg;
use crate::pattern::path::absolute;
use crate::shell::fds::Saved;
use crate::shell::options::Opt;
use crate::shell::{Flow, Shell};
use crate::syntax::{RedirOp, RedirTarget, Redirect};
use crate::sys::stream::Bytes;
use crate::sys::{Errno, FileType, OpenFlags};

impl Shell {
    /// Apply redirections, returning what to restore. A failure is
    /// reported and returned as `Flow::Fail(1)`, with the redirections
    /// already applied undone.
    pub(crate) fn redirect(&mut self, redirs: &[Redirect]) -> Result<Saved, Flow> {
        let mut saved = Saved::new();
        for r in redirs {
            if let Err(e) = self.redirect_one(r, &mut saved) {
                self.fds.restore(saved);
                return Err(e);
            }
        }
        Ok(saved)
    }

    fn redirect_one(&mut self, r: &Redirect, saved: &mut Saved) -> Result<(), Flow> {
        let fd = r.fd.unwrap_or(r.op.default_fd());
        let new = match (&r.target, r.op) {
            (RedirTarget::Here(cell), _) => {
                let text = match cell.get() {
                    Some(h) => self.expand_here(&h.body)?,
                    None => String::new(),
                };
                Some(Bytes::fd(text.into_bytes()))
            }
            (RedirTarget::Word(w), RedirOp::DupIn | RedirOp::DupOut) => {
                let t = self.expand_string(w)?;
                if t == "-" {
                    None
                } else {
                    match t.parse::<u32>().ok().and_then(|n| self.fds.get(n)) {
                        Some(f) => Some(f),
                        None => {
                            self.report(&Msg::BadFd(t));
                            return Err(Flow::Fail(1));
                        }
                    }
                }
            }
            (RedirTarget::Word(w), op) => {
                let name = self.expand_string(w)?;
                let path = absolute(&self.cwd, &name);
                let flags = match op {
                    RedirOp::In => OpenFlags {
                        read: true,
                        ..OpenFlags::default()
                    },
                    RedirOp::Out | RedirOp::Clobber => OpenFlags {
                        write: true,
                        create: true,
                        truncate: true,
                        ..OpenFlags::default()
                    },
                    RedirOp::Append => OpenFlags {
                        write: true,
                        create: true,
                        append: true,
                        ..OpenFlags::default()
                    },
                    _ => OpenFlags {
                        read: true,
                        write: true,
                        create: true,
                        ..OpenFlags::default()
                    },
                };
                if op == RedirOp::Out
                    && self.opts.get(Opt::NoClobber)
                    && matches!(self.sys.stat(&path), Ok(st) if st.ftype == FileType::File)
                {
                    self.report(&Msg::Open(name, Errno::EEXIST.text().to_string()));
                    return Err(Flow::Fail(1));
                }
                match self.sys.open(&path, flags, 0o666 & !self.umask) {
                    Ok(f) => Some(f),
                    Err(e) => {
                        self.report(&Msg::Open(name, e.to_string()));
                        return Err(Flow::Fail(1));
                    }
                }
            }
        };
        let old = match new {
            Some(f) => self.fds.set(fd, f),
            None => self.fds.close(fd),
        };
        saved.push((fd, old));
        Ok(())
    }
}
