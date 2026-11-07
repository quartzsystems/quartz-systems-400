// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Frames: a `u32` little-endian length, then that many bytes.

use std::io::{ErrorKind, Read, Write};

use crate::{Wire, WireError, decode, encode};

/// The longest frame accepted: 256 MiB.
pub const MAX_FRAME: usize = 256 << 20;

/// Write one frame.
pub fn write_frame(w: &mut impl Write, payload: &[u8]) -> Result<(), WireError> {
    write_frame_max(w, payload, MAX_FRAME)
}

/// Write one frame of at most `max` bytes.
pub fn write_frame_max(w: &mut impl Write, payload: &[u8], max: usize) -> Result<(), WireError> {
    if payload.len() > max.min(MAX_FRAME) {
        return Err(WireError::TooLong { len: payload.len() });
    }
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&u32::try_from(payload.len()).expect("bounded").to_le_bytes());
    buf.extend_from_slice(payload);
    w.write_all(&buf)?;
    Ok(())
}

/// Read one frame; [`WireError::Closed`] at a clean end of stream.
pub fn read_frame(r: &mut impl Read) -> Result<Vec<u8>, WireError> {
    read_frame_max(r, MAX_FRAME)
}

/// Read one frame of at most `max` bytes; a longer one is refused
/// before its bytes are read.
pub fn read_frame_max(r: &mut impl Read, max: usize) -> Result<Vec<u8>, WireError> {
    let mut len = [0u8; 4];
    match r.read_exact(&mut len) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::UnexpectedEof => return Err(WireError::Closed),
        Err(e) => return Err(e.into()),
    }
    let n = u32::from_le_bytes(len) as usize;
    if n > max.min(MAX_FRAME) {
        return Err(WireError::TooLong { len: n });
    }
    let mut buf = vec![0u8; n];
    r.read_exact(&mut buf).map_err(|e| {
        if e.kind() == ErrorKind::UnexpectedEof {
            WireError::Closed
        } else {
            e.into()
        }
    })?;
    Ok(buf)
}

/// Write a value as one frame.
pub fn send<T: Wire>(w: &mut impl Write, v: &T) -> Result<(), WireError> {
    write_frame(w, &encode(v))
}

/// Read one frame as a value.
pub fn recv<T: Wire>(r: &mut impl Read) -> Result<T, WireError> {
    decode(&read_frame(r)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames() {
        let mut buf = Vec::new();
        send(&mut buf, &"hello".to_string()).unwrap();
        send(&mut buf, &42u32).unwrap();
        let mut r = buf.as_slice();
        assert_eq!(recv::<String>(&mut r).unwrap(), "hello");
        assert_eq!(recv::<u32>(&mut r).unwrap(), 42);
        assert!(matches!(recv::<u32>(&mut r), Err(WireError::Closed)));
        let mut cut = &buf[..6];
        assert!(matches!(read_frame(&mut cut), Err(WireError::Closed)));
    }

    #[test]
    fn frame_limit() {
        let mut buf = Vec::new();
        write_frame(&mut buf, &[7; 9]).unwrap();
        assert!(matches!(
            read_frame_max(&mut buf.as_slice(), 8),
            Err(WireError::TooLong { len: 9 })
        ));
        assert_eq!(read_frame_max(&mut buf.as_slice(), 9).unwrap(), [7; 9]);
        assert!(matches!(
            write_frame_max(&mut Vec::new(), &[0; 9], 8),
            Err(WireError::TooLong { len: 9 })
        ));
    }
}
