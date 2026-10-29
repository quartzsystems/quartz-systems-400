// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Reading and writing through a utility's descriptors and the `Sys`:
//! opening operands (`-` is standard input), reading whole files or
//! lines, and buffered output that notices a closed pipe.

use std::io;

use qsh::sys::stream::{read_to_end, write_all};
use qsh::sys::{Errno, Fd, FileType, OpenFlags, SysError, UtilCtx};

/// Flags to open for reading.
pub const READ: OpenFlags = OpenFlags {
    read: true,
    write: false,
    create: false,
    truncate: false,
    append: false,
    excl: false,
};

/// Open a path for reading (absolute against the working directory).
pub fn open_read(cx: &UtilCtx<'_>, path: &str) -> Result<Fd, SysError> {
    let abs = cx.path(path);
    if cx.sys.stat(&abs).is_ok_and(|st| st.ftype == FileType::Dir) {
        return Err(Errno::EISDIR.into());
    }
    cx.sys.open(&abs, READ, 0)
}

/// Open a path for writing, created with `0666` less the umask;
/// truncated unless `append`.
pub fn open_write(cx: &UtilCtx<'_>, path: &str, append: bool) -> Result<Fd, SysError> {
    let flags = OpenFlags {
        write: true,
        create: true,
        truncate: !append,
        append,
        ..OpenFlags::default()
    };
    cx.sys.open(&cx.path(path), flags, 0o666 & !cx.umask)
}

/// Standard input, or an empty stream when the utility has none.
pub fn stdin(cx: &UtilCtx<'_>) -> Fd {
    cx.fd(0)
        .cloned()
        .unwrap_or_else(|| qsh::sys::Bytes::fd(Vec::new()))
}

/// An operand's stream: `-` is standard input.
pub fn input(cx: &UtilCtx<'_>, name: &str) -> Result<Fd, SysError> {
    if name == "-" {
        Ok(stdin(cx))
    } else {
        open_read(cx, name)
    }
}

/// The operands to read, or standard input alone when there are none.
pub fn inputs(operands: &[String]) -> Vec<String> {
    if operands.is_empty() {
        vec!["-".to_string()]
    } else {
        operands.to_vec()
    }
}

/// Read a whole operand.
pub fn read_input(cx: &UtilCtx<'_>, name: &str) -> Result<Vec<u8>, SysError> {
    let fd = input(cx, name)?;
    read_to_end(&*fd).map_err(SysError::from)
}

/// Buffered reading of lines from a stream.
pub struct Lines {
    fd: Fd,
    buf: Vec<u8>,
    pos: usize,
    eof: bool,
}

impl Lines {
    /// Lines of a stream.
    pub fn new(fd: Fd) -> Self {
        Self {
            fd,
            buf: Vec::new(),
            pos: 0,
            eof: false,
        }
    }

    fn fill(&mut self) -> io::Result<bool> {
        if self.eof {
            return Ok(false);
        }
        if self.pos > 0 {
            self.buf.drain(..self.pos);
            self.pos = 0;
        }
        let mut chunk = [0u8; 8192];
        loop {
            match self.fd.read(&mut chunk) {
                Ok(0) => {
                    self.eof = true;
                    return Ok(false);
                }
                Ok(n) => {
                    self.buf.extend_from_slice(&chunk[..n]);
                    return Ok(true);
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    /// The next line including its newline (the last may lack one), or
    /// `None` at end of file.
    pub fn next_line(&mut self) -> io::Result<Option<Vec<u8>>> {
        loop {
            if let Some(i) = self.buf[self.pos..].iter().position(|&b| b == b'\n') {
                let line = self.buf[self.pos..=self.pos + i].to_vec();
                self.pos += i + 1;
                return Ok(Some(line));
            }
            if !self.fill()? {
                if self.pos < self.buf.len() {
                    let line = self.buf[self.pos..].to_vec();
                    self.pos = self.buf.len();
                    return Ok(Some(line));
                }
                return Ok(None);
            }
        }
    }

    /// The next line without its newline, and whether it had one.
    pub fn next_text(&mut self) -> io::Result<Option<(Vec<u8>, bool)>> {
        Ok(self.next_line()?.map(|mut l| {
            let nl = l.last() == Some(&b'\n');
            if nl {
                l.pop();
            }
            (l, nl)
        }))
    }

    /// The next byte, or `None` at end of file.
    pub fn next_byte(&mut self) -> io::Result<Option<u8>> {
        if self.pos >= self.buf.len() && !self.fill()? {
            return Ok(None);
        }
        let b = self.buf[self.pos];
        self.pos += 1;
        Ok(Some(b))
    }

    /// Everything left.
    pub fn rest(&mut self) -> io::Result<Vec<u8>> {
        let mut out = self.buf[self.pos..].to_vec();
        self.pos = self.buf.len();
        if !self.eof {
            out.extend(read_to_end(&*self.fd)?);
            self.eof = true;
        }
        Ok(out)
    }
}

/// Split bytes into lines, each keeping its newline.
pub fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, &b) in data.iter().enumerate() {
        if b == b'\n' {
            out.push(&data[start..=i]);
            start = i + 1;
        }
    }
    if start < data.len() {
        out.push(&data[start..]);
    }
    out
}

/// Buffered output to a descriptor. A write that fails (a closed pipe)
/// is remembered; the utility ends with [`Out::finish`].
pub struct Out {
    fd: Option<Fd>,
    buf: Vec<u8>,
    failed: Option<io::Error>,
}

const FLUSH_AT: usize = 32 * 1024;

impl Out {
    /// Output to descriptor `n` of the utility.
    pub fn new(cx: &UtilCtx<'_>, n: u32) -> Self {
        Self::on(cx.fd(n).cloned())
    }

    /// Output to standard output.
    pub fn stdout(cx: &UtilCtx<'_>) -> Self {
        Self::new(cx, 1)
    }

    /// Output to a stream.
    pub fn on(fd: Option<Fd>) -> Self {
        Self {
            fd,
            buf: Vec::new(),
            failed: None,
        }
    }

    /// Has a write failed?
    pub fn failed(&self) -> bool {
        self.failed.is_some()
    }

    /// Queue bytes; `false` once a write has failed.
    pub fn write(&mut self, b: &[u8]) -> bool {
        if self.failed.is_some() {
            return false;
        }
        self.buf.extend_from_slice(b);
        if self.buf.len() >= FLUSH_AT {
            self.flush();
        }
        self.failed.is_none()
    }

    /// Queue text.
    pub fn text(&mut self, s: &str) -> bool {
        self.write(s.as_bytes())
    }

    /// Push everything queued to the stream.
    pub fn flush(&mut self) -> bool {
        if self.failed.is_some() {
            return false;
        }
        if self.buf.is_empty() {
            return true;
        }
        let r = match &self.fd {
            Some(fd) => write_all(&**fd, &self.buf),
            None => Err(io::Error::from(SysError::new(Errno::EBADF))),
        };
        self.buf.clear();
        match r {
            Ok(()) => true,
            Err(e) => {
                self.failed = Some(e);
                false
            }
        }
    }

    /// Flush and report: the status to return (`status` when every
    /// write succeeded, else 1 after a message as `name`).
    pub fn finish(&mut self, cx: &UtilCtx<'_>, name: &str, status: i32) -> i32 {
        self.flush();
        match &self.failed {
            None => status,
            Some(e) => {
                if e.kind() != io::ErrorKind::BrokenPipe {
                    super::complain(cx, name, &format!("standard output: {}", errno_text(e)));
                }
                1
            }
        }
    }
}

impl Drop for Out {
    fn drop(&mut self) {
        self.flush();
    }
}

/// The system's text for an I/O error.
pub fn errno_text(e: &io::Error) -> &'static str {
    super::sys_error(e).errno.text()
}

/// Copy a stream to another until end of file.
pub fn copy(from: &Fd, to: &Fd) -> io::Result<u64> {
    let mut buf = [0u8; 16384];
    let mut total = 0;
    loop {
        match from.read(&mut buf) {
            Ok(0) => return Ok(total),
            Ok(n) => {
                write_all(&**to, &buf[..n])?;
                total += n as u64;
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Read a line from standard input for a `y`/`n` prompt; `true` when
/// it starts with `y` or `Y`.
pub fn confirm(cx: &UtilCtx<'_>, prompt: &str) -> bool {
    let _ = cx.err(prompt);
    let fd = stdin(cx);
    let mut line = Vec::new();
    let mut b = [0u8; 1];
    while let Ok(1) = fd.read(&mut b) {
        if b[0] == b'\n' {
            break;
        }
        line.push(b[0]);
    }
    matches!(line.first(), Some(b'y' | b'Y'))
}
