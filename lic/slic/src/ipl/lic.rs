// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Attaching the LIC partition, abnormal-IPL detection, history records.
//! The record holds only what the IPL needs before the object store opens.

use std::fs::OpenOptions;
use std::io::Cursor;

use licpart::{Device, IplHistory, IplMode, LicError, LicPart, Outcome, SLOT_BYTES};

use crate::ipl::system::{System, now_unix};
use crate::src;

/// C1001010 and C1001020: locate, open, read or initialize, detect an
/// abnormal previous IPL, mark this one in progress.
pub fn attach(system: &mut System) -> Result<(), &'static src::Src> {
    // On a D IPL (installation media) a missing partition is normal: run
    // with an in-memory record. Everything else needs the partition.
    let device: Box<dyn Device> = match licpart::locate() {
        Ok(path) => {
            system
                .out
                .serial(&format!("slic: LIC partition at {}\n", path.display()));
            system.emit(&src::LIC_LOCATED);
            match OpenOptions::new().read(true).write(true).open(&path) {
                Ok(f) => Box::new(f),
                Err(e) => {
                    system
                        .out
                        .serial(&format!("slic: open {}: {e}\n", path.display()));
                    return Err(&src::LIC_NOT_FOUND);
                }
            }
        }
        Err(e) if system.boot.ipl_d => {
            system
                .out
                .serial(&format!("slic: {e}; IPL type D continues\n"));
            system.emit(&src::MEDIA_IPL);
            Box::new(Cursor::new(vec![0u8; 2 * SLOT_BYTES]))
        }
        Err(e) => {
            system.out.serial(&format!("slic: {e}\n"));
            return Err(&src::LIC_NOT_FOUND);
        }
    };
    let mut part = LicPart::open(device);
    let mut record = match part.read() {
        Ok(r) => r,
        Err(LicError::NoValidCopy) => {
            // First boot: both copies blank. Never happens with one valid copy.
            system
                .out
                .serial("slic: LIC partition blank, initializing\n");
            match LicPart::init(part.into_device(), system.start_time) {
                Ok((p, r)) => {
                    part = p;
                    r
                }
                Err(e) => {
                    system.out.serial(&format!("slic: LIC init: {e}\n"));
                    return Err(&src::LIC_UNREADABLE);
                }
            }
        }
        Err(e) => {
            system.out.serial(&format!("slic: LIC read: {e}\n"));
            return Err(&src::LIC_UNREADABLE);
        }
    };
    system.emit(&src::LIC_READ);
    system.out.serial(&format!(
        "slic: LIC record seq {} ipl_count {} abnormal_count {}\n",
        record.sequence, record.header.ipl_count, record.header.abnormal_count
    ));

    // Keylock: cmdline wins, then the service console's override, then normal.
    if system.boot.mode.is_none()
        && let Some(next) = record.header.next_mode.take()
    {
        system.mode = next;
        system
            .out
            .serial(&format!("slic: mode {next:?} from LIC override\n"));
    }

    if record.header.ipl_in_progress {
        system.abnormal = true;
        record.header.abnormal_count += 1;
        record.push_history(IplHistory {
            start_time: 0,
            end_time: 0,
            ipl_type: record.header.last_ipl_type,
            mode: record.header.last_ipl_mode,
            slot: record.header.last_slot,
            final_src: *b"        ",
            outcome: Outcome::Abnormal,
        });
        system.emit(&src::PREV_ABNORMAL);
    }

    record.header.ipl_in_progress = true;
    record.header.last_ipl_type = system.ipl_type();
    record.header.last_ipl_mode = system.mode;
    record.header.last_slot = system.boot.slot;
    record.header.ipl_count += 1;
    if let Err(e) = part.write(&mut record) {
        system.out.serial(&format!("slic: LIC write: {e}\n"));
        return Err(&src::LIC_UNREADABLE);
    }
    system.lic = Some((part, record));
    Ok(())
}

/// C6001090 bookkeeping: clear in-progress, remember the good slot, append
/// the history record.
pub fn complete(system: &mut System) {
    let entry = history_entry(system, Outcome::Normal, &src::IPL_COMPLETE);
    let slot = system.boot.slot;
    if let Some((part, record)) = system.lic.as_mut() {
        record.header.ipl_in_progress = false;
        record.header.last_good_slot = slot;
        record.push_history(entry);
        if let Err(e) = part.write(record) {
            system.out.serial(&format!("slic: LIC write: {e}\n"));
        }
    }
    system.complete = true;
}

/// A fatal SRC: record the failed IPL. The in-progress flag is cleared
/// because the failure is now recorded explicitly.
pub fn fail(system: &mut System, final_src: &'static src::Src) {
    let entry = history_entry(system, Outcome::Failed, final_src);
    if let Some((part, record)) = system.lic.as_mut() {
        record.header.ipl_in_progress = false;
        record.push_history(entry);
        let _ = part.write(record);
    }
}

/// Power down bookkeeping: the IPL ended normally (or was never completed).
pub fn shutdown(system: &mut System, final_src: &'static src::Src) {
    let outcome = if system.complete {
        Outcome::Normal
    } else {
        Outcome::Failed
    };
    let entry = history_entry(system, outcome, final_src);
    if let Some((part, record)) = system.lic.as_mut() {
        record.header.ipl_in_progress = false;
        if !system.complete {
            record.push_history(entry);
        }
        let _ = part.write(record);
    }
}

/// Set the keylock override for the next IPL.
pub fn set_next_mode(system: &mut System, mode: IplMode) -> Result<(), String> {
    match system.lic.as_mut() {
        Some((part, record)) => {
            record.header.next_mode = Some(mode);
            part.write(record).map(|_| ()).map_err(|e| e.to_string())
        }
        None => Err("LIC partition not attached".into()),
    }
}

fn history_entry(system: &System, outcome: Outcome, final_src: &src::Src) -> IplHistory {
    IplHistory {
        start_time: system.start_time,
        end_time: now_unix(),
        ipl_type: system.ipl_type(),
        mode: system.mode,
        slot: system.boot.slot,
        final_src: final_src.bytes(),
        outcome,
    }
}
