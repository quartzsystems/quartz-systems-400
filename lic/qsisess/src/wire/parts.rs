// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A message in parts: the first frame says the envelope's whole length
//! (`u32`) and carries its first bytes; [`super::kind::PART`] frames
//! carry the rest, in order, with nothing between them.

use qsipart::frame::MAX_BODY;

use super::kind;

/// The envelope bytes a first frame holds.
pub const FIRST_ROOM: usize = MAX_BODY - 4;

/// The frames of one message of `kind` with envelope `env` (at most
/// `u32::MAX` bytes; the session's limit keeps it far below).
pub fn message_frames(kind_: u16, env: &[u8]) -> Vec<u8> {
    let total = u32::try_from(env.len()).unwrap_or(u32::MAX);
    let (head, rest) = env.split_at(env.len().min(FIRST_ROOM));
    let mut first = Vec::with_capacity(4 + head.len());
    first.extend_from_slice(&total.to_le_bytes());
    first.extend_from_slice(head);
    let mut out = Vec::with_capacity(env.len() + 16 + env.len() / MAX_BODY * 8);
    out.extend(super::frame(kind_, &first).unwrap_or_default());
    for chunk in rest.chunks(MAX_BODY) {
        out.extend(super::frame(kind::PART, chunk).unwrap_or_default());
    }
    out
}

/// What a frame did to the message being put together.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// More parts are to come.
    More,
    /// The whole envelope, of this kind.
    Done(u16, Vec<u8>),
    /// The peer's message is longer than the limit.
    TooLong(usize),
    /// The parts do not fit together.
    Bad(&'static str),
}

/// The message being put together from its parts.
#[derive(Debug, Default)]
pub(crate) struct Assembler {
    cur: Option<(u16, usize, Vec<u8>)>,
}

impl Assembler {
    /// Whether a message is half in.
    pub(crate) fn busy(&self) -> bool {
        self.cur.is_some()
    }

    /// Bytes held for the message half in.
    pub(crate) fn held(&self) -> usize {
        self.cur.as_ref().map_or(0, |c| c.2.len())
    }

    /// A first frame.
    pub(crate) fn first(&mut self, kind_: u16, body: &[u8], max: usize) -> Step {
        if self.busy() {
            return Step::Bad("a message began inside another");
        }
        let Some((n, head)) = body.split_first_chunk::<4>() else {
            return Step::Bad("a first part shorter than its length");
        };
        let total = u32::from_le_bytes(*n) as usize;
        if total > max {
            return Step::TooLong(total);
        }
        if head.len() > total {
            return Step::Bad("a first part longer than its message");
        }
        if head.len() == total {
            return Step::Done(kind_, head.to_vec());
        }
        if head.len() < FIRST_ROOM {
            return Step::Bad("a first part short of a full frame");
        }
        self.cur = Some((kind_, total, head.to_vec()));
        Step::More
    }

    /// A next part.
    pub(crate) fn part(&mut self, body: &[u8]) -> Step {
        let Some((_, total, buf)) = self.cur.as_mut() else {
            return Step::Bad("a part with no message begun");
        };
        if body.is_empty() || buf.len() + body.len() > *total {
            return Step::Bad("a part that does not fit its message");
        }
        buf.extend_from_slice(body);
        if buf.len() == *total {
            let (k, _, buf) = self.cur.take().unwrap_or_default();
            return Step::Done(k, buf);
        }
        if body.len() < MAX_BODY {
            return Step::Bad("a part short of a full frame");
        }
        Step::More
    }

    /// Forget what is half in.
    pub(crate) fn reset(&mut self) {
        self.cur = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qsipart::frame::Reader;

    fn assemble(bytes: &[u8], max: usize) -> Vec<Step> {
        let mut r = Reader::new();
        let mut a = Assembler::default();
        let mut steps = Vec::new();
        for chunk in bytes.chunks(1000) {
            r.push(chunk);
            while let Some(f) = r.next_frame() {
                steps.push(if f.kind == kind::PART {
                    a.part(&f.body)
                } else {
                    a.first(f.kind, &f.body, max)
                });
            }
        }
        steps
    }

    #[test]
    fn whole_in_one_and_in_many() {
        let small = message_frames(kind::REQUEST, b"abc");
        assert_eq!(
            assemble(&small, 100),
            vec![Step::Done(kind::REQUEST, b"abc".to_vec())]
        );
        let env: Vec<u8> = (0..100_000u32).map(|i| i.to_le_bytes()[0]).collect();
        let big = message_frames(kind::REPLY, &env);
        let steps = assemble(&big, 1 << 20);
        assert_eq!(steps.len(), 4);
        assert_eq!(steps.last(), Some(&Step::Done(kind::REPLY, env)));
        assert_eq!(assemble(&big, 50_000)[0], Step::TooLong(100_000));
    }

    #[test]
    fn parts_that_do_not_fit() {
        let mut a = Assembler::default();
        assert!(matches!(a.part(b"x"), Step::Bad(_)));
        assert!(matches!(a.first(1, b"\x01", 9), Step::Bad(_)));
        assert!(matches!(a.first(1, b"\x01\x00\x00\x00ab", 9), Step::Bad(_)));
        // Claims 100 000 bytes but sends a short first part.
        assert!(matches!(
            a.first(1, &[0xA0, 0x86, 1, 0, 1, 2], 1 << 20),
            Step::Bad(_)
        ));
        let mut first = 100_000u32.to_le_bytes().to_vec();
        first.extend(vec![0; FIRST_ROOM]);
        assert_eq!(a.first(1, &first, 1 << 20), Step::More);
        assert!(matches!(a.first(1, &first, 1 << 20), Step::Bad(_)));
        assert!(matches!(a.part(&[]), Step::Bad(_)));
        assert!(matches!(a.part(&[0; 10]), Step::Bad(_)));
        assert_eq!(a.held(), FIRST_ROOM + 10);
    }
}
