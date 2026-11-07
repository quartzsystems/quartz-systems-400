// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The streaming converter: a partial character and both shift states
//! carry between calls.

use super::mixed::{SI, SO, Unit, unit};
use super::{Codec, Unsupported};

/// The shift state of mixed data: after SI (or at the start) single
/// bytes, after SO pairs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shift {
    /// Single-byte characters.
    Single,
    /// Double-byte characters.
    Double,
}

/// A streaming conversion from one CCSID to another that carries a
/// partial character between calls.
#[derive(Debug, Clone)]
pub struct Converter {
    from: Option<Codec>,
    to: Option<Codec>,
    pending: Vec<u8>,
    /// The input's shift state after the bytes consumed.
    input: Shift,
    /// The output's shift state after the bytes written.
    output: Shift,
    /// Characters that had to be substituted so far.
    pub substituted: usize,
}

impl Converter {
    /// A converter from `from` to `to`.
    pub fn new(from: u16, to: u16) -> Result<Self, Unsupported> {
        let from = Codec::of(from)?;
        let to = Codec::of(to)?;
        Ok(Self {
            input: from.map_or(Shift::Single, |c| c.initial_shift()),
            output: to.map_or(Shift::Single, |c| c.initial_shift()),
            from,
            to,
            pending: Vec::new(),
            substituted: 0,
        })
    }

    /// Start decoding in shift state `input`, for data read from the
    /// middle of a mixed stream. Only a mixed source has a choice. The
    /// output still starts in single-byte state, so a pass-through
    /// writes an SO before the first pair.
    #[must_use]
    pub fn with_shift(mut self, input: Shift) -> Self {
        if self.from.is_some_and(|c| c.is_mixed()) {
            self.input = input;
        }
        self
    }

    /// The input's shift state after the bytes consumed so far (pending
    /// bytes excluded): always `Double` for a double-byte source and
    /// `Single` for one that is not mixed.
    #[must_use]
    pub fn input_shift(&self) -> Shift {
        self.input
    }

    /// Whether bytes pass through unchanged (same CCSID or same tables,
    /// or either is binary).
    #[must_use]
    pub fn is_identity(&self) -> bool {
        match (self.from, self.to) {
            (Some(a), Some(b)) => a.ccsid == b.ccsid || a.form == b.form,
            _ => true,
        }
    }

    /// Convert `input` and append to `out`. A trailing partial character
    /// waits for the next call.
    pub fn feed(&mut self, input: &[u8], out: &mut Vec<u8>) {
        if self.is_identity() {
            self.pass(input, out);
            return;
        }
        let (Some(from), Some(to)) = (self.from, self.to) else {
            return;
        };
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(input);
        let mut chars = Vec::with_capacity(buf.len());
        let used = from.decode_in(&buf, &mut self.input, &mut chars);
        for ch in chars {
            if !to.encode_in(ch, &mut self.output, out) {
                self.substituted += 1;
            }
        }
        buf.drain(..used);
        self.pending = buf;
    }

    /// Copy bytes through. Mixed data keeps its shift states and holds
    /// half a pair back, so [`Self::flush_shift`] never splits one.
    fn pass(&mut self, input: &[u8], out: &mut Vec<u8>) {
        let mixed = self.from.is_some_and(|c| c.is_mixed());
        if !mixed {
            out.extend_from_slice(input);
            return;
        }
        let mut buf = std::mem::take(&mut self.pending);
        buf.extend_from_slice(input);
        let start = self.input;
        let mut at = 0;
        while let Some((u, n)) = unit(&buf[at..], self.input) {
            if let Unit::Shift(s) = u {
                self.input = s;
            }
            at += n;
        }
        // After a flush the output owes an SO before more pairs.
        if at > 0
            && start == Shift::Double
            && self.output == Shift::Single
            && !matches!(buf[0], SO | SI)
        {
            out.push(SO);
        }
        self.output = self.input;
        out.extend_from_slice(&buf[..at]);
        buf.drain(..at);
        self.pending = buf;
    }

    /// Put the output back in single-byte state: an SI when it is in
    /// double-byte state. Pending input stays.
    pub fn flush_shift(&mut self, out: &mut Vec<u8>) {
        if self.output == Shift::Double && self.to.is_some_and(|c| c.is_mixed()) {
            out.push(SI);
            self.output = Shift::Single;
        }
    }

    /// Flush a trailing partial character as one substitution (passed
    /// through unchanged when converting nothing) and put the output
    /// back in single-byte state.
    pub fn finish(&mut self, out: &mut Vec<u8>) {
        if !self.pending.is_empty() {
            if self.is_identity() {
                out.append(&mut self.pending);
            } else {
                self.pending.clear();
                if let Some(to) = self.to {
                    to.encode_in('\u{FFFD}', &mut self.output, out);
                    self.substituted += 1;
                }
            }
        }
        self.flush_shift(out);
    }

    /// Bytes held back waiting for the rest of a character.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}
