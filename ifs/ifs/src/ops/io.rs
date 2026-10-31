// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `read()`, `write()`, `pread()`, `pwrite()`, `lseek()`, `fsync()`
//! and `ftruncate()` on descriptors.

use super::flags as f;
use super::{Ctx, Ifs, Open, OpenKind, Rest};
use crate::backend::{BFile, Backend};
use crate::errno::{Errno, Result};

/// Largest offset a file may reach.
const MAX_OFF: u64 = i64::MAX as u64;

impl<B: Backend> Ifs<B> {
    fn open_of<'r>(
        rest: &'r mut Rest<B::File>,
        cx: &Ctx<'_>,
        fd: i32,
    ) -> Result<&'r mut Open<B::File>> {
        let id = rest
            .owners
            .get(&cx.owner)
            .and_then(|o| o.fds.get(&fd))
            .ok_or(Errno::EBADF)?
            .open;
        rest.opens.get_mut(&id).ok_or(Errno::EBADF)
    }

    /// An open object's size.
    pub(crate) fn size_of(o: &Open<B::File>) -> Result<u64> {
        Ok(match &o.kind {
            OpenKind::File(file) => file.stat()?.size,
            OpenKind::Dev(_) | OpenKind::Dir => 0,
            OpenKind::Member { data, .. } | OpenKind::Stream { data, .. } => data.len() as u64,
            OpenKind::Opt { data: Some(d), .. } => d.len() as u64,
            OpenKind::Opt { size, .. } => *size,
        })
    }

    /// Read file bytes at `pos`.
    fn raw_read(&self, o: &mut Open<B::File>, buf: &mut [u8], pos: u64) -> Result<usize> {
        match &mut o.kind {
            OpenKind::File(file) => file.read_at(buf, pos),
            OpenKind::Dev(d) => d.read(buf),
            OpenKind::Member { data, .. }
            | OpenKind::Stream { data, .. }
            | OpenKind::Opt {
                data: Some(data), ..
            } => {
                let Ok(p) = usize::try_from(pos) else {
                    return Ok(0);
                };
                if p >= data.len() {
                    return Ok(0);
                }
                let n = buf.len().min(data.len() - p);
                buf[..n].copy_from_slice(&data[p..p + n]);
                Ok(n)
            }
            OpenKind::Opt { vol, path, .. } => self.opt.read(vol, path, pos, buf),
            OpenKind::Dir => Err(Errno::EISDIR),
        }
    }

    /// Write file bytes at `pos`.
    pub(crate) fn raw_write(o: &mut Open<B::File>, data: &[u8], pos: u64) -> Result<usize> {
        if pos.saturating_add(data.len() as u64) > MAX_OFF {
            return Err(Errno::EFBIG);
        }
        let n = match &mut o.kind {
            OpenKind::File(file) => {
                let mut done = 0;
                while done < data.len() {
                    let n = file.write_at(&data[done..], pos + done as u64)?;
                    if n == 0 {
                        return Err(Errno::EIO);
                    }
                    done += n;
                }
                if o.oflag & (f::O_SYNC | f::O_DSYNC) != 0 {
                    file.sync(o.oflag & f::O_SYNC == 0)?;
                }
                done
            }
            OpenKind::Dev(d) => d.write(data)?,
            OpenKind::Member {
                data: buf, dirty, ..
            }
            | OpenKind::Stream {
                data: buf, dirty, ..
            }
            | OpenKind::Opt {
                data: Some(buf),
                dirty,
                ..
            } => {
                let p = usize::try_from(pos).map_err(|_| Errno::EFBIG)?;
                if buf.len() < p + data.len() {
                    buf.resize(p + data.len(), 0);
                }
                buf[p..p + data.len()].copy_from_slice(data);
                *dirty = true;
                data.len()
            }
            OpenKind::Opt { .. } => return Err(Errno::EROFS),
            OpenKind::Dir => return Err(Errno::EISDIR),
        };
        Ok(n)
    }

    /// `read()`.
    pub fn read(&self, cx: &Ctx<'_>, fd: i32, buf: &mut [u8]) -> Result<usize> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if !o.access.read {
            return Err(Errno::EBADF);
        }
        if buf.is_empty() {
            return Ok(0);
        }
        let Some(mut text) = o.text.take() else {
            let pos = o.pos;
            let n = self.raw_read(o, buf, pos)?;
            o.pos += n as u64;
            return Ok(n);
        };
        let result = (|| {
            if text.has_pending() {
                return Ok(text.serve(buf));
            }
            loop {
                let mut raw = vec![0u8; buf.len().max(4)];
                let pos = o.pos;
                let n = self.raw_read(o, &mut raw, pos)?;
                o.pos += n as u64;
                let got = text.convert_read(&raw[..n], n == 0, buf);
                // A partial character alone gives nothing yet: read on.
                if got > 0 || n == 0 {
                    return Ok(got);
                }
            }
        })();
        o.text = Some(text);
        result
    }

    /// `write()`.
    pub fn write(&self, cx: &Ctx<'_>, fd: i32, data: &[u8]) -> Result<usize> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if !o.access.write {
            return Err(Errno::EBADF);
        }
        if o.oflag & f::O_APPEND != 0 {
            o.pos = Self::size_of(o)?;
        }
        let raw = match &mut o.text {
            Some(t) => t.convert_write(data),
            None => data.to_vec(),
        };
        let pos = o.pos;
        let n = Self::raw_write(o, &raw, pos)?;
        o.pos += n as u64;
        Ok(if o.text.is_some() { data.len() } else { n })
    }

    /// `pread()`: at `off`, the position unchanged.
    pub fn pread(&self, cx: &Ctx<'_>, fd: i32, buf: &mut [u8], off: u64) -> Result<usize> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if !o.access.read {
            return Err(Errno::EBADF);
        }
        if matches!(o.kind, OpenKind::Dev(_)) {
            return Err(Errno::ESPIPE);
        }
        match o.text.take() {
            None => self.raw_read(o, buf, off),
            Some(t) => {
                let mut raw = vec![0u8; buf.len()];
                let r = self.raw_read(o, &mut raw, off).map(|n| {
                    let conv = t.convert_once(&raw[..n]);
                    let k = conv.len().min(buf.len());
                    buf[..k].copy_from_slice(&conv[..k]);
                    k
                });
                o.text = Some(t);
                r
            }
        }
    }

    /// `pwrite()`: at `off`, the position unchanged.
    pub fn pwrite(&self, cx: &Ctx<'_>, fd: i32, data: &[u8], off: u64) -> Result<usize> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if !o.access.write {
            return Err(Errno::EBADF);
        }
        if matches!(o.kind, OpenKind::Dev(_)) {
            return Err(Errno::ESPIPE);
        }
        let raw = match &o.text {
            Some(t) => t.convert_once_out(data),
            None => data.to_vec(),
        };
        let n = Self::raw_write(o, &raw, off)?;
        Ok(if o.text.is_some() { data.len() } else { n })
    }

    /// `lseek()`.
    pub fn lseek(&self, cx: &Ctx<'_>, fd: i32, off: i64, whence: i32) -> Result<u64> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if matches!(o.kind, OpenKind::Dev(_)) {
            return Ok(0);
        }
        let base = match whence {
            f::SEEK_SET => 0,
            f::SEEK_CUR => o.pos,
            f::SEEK_END => Self::size_of(o)?,
            _ => return Err(Errno::EINVAL),
        };
        let base = i64::try_from(base).map_err(|_| Errno::EOVERFLOW)?;
        let new = base.checked_add(off).ok_or(Errno::EOVERFLOW)?;
        let new = u64::try_from(new).map_err(|_| Errno::EINVAL)?;
        o.pos = new;
        if let Some(t) = &mut o.text {
            t.reset();
        }
        Ok(new)
    }

    /// `fsync()` (`data_only`: `fdatasync()`); a member or stream is
    /// written back.
    pub fn fsync(&self, cx: &Ctx<'_>, fd: i32, data_only: bool) -> Result<()> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if let OpenKind::File(file) = &o.kind {
            return file.sync(data_only);
        }
        self.flush(o)
    }

    /// `ftruncate()`.
    pub fn ftruncate(&self, cx: &Ctx<'_>, fd: i32, len: u64) -> Result<()> {
        let mut st = self.lock();
        let o = Self::open_of(&mut st.rest, cx, fd)?;
        if !o.access.write {
            return Err(Errno::EBADF);
        }
        if len > MAX_OFF {
            return Err(Errno::EFBIG);
        }
        match &mut o.kind {
            OpenKind::File(file) => file.set_len(len),
            OpenKind::Member { data, dirty, .. }
            | OpenKind::Stream { data, dirty, .. }
            | OpenKind::Opt {
                data: Some(data),
                dirty,
                ..
            } => {
                data.resize(usize::try_from(len).map_err(|_| Errno::EFBIG)?, 0);
                *dirty = true;
                Ok(())
            }
            OpenKind::Dev(_) | OpenKind::Opt { .. } => Err(Errno::EINVAL),
            OpenKind::Dir => Err(Errno::EISDIR),
        }
    }
}
