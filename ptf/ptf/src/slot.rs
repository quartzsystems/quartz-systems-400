// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LIC PTFs and the A/B slots (`docs/PTF.md` §5). A LIC PTF's image goes
//! into the slot the system did not boot; the next IPL boots that slot
//! once (the firmware's one-shot boot), and only the IPL that comes up
//! there makes the PTF Temporarily applied and the slot the default. A
//! power loss before then boots the old slot again, which still holds
//! the old image, and the PTF stays Apply at next IPL. The state is IPL
//! state, kept in the LIC partition record ([`TAG_LIC_SLOTS`]).

use std::fmt::Write as _;
use std::io;

pub use licpart::Slot;

use crate::db::PtfDb;
use crate::status::{Action, Status};
use crate::{Error, Result};

/// The LIC record's TLV tag for [`SlotState`].
pub const TAG_LIC_SLOTS: u16 = 0x0107;

/// The ESP's two slot images.
pub trait Esp {
    /// Replace a slot's image; returns once it is durable.
    fn write(&mut self, slot: Slot, image: &[u8]) -> io::Result<()>;
}

/// The firmware's boot choice.
pub trait Firmware {
    /// Boot `slot` at the next IPL only.
    fn boot_next(&mut self, slot: Slot) -> std::result::Result<(), String>;
    /// Boot `slot` from now on.
    fn boot_default(&mut self, slot: Slot) -> std::result::Result<(), String>;
}

/// Where the LIC PTFs are.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotState {
    /// The slot IPLs boot by default; `None` is the firmware's own
    /// choice (slot A, the removable-media path).
    pub default: Option<Slot>,
    /// A LIC PTF whose image waits in a slot for the IPL that boots it.
    pub pending: Option<(String, Slot)>,
    /// A LIC PTF being removed, and the slot with the image before it.
    pub removing: Option<(String, Slot)>,
    /// Temporarily applied LIC PTFs and the slot holding each.
    pub temp: Vec<(String, Slot)>,
}

fn letter(s: Slot) -> char {
    match s {
        Slot::A => 'A',
        Slot::B => 'B',
    }
}

fn slot_of(t: &str) -> Option<Slot> {
    match t {
        "A" => Some(Slot::A),
        "B" => Some(Slot::B),
        _ => None,
    }
}

impl SlotState {
    /// The TLV's bytes: a line per fact.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = String::from("QS400SLOTS 1\n");
        if let Some(d) = self.default {
            let _ = writeln!(out, "default {}", letter(d));
        }
        if let Some((p, s)) = &self.pending {
            let _ = writeln!(out, "pending {p} {}", letter(*s));
        }
        if let Some((p, s)) = &self.removing {
            let _ = writeln!(out, "removing {p} {}", letter(*s));
        }
        for (p, s) in &self.temp {
            let _ = writeln!(out, "temp {p} {}", letter(*s));
        }
        out.into_bytes()
    }

    /// From the TLV's bytes; empty or damaged is the initial state.
    #[must_use]
    pub fn decode(b: &[u8]) -> Self {
        let text = String::from_utf8_lossy(b);
        let mut st = Self::default();
        let mut lines = text.lines();
        if lines.next() != Some("QS400SLOTS 1") {
            return st;
        }
        for line in lines {
            let f: Vec<&str> = line.split(' ').collect();
            match f.as_slice() {
                ["default", s] => st.default = slot_of(s),
                ["pending", p, s] => st.pending = slot_of(s).map(|s| ((*p).to_string(), s)),
                ["removing", p, s] => st.removing = slot_of(s).map(|s| ((*p).to_string(), s)),
                ["temp", p, s] => {
                    if let Some(s) = slot_of(s) {
                        st.temp.push(((*p).to_string(), s));
                    }
                }
                _ => {}
            }
        }
        st
    }

    /// The slot holding a temporarily applied LIC PTF.
    #[must_use]
    pub fn slot_of(&self, ptf: &str) -> Option<Slot> {
        self.temp.iter().find(|(p, _)| p == ptf).map(|(_, s)| *s)
    }
}

/// The image built for `slot`: slot A's, or slot B's when the PTF has one.
#[must_use]
pub fn image_for<'a>(slot: Slot, a: &'a [u8], b: Option<&'a [u8]>) -> &'a [u8] {
    match (slot, b) {
        (Slot::B, Some(b)) => b,
        _ => a,
    }
}

/// `APYPTF` of a LIC PTF: its image into the slot not booted. The PTF
/// is Apply at next IPL already (`apply::apply_temp`); `pending` is set
/// only once the image is durable.
pub fn stage(
    st: &mut SlotState,
    esp: &mut dyn Esp,
    booted: Slot,
    ptf: &str,
    a: &[u8],
    b: Option<&[u8]>,
) -> Result<Slot> {
    if let Some((p, _)) = &st.pending
        && p != ptf
    {
        return Err(Error::refused("CPF3601", format!("LIC PTF {p} waits for the IPL")));
    }
    if st.removing.is_some() {
        return Err(Error::refused("CPF3601", "a LIC PTF waits to be removed at the IPL"));
    }
    let target = booted.other();
    if st.temp.iter().any(|(_, s)| s.other() == target) {
        return Err(Error::refused(
            "CPF3601",
            "the other slot holds the image before a temporarily applied LIC PTF; apply it permanently first",
        ));
    }
    st.pending = None;
    esp.write(target, image_for(target, a, b)).map_err(|e| Error::NotPtf(format!("slot {}: {e}", letter(target))))?;
    st.pending = Some((ptf.to_string(), target));
    Ok(target)
}

/// The slot the next IPL boots once: `IPLSRC(A|B)` when given, else a
/// pending apply's, else a pending removal's; `None` for the default.
#[must_use]
pub fn next_slot(st: &SlotState, iplsrc: Option<Slot>) -> Option<Slot> {
    iplsrc
        .or_else(|| st.pending.as_ref().map(|(_, s)| *s))
        .or_else(|| st.removing.as_ref().map(|(_, s)| *s))
}

/// What the IPL did with the LIC PTFs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IplEvent {
    /// The PTF took effect from the other slot (`C6004503`).
    Applied(String),
    /// The PTF is removed: the IPL came up on the old image.
    Removed(String),
    /// The IPL did not come up on the PTF's slot; it waits for the next.
    Waiting(String),
    /// Apply at next IPL without a staged image (a power loss while it
    /// was written): stage it again.
    Restage(String),
}

/// At IPL, booted from `booted`: settle the pending apply or removal.
/// A failed default change is returned with the events.
pub fn at_ipl(
    st: &mut SlotState,
    db: &mut PtfDb,
    booted: Slot,
    fw: &mut dyn Firmware,
    now: u64,
) -> (Vec<IplEvent>, Option<String>) {
    let mut ev = Vec::new();
    let mut fw_err = None;
    if let Some((p, s)) = st.pending.clone() {
        if booted == s {
            if let Some(r) = db.ptfs.get_mut(&p) {
                r.status = Status::TempApplied;
                r.action = Action::None;
                r.changed = now;
            }
            st.temp.retain(|(q, _)| q != &p);
            st.temp.push((p.clone(), s));
            st.pending = None;
            st.default = Some(s);
            fw_err = fw.boot_default(s).err();
            ev.push(IplEvent::Applied(p));
        } else {
            ev.push(IplEvent::Waiting(p));
        }
    }
    if let Some((p, s)) = st.removing.clone() {
        if booted == s {
            if let Some(r) = db.ptfs.get_mut(&p) {
                r.status = Status::TempRemoved;
                r.action = Action::None;
                r.changed = now;
            }
            st.temp.retain(|(q, _)| q != &p);
            st.removing = None;
            st.default = Some(s);
            fw_err = fw.boot_default(s).err().or(fw_err);
            ev.push(IplEvent::Removed(p));
        } else {
            ev.push(IplEvent::Waiting(p));
        }
    }
    for r in db.ptfs.values() {
        let staged = st.pending.as_ref().is_some_and(|(p, _)| p == &r.ptf);
        if r.lic && r.action == Action::ApplyAtIpl && !staged {
            ev.push(IplEvent::Restage(r.ptf.clone()));
        }
    }
    (ev, fw_err)
}

/// `RMVPTF` of a temporarily applied LIC PTF: Remove at next IPL, and
/// the IPL boots the slot with the image before it.
pub fn remove(st: &mut SlotState, db: &mut PtfDb, ptf: &str, now: u64) -> Result<Slot> {
    let s = st
        .slot_of(ptf)
        .ok_or_else(|| Error::refused("CPF3606", format!("{ptf} is not a temporarily applied LIC PTF")))?;
    if st.pending.is_some() {
        return Err(Error::refused("CPF3601", "a LIC PTF waits for the IPL"));
    }
    let r = db
        .ptfs
        .get_mut(ptf)
        .ok_or_else(|| Error::refused("CPF3606", ptf.to_string()))?;
    r.action = Action::RemoveAtIpl;
    r.changed = now;
    st.removing = Some((ptf.to_string(), s.other()));
    Ok(s.other())
}

/// `APYPTF APY(*PERM)` of a LIC PTF: its image into the other slot too,
/// so both slots hold it and the old image is gone.
pub fn make_permanent(
    st: &mut SlotState,
    esp: &mut dyn Esp,
    db: &mut PtfDb,
    ptf: &str,
    a: &[u8],
    b: Option<&[u8]>,
    now: u64,
) -> Result<()> {
    let s = st
        .slot_of(ptf)
        .ok_or_else(|| Error::refused("CPF3606", format!("{ptf} is not a temporarily applied LIC PTF")))?;
    if st.removing.as_ref().is_some_and(|(p, _)| p == ptf) {
        return Err(Error::refused("CPF3601", format!("{ptf} waits to be removed")));
    }
    let other = s.other();
    esp.write(other, image_for(other, a, b)).map_err(|e| Error::NotPtf(format!("slot {}: {e}", letter(other))))?;
    st.temp.retain(|(p, _)| p != ptf);
    if let Some(r) = db.ptfs.get_mut(ptf) {
        r.status = Status::PermApplied;
        r.action = Action::None;
        r.changed = now;
    }
    Ok(())
}
