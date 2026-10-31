// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! LIC PTFs and the A/B slots in the service (`docs/PTF.md` §5): the
//! slot state in the LIC record, the ESP the images go to, the firmware
//! the boot choice goes to. The logic is `ptf::slot`.

use std::fs::OpenOptions;
use std::io;

use ptf::slot::{self, Esp, Firmware, IplEvent, Slot, SlotState, TAG_LIC_SLOTS};
use ptf::{Action, PtfDb};

use super::ptfcmd;
use crate::ipl::system::{System, now_unix};
use crate::platform::efivar::{self, EfiFirmware};

/// The ESP partition, written through the FAT code the installer uses.
struct EspDev;

impl Esp for EspDev {
    fn write(&mut self, s: Slot, image: &[u8]) -> io::Result<()> {
        let lic = licpart::locate().ok();
        let (node, _) = diskimg::locate::esp(lic.as_deref()).map_err(|e| io::Error::other(e.to_string()))?;
        let mut f = OpenOptions::new().read(true).write(true).open(&node)?;
        diskimg::fat::write_file(&mut f, efivar::entry(s).1, image).map_err(|e| io::Error::other(e.to_string()))?;
        f.sync_all()
    }
}

/// The slot state from the LIC record (reread, so `slic`'s writes count).
pub fn load(system: &mut System) -> SlotState {
    match system.lic.as_mut() {
        Some((part, record)) => {
            part.refresh(record);
            record.tlv(TAG_LIC_SLOTS).map(SlotState::decode).unwrap_or_default()
        }
        None => SlotState::default(),
    }
}

/// Keep the slot state in the LIC record.
pub fn store(system: &mut System, st: &SlotState) -> Result<(), String> {
    let (part, record) = system.lic.as_mut().ok_or("LIC partition not attached")?;
    part.refresh(record);
    record.set_tlv(TAG_LIC_SLOTS, st.encode()).map_err(|e| e.to_string())?;
    part.write(record).map(|_| ()).map_err(|e| e.to_string())
}

/// The images of a LIC PTF: slot A's build and slot B's when it has one.
fn images(system: &mut System, db: &PtfDb, ptf: &str) -> Result<(Vec<u8>, Option<Vec<u8>>), String> {
    let (pkg, _) = ptfcmd::package_of(system, db, ptf)?;
    let a = pkg.lic.ok_or_else(|| format!("CPF3598 {ptf} carries no LIC image"))?;
    Ok((a, pkg.lic_b))
}

/// `APYPTF` of a LIC PTF (now Apply at next IPL): its image into the
/// slot not booted. The slot written.
pub fn stage(system: &mut System, db: &mut PtfDb, ptf: &str) -> Result<Slot, String> {
    let (a, b) = images(system, db, ptf)?;
    let mut st = load(system);
    let booted = system.boot.slot;
    let r = slot::stage(&mut st, &mut EspDev, booted, ptf, &a, b.as_deref());
    store(system, &st)?;
    r.map_err(|e| {
        // Not staged: it stays loaded.
        if let Some(rec) = db.ptfs.get_mut(ptf) {
            rec.action = Action::None;
        }
        format!("{} {e}", e.msgid())
    })
}

/// `RMVPTF` of a temporarily applied LIC PTF.
pub fn remove(system: &mut System, db: &mut PtfDb, ptf: &str) -> Result<Slot, String> {
    let mut st = load(system);
    let s = slot::remove(&mut st, db, ptf, now_unix()).map_err(|e| format!("{} {e}", e.msgid()))?;
    store(system, &st)?;
    Ok(s)
}

/// `APYPTF APY(*PERM)` of a LIC PTF: both slots hold its image.
pub fn make_permanent(system: &mut System, db: &mut PtfDb, ptf: &str) -> Result<(), String> {
    let (a, b) = images(system, db, ptf)?;
    let mut st = load(system);
    let r = slot::make_permanent(&mut st, &mut EspDev, db, ptf, &a, b.as_deref(), now_unix());
    store(system, &st)?;
    r.map_err(|e| format!("{} {e}", e.msgid()))
}

/// Whether `ptf` is a LIC PTF that `ptf::slot` keeps.
pub fn is_lic(db: &PtfDb, ptf: &str) -> bool {
    db.get(ptf).is_some_and(|r| r.lic)
}

/// At IPL: the LIC PTF waiting for this slot takes effect (C6004503),
/// a removal settles, an image torn by a power loss is written again.
/// What was done, for the IPL's message.
pub fn at_ipl(system: &mut System, db: &mut PtfDb) -> Vec<String> {
    let mut st = load(system);
    if st == SlotState::default() && !db.ptfs.values().any(|r| r.lic && r.action != Action::None) {
        return Vec::new();
    }
    let mut out = Vec::new();
    let booted = system.boot.slot;
    let (events, fw_err) = match EfiFirmware::open() {
        Ok(mut fw) => slot::at_ipl(&mut st, db, booted, &mut fw, now_unix()),
        Err(e) => {
            let mut none = NoFirmware(e);
            slot::at_ipl(&mut st, db, booted, &mut none, now_unix())
        }
    };
    if let Some(e) = fw_err {
        out.push(format!("boot order not changed: {e}"));
    }
    let _ = store(system, &st);
    for ev in events {
        match ev {
            IplEvent::Applied(p) => {
                system.emit(&crate::src::PTF_LIC);
                out.push(format!("LIC PTF {p} in effect from slot {booted:?}"));
            }
            IplEvent::Removed(p) => out.push(format!("LIC PTF {p} removed; slot {booted:?} is the IPL source")),
            IplEvent::Waiting(p) => out.push(format!("LIC PTF {p} still waits for its slot")),
            IplEvent::Restage(p) => match stage(system, db, &p) {
                Ok(s) => out.push(format!("LIC PTF {p} written to slot {s:?} again")),
                Err(e) => out.push(format!("LIC PTF {p}: {e}")),
            },
        }
    }
    out
}

/// A firmware that cannot be asked: the change is reported.
struct NoFirmware(String);

impl Firmware for NoFirmware {
    fn boot_next(&mut self, _: Slot) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn boot_default(&mut self, _: Slot) -> Result<(), String> {
        Err(self.0.clone())
    }
}

/// At power down (`slic`): arm the one-shot boot of the slot `IPLSRC`
/// names, else of a pending LIC PTF's slot. What was armed.
pub fn arm(record: &licpart::Record, iplsrc: Option<Slot>) -> Result<Option<Slot>, String> {
    let st = record.tlv(TAG_LIC_SLOTS).map(SlotState::decode).unwrap_or_default();
    let Some(s) = slot::next_slot(&st, iplsrc) else {
        return Ok(None);
    };
    EfiFirmware::open()?.boot_next(s)?;
    Ok(Some(s))
}
