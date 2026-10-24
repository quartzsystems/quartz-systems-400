// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Text-mode opens (`O_TEXTDATA`): data converts between the file's
//! CCSID and the open's on every read and write; line ends are left
//! alone. Offsets stay the file's: a read moves the position by the
//! file bytes it consumed, and converted bytes that did not fit the
//! caller's buffer wait for the next read. Positioning drops them.

use ccsid::conv::{Converter, Unsupported};

/// One open's conversion state.
#[derive(Debug, Clone)]
pub struct TextState {
    /// The file's CCSID.
    pub file_ccsid: u16,
    /// The open's CCSID.
    pub open_ccsid: u16,
    read: Converter,
    write: Converter,
    /// Converted bytes not yet returned.
    pending: Vec<u8>,
}

impl TextState {
    /// A state converting between `file_ccsid` and `open_ccsid`; `None`
    /// when no conversion is needed.
    pub fn new(file_ccsid: u16, open_ccsid: u16) -> Result<Option<Self>, Unsupported> {
        let read = Converter::new(file_ccsid, open_ccsid)?;
        if read.is_identity() {
            return Ok(None);
        }
        Ok(Some(Self {
            file_ccsid,
            open_ccsid,
            read,
            write: Converter::new(open_ccsid, file_ccsid)?,
            pending: Vec::new(),
        }))
    }

    /// Serve waiting converted bytes into `buf`; returns how many.
    pub fn serve(&mut self, buf: &mut [u8]) -> usize {
        let n = buf.len().min(self.pending.len());
        buf[..n].copy_from_slice(&self.pending[..n]);
        self.pending.drain(..n);
        n
    }

    /// Whether converted bytes are waiting.
    #[must_use]
    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }

    /// Convert file bytes just read and serve what fits; `eof` flushes a
    /// partial character.
    pub fn convert_read(&mut self, raw: &[u8], eof: bool, buf: &mut [u8]) -> usize {
        let mut out = Vec::with_capacity(raw.len() * 2);
        self.read.feed(raw, &mut out);
        if eof {
            self.read.finish(&mut out);
        }
        self.pending.extend_from_slice(&out);
        self.serve(buf)
    }

    /// Convert bytes to write into the file's CCSID.
    pub fn convert_write(&mut self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() * 2);
        self.write.feed(data, &mut out);
        out
    }

    /// The file bytes of a partial character left at close.
    pub fn finish_write(&mut self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write.finish(&mut out);
        out
    }

    /// Forget waiting bytes and partial characters (after a seek).
    pub fn reset(&mut self) {
        self.pending.clear();
        if let (Ok(r), Ok(w)) = (
            Converter::new(self.file_ccsid, self.open_ccsid),
            Converter::new(self.open_ccsid, self.file_ccsid),
        ) {
            self.read = r;
            self.write = w;
        }
    }

    /// A one-shot conversion of file bytes (for `pread`).
    #[must_use]
    pub fn convert_once(&self, raw: &[u8]) -> Vec<u8> {
        ccsid::conv::convert(self.file_ccsid, self.open_ccsid, raw).unwrap_or_else(|_| raw.to_vec())
    }

    /// A one-shot conversion to file bytes (for `pwrite`).
    #[must_use]
    pub fn convert_once_out(&self, data: &[u8]) -> Vec<u8> {
        ccsid::conv::convert(self.open_ccsid, self.file_ccsid, data)
            .unwrap_or_else(|_| data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        assert!(TextState::new(1208, 1208).unwrap().is_none());
        assert!(TextState::new(65535, 37).unwrap().is_none());
        let mut t = TextState::new(37, 819).unwrap().unwrap();
        let raw = t.convert_write(b"Hello\n");
        assert_eq!(raw, ccsid::to_ebcdic("Hello\n").unwrap());
        let mut buf = [0u8; 3];
        assert_eq!(t.convert_read(&raw, true, &mut buf), 3);
        assert_eq!(&buf, b"Hel");
        assert!(t.has_pending());
        let mut rest = [0u8; 10];
        assert_eq!(t.serve(&mut rest), 3);
        assert_eq!(&rest[..3], b"lo\n");
        t.reset();
        assert!(!t.has_pending());
        // UTF-8 file read as UTF-16: output grows.
        let t16 = TextState::new(1208, 1200).unwrap().unwrap();
        assert_eq!(t16.convert_once("é".as_bytes()), vec![0x00, 0xE9]);
        assert_eq!(t16.convert_once_out(&[0x00, 0xE9]), "é".as_bytes());
    }
}
