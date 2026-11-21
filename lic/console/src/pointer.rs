// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A pointer on the framebuffer display: the events of a mouse or a
//! tablet as `struct input_event` gives them, the position in pixels,
//! the cell under it. Pure logic; the devices are read in
//! `keyboard::evdev`.

use crate::keyboard::KeyEvent;
use crate::scale::Layout;

/// `EV_KEY`.
pub const EV_KEY: u16 = 1;
/// `EV_REL`.
pub const EV_REL: u16 = 2;
/// `EV_ABS`.
pub const EV_ABS: u16 = 3;
const REL_X: u16 = 0;
const REL_Y: u16 = 1;
const REL_WHEEL: u16 = 8;
const ABS_X: u16 = 0;
const ABS_Y: u16 = 1;
const BTN_LEFT: u16 = 0x110;

/// `struct input_event` on `x86_64`: timeval (16), type (2), code (2),
/// value (4).
pub const EVENT_LEN: usize = 24;

/// The largest value of [`PointerEvent::Absolute`]'s `at`: the far end of
/// an axis.
pub const ABS_FULL: u16 = u16::MAX;

/// An axis of a tablet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    /// Across.
    X,
    /// Down.
    Y,
}

/// What a pointing device reported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerEvent {
    /// A mouse moved, in its counts (about a pixel each).
    Move {
        /// Right is positive.
        dx: i32,
        /// Down is positive.
        dy: i32,
    },
    /// A tablet's position on one axis, from 0 to [`ABS_FULL`] across its
    /// range.
    Absolute {
        /// Which axis.
        axis: Axis,
        /// Where on it.
        at: u16,
    },
    /// The left button.
    Button {
        /// Pressed rather than released.
        pressed: bool,
    },
    /// One notch of the wheel.
    Wheel {
        /// Away from the user (Page Up) rather than toward.
        up: bool,
    },
}

/// The range of an absolute axis (`struct input_absinfo`'s minimum and
/// maximum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AxisRange {
    /// Smallest value.
    pub min: i32,
    /// Largest value.
    pub max: i32,
}

impl AxisRange {
    /// A value as a share of the range, 0 to [`ABS_FULL`], clamped.
    #[must_use]
    pub fn share(&self, value: i32) -> u16 {
        if self.max <= self.min {
            return 0;
        }
        let v = i64::from(value.clamp(self.min, self.max)) - i64::from(self.min);
        let span = i64::from(self.max) - i64::from(self.min);
        u16::try_from(v * i64::from(ABS_FULL) / span).unwrap_or(ABS_FULL)
    }
}

/// A device's absolute axes; `None` for one it does not have.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AbsRanges {
    /// `ABS_X`.
    pub x: Option<AxisRange>,
    /// `ABS_Y`.
    pub y: Option<AxisRange>,
}

/// One event of an event device, as the console takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceEvent {
    /// A key.
    Key(KeyEvent),
    /// The pointer.
    Pointer(PointerEvent),
}

/// The event of a type, code and value; `None` for one the console has
/// no use for.
#[must_use]
pub fn decode_event(kind: u16, code: u16, value: i32, ranges: &AbsRanges) -> Option<DeviceEvent> {
    let pointer = |p| Some(DeviceEvent::Pointer(p));
    match (kind, code) {
        (EV_KEY, BTN_LEFT) => match value {
            0 => pointer(PointerEvent::Button { pressed: false }),
            1 => pointer(PointerEvent::Button { pressed: true }),
            _ => None,
        },
        (EV_KEY, _) => Some(DeviceEvent::Key(KeyEvent {
            code,
            pressed: value != 0,
        })),
        (EV_REL, REL_X) if value != 0 => pointer(PointerEvent::Move { dx: value, dy: 0 }),
        (EV_REL, REL_Y) if value != 0 => pointer(PointerEvent::Move { dx: 0, dy: value }),
        (EV_REL, REL_WHEEL) if value != 0 => pointer(PointerEvent::Wheel { up: value > 0 }),
        (EV_ABS, ABS_X) => pointer(PointerEvent::Absolute {
            axis: Axis::X,
            at: ranges.x?.share(value),
        }),
        (EV_ABS, ABS_Y) => pointer(PointerEvent::Absolute {
            axis: Axis::Y,
            at: ranges.y?.share(value),
        }),
        _ => None,
    }
}

/// The event of a raw `struct input_event`; `None` for a short one too.
#[must_use]
pub fn decode_raw(raw: &[u8], ranges: &AbsRanges) -> Option<DeviceEvent> {
    let raw = raw.get(..EVENT_LEN)?;
    let kind = u16::from_ne_bytes([raw[16], raw[17]]);
    let code = u16::from_ne_bytes([raw[18], raw[19]]);
    let value = i32::from_ne_bytes([raw[20], raw[21], raw[22], raw[23]]);
    decode_event(kind, code, value, ranges)
}

/// The pointer's place in pixels on a display of a size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

/// `at` of `0..=full` on `0..n`.
fn along(at: usize, full: usize, n: usize) -> usize {
    (at.min(full) * n.saturating_sub(1))
        .checked_div(full)
        .unwrap_or(0)
}

impl Position {
    /// The middle of a display.
    #[must_use]
    pub fn new(width: usize, height: usize) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        Self {
            x: width / 2,
            y: height / 2,
            width,
            height,
        }
    }

    /// Where it is.
    #[must_use]
    pub fn xy(&self) -> (usize, usize) {
        (self.x, self.y)
    }

    /// The display's size as it was last fitted.
    #[must_use]
    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    /// The same place on a display of another size (a mode change),
    /// inside it.
    #[must_use]
    pub fn fitted(self, width: usize, height: usize) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        if (width, height) == (self.width, self.height) {
            return self;
        }
        Self {
            x: along(self.x, self.width - 1, width),
            y: along(self.y, self.height - 1, height),
            width,
            height,
        }
    }

    /// Moved by a mouse's counts, kept on the display.
    pub fn moved(&mut self, dx: i32, dy: i32) {
        let step = |v: usize, d: i32, n: usize| {
            let v = i64::try_from(v)
                .unwrap_or(i64::MAX)
                .saturating_add(i64::from(d));
            usize::try_from(v.max(0)).unwrap_or(0).min(n - 1)
        };
        self.x = step(self.x, dx, self.width);
        self.y = step(self.y, dy, self.height);
    }

    /// Placed by a tablet on one axis.
    pub fn place(&mut self, axis: Axis, at: u16) {
        let full = usize::from(ABS_FULL);
        match axis {
            Axis::X => self.x = along(usize::from(at), full, self.width),
            Axis::Y => self.y = along(usize::from(at), full, self.height),
        }
    }

    /// The 0-based cell under the pointer; `None` outside the grid.
    #[must_use]
    pub fn cell(&self, layout: &Layout) -> Option<(u16, u16)> {
        let (row, col) = layout.cell_at(self.x, self.y)?;
        Some((u16::try_from(row).ok()?, u16::try_from(col).ok()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scale::Scale;

    const TABLET: AbsRanges = AbsRanges {
        x: Some(AxisRange { min: 0, max: 32767 }),
        y: Some(AxisRange { min: 0, max: 32767 }),
    };

    fn raw(kind: u16, code: u16, value: i32) -> [u8; EVENT_LEN] {
        let mut b = [0u8; EVENT_LEN];
        b[16..18].copy_from_slice(&kind.to_ne_bytes());
        b[18..20].copy_from_slice(&code.to_ne_bytes());
        b[20..24].copy_from_slice(&value.to_ne_bytes());
        b
    }

    #[test]
    fn events_of_a_mouse_a_tablet_and_a_keyboard() {
        let none = AbsRanges::default();
        let p = |e| Some(DeviceEvent::Pointer(e));
        assert_eq!(
            decode_raw(&raw(EV_KEY, 30, 1), &none),
            Some(DeviceEvent::Key(KeyEvent {
                code: 30,
                pressed: true
            }))
        );
        assert_eq!(
            decode_raw(&raw(EV_KEY, BTN_LEFT, 1), &none),
            p(PointerEvent::Button { pressed: true })
        );
        assert_eq!(
            decode_raw(&raw(EV_KEY, BTN_LEFT, 0), &none),
            p(PointerEvent::Button { pressed: false })
        );
        assert_eq!(decode_raw(&raw(EV_KEY, BTN_LEFT, 2), &none), None);
        assert_eq!(
            decode_raw(&raw(EV_REL, REL_X, -3), &none),
            p(PointerEvent::Move { dx: -3, dy: 0 })
        );
        assert_eq!(
            decode_raw(&raw(EV_REL, REL_Y, 4), &none),
            p(PointerEvent::Move { dx: 0, dy: 4 })
        );
        assert_eq!(
            decode_raw(&raw(EV_REL, REL_WHEEL, -1), &none),
            p(PointerEvent::Wheel { up: false })
        );
        assert_eq!(decode_raw(&raw(EV_REL, REL_WHEEL, 0), &none), None);
        // A tablet's axes need their ranges.
        assert_eq!(decode_raw(&raw(EV_ABS, ABS_X, 100), &none), None);
        assert_eq!(
            decode_raw(&raw(EV_ABS, ABS_X, 32767), &TABLET),
            p(PointerEvent::Absolute {
                axis: Axis::X,
                at: ABS_FULL
            })
        );
        assert_eq!(
            decode_raw(&raw(EV_ABS, ABS_Y, -5), &TABLET),
            p(PointerEvent::Absolute {
                axis: Axis::Y,
                at: 0
            })
        );
        // Synchronization, other axes, a short buffer: nothing.
        assert_eq!(decode_raw(&raw(0, 0, 0), &TABLET), None);
        assert_eq!(decode_raw(&raw(EV_ABS, 0x2f, 1), &TABLET), None);
        assert_eq!(decode_raw(&[0; 10], &TABLET), None);
    }

    #[test]
    fn a_range_is_shared_out_without_overflow() {
        let r = AxisRange {
            min: i32::MIN,
            max: i32::MAX,
        };
        assert_eq!(r.share(i32::MIN), 0);
        assert_eq!(r.share(i32::MAX), ABS_FULL);
        let r = AxisRange { min: 10, max: 10 };
        assert_eq!(r.share(10), 0);
        let r = AxisRange { min: 0, max: 100 };
        assert_eq!(r.share(50), ABS_FULL / 2);
        assert_eq!(r.share(1000), ABS_FULL);
    }

    #[test]
    fn the_position_stays_on_the_display() {
        let mut p = Position::new(1024, 768);
        assert_eq!(p.xy(), (512, 384));
        p.moved(i32::MAX, i32::MIN);
        assert_eq!(p.xy(), (1023, 0));
        p.moved(-5000, 5000);
        assert_eq!(p.xy(), (0, 767));
        p.place(Axis::X, ABS_FULL);
        p.place(Axis::Y, 0);
        assert_eq!(p.xy(), (1023, 0));
        p.place(Axis::X, ABS_FULL / 2);
        assert_eq!(p.xy(), (511, 0));
        // A degenerate display is one pixel.
        let mut p = Position::new(0, 0);
        p.moved(7, 7);
        assert_eq!(p.xy(), (0, 0));
    }

    #[test]
    fn a_mode_change_keeps_the_place_and_clamps_it() {
        let mut p = Position::new(1920, 1080);
        p.moved(10_000, 10_000);
        assert_eq!(p.xy(), (1919, 1079));
        let small = p.fitted(1024, 768);
        assert_eq!(
            small.xy(),
            (1023, 767),
            "the far corner stays the far corner"
        );
        assert_eq!(small.size(), (1024, 768));
        let mut q = Position::new(1920, 1080);
        q.moved(-10_000, -10_000);
        assert_eq!(q.fitted(640, 480).xy(), (0, 0));
        // The middle stays the middle, about.
        let m = Position::new(1920, 1080).fitted(1024, 768);
        assert_eq!(m.xy(), (511, 383));
        // The same size: nothing moves.
        assert_eq!(p.fitted(1920, 1080), p);
        // Back up: still inside.
        let big = small.fitted(3840, 2160);
        assert_eq!(big.xy(), (3839, 2159));
    }

    #[test]
    fn the_cell_under_a_pixel_at_the_grid_edges() {
        for (w, h, scale) in [(1024, 768, 1), (1920, 1080, 2)] {
            let l = Layout::compute(w, h, 24, 80, Scale::Auto);
            assert_eq!(l.scale, scale);
            let at = |x, y| {
                let mut p = Position::new(w, h);
                p.moved(-10_000, -10_000);
                p.moved(x, y);
                p.cell(&l)
            };
            let (x0, y0) = (i32::try_from(l.x0).unwrap(), i32::try_from(l.y0).unwrap());
            let (gw, gh) = (
                i32::try_from(l.width()).unwrap(),
                i32::try_from(l.height()).unwrap(),
            );
            assert_eq!(at(x0, y0), Some((0, 0)));
            assert_eq!(at(x0 - 1, y0), None);
            assert_eq!(at(x0, y0 - 1), None);
            assert_eq!(at(x0 + gw - 1, y0 + gh - 1), Some((23, 79)));
            assert_eq!(at(x0 + gw, y0), None);
            assert_eq!(at(x0, y0 + gh), None);
            let cw = i32::try_from(9 * scale).unwrap();
            let ch = i32::try_from(16 * scale).unwrap();
            assert_eq!(at(x0 + cw, y0 + ch - 1), Some((0, 1)));
            assert_eq!(at(x0 + cw - 1, y0 + ch), Some((1, 0)));
        }
    }
}
