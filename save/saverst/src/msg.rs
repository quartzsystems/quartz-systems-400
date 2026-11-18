// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The messages the engine reports and their data. The identifiers are
//! IBM i V7R1M0's for each meaning (`tools/conformance/oracle/save/primary/README.md`,
//! rules in brackets) and the data is laid out as IBM declares it
//! (`tools/conformance/oracle/msgdata/fields.txt`), so a program that
//! monitors for a message reads what it expects. The text is
//! `runtime/msgf/msgs/qcpfmsg.toml`'s.

use crate::restore::RestoreReport;
use crate::save::{ObjResult, SaveReport};

/// Objects saved from one library to a save file, all of them [5].
pub const SAVED_LIB: &str = "CPC3722";
/// The same to media.
pub const SAVED_LIB_MEDIA: &str = "CPC3701";
/// Objects saved, some not included (`OBJJRN(*NO)`, `FILEMBR`) [5].
pub const SAVED_NOT_INCLUDED: &str = "CPC3723";
/// Objects saved, some not saved: the escape, to a save file [5].
pub const SAVED_LIB_PARTIAL: &str = "CPF3701";
/// The same to media.
pub const SAVED_LIB_PARTIAL_MEDIA: &str = "CPF3771";
/// Several libraries, some not saved or saved in part.
pub const SAVED_LIBS_PARTIAL: &str = "CPF3777";
/// Nothing was saved or restored for the library [1, 5].
pub const NONE_FOR_LIB: &str = "CPF3770";
/// A library to save or restore into does not exist [5].
pub const NO_LIBRARY: &str = "CPF3781";
/// A named object to save was not found [5].
pub const NOT_FOUND_SAVE: &str = "CPF3764";
/// An object in use, not saved or not restored [9].
pub const IN_USE: &str = "CPF3761";
/// Not saved or not restored: not authorized.
pub const NOT_AUTHORIZED: &str = "CPF3760";
/// The save file being written, inside the saved library, saved
/// without its data [21].
pub const SAVF_NO_DATA: &str = "CPI374B";
/// Objects restored to a library, all of them [1].
pub const RESTORED_LIB: &str = "CPC3703";
/// Objects restored, some not, or differences accepted: the escape [1, 4].
pub const RESTORED_LIB_PARTIAL: &str = "CPF3773";
/// An object not restored, after its own reason [6-11].
pub const NOT_RESTORED: &str = "CPF3756";
/// Not selected by `OPTION(*NEW)`: it exists [2].
pub const SKIP_EXISTS: &str = "CPF3725";
/// Not selected by `OPTION(*OLD)`: it does not exist [2].
pub const SKIP_MISSING: &str = "CPF3724";
/// Not selected by `OPTION(*FREE)`: its storage was not freed [2].
pub const SKIP_NOT_FREED: &str = "CPF3726";
/// An existing journal is never replaced [6].
pub const JOURNAL_EXISTS: &str = "CPF7085";
/// An existing data queue is never replaced [7].
pub const DTAQ_EXISTS: &str = "CPD2415";
/// An attached receiver is never replaced [8].
pub const RECEIVER_ATTACHED: &str = "CPF7081";
/// An existing detached receiver is not replaced [8].
pub const RECEIVER_EXISTS: &str = "CPF3734";
/// A logical file whose physical file is missing [10].
pub const NOT_RESTORED_BASED_ON: &str = "CPF3204";
/// `MBROPT(*MATCH)` and the member lists differ [11].
pub const NOT_RESTORED_MEMBERS: &str = "CPF3284";
/// Level identifiers differ [12].
pub const NOT_RESTORED_LEVEL: &str = "CPF3283";
/// `ALWOBJDIF(*ALL)` renamed the existing file [12].
pub const FILE_RENAMED: &str = "CPI320A";
/// The owner differs and `ALWOBJDIF` does not allow it [13].
pub const NOT_RESTORED_OWNER: &str = "CPF3766";
/// Restored; the existing owner was kept [13].
pub const OWNER_KEPT: &str = "CPF370B";
/// The primary group differs and `ALWOBJDIF` does not allow it [13].
pub const NOT_RESTORED_PGP: &str = "CPD37A2";
/// Restored; the existing primary group was kept [13].
pub const PGP_KEPT: &str = "CPI3811";
/// The authorization list differs and `ALWOBJDIF` does not allow it.
pub const NOT_RESTORED_AUTL: &str = "CPF3881";
/// Restored; the existing authorization list was kept.
pub const AUTL_KEPT: &str = "CPI371D";
/// Restored as a new object whose authorization list is missing: its
/// public authority is `*EXCLUDE`.
pub const AUTL_MISSING: &str = "CPF3836";
/// Security or data format changes occurred [4].
pub const CHANGES: &str = "CPF3848";
/// The saved object could not be read or written: damaged.
pub const NOT_RESTORED_DAMAGED: &str = "CPF3795";
/// A named object is not in the save, nothing restored [24].
pub const NOT_IN_SAVE: &str = "CPF3801";
/// One of several named objects is not in the save [24].
pub const NOT_IN_LIST: &str = "CPD3773";
/// Nothing named was in the save: the escape [24].
pub const NONE_IN_SAVE: &str = "CPF3780";
/// An owner that does not exist: the default owner.
pub const DEFAULT_OWNER: &str = "CPF3719";
/// Journaling could not be started again [18].
pub const JOURNAL_MISSING: &str = "CPF3294";
/// `QALWOBJRST` does not allow a system-state object.
pub const NOT_RESTORED_SYSTEM_STATE: &str = "CPD3740";
/// `QALWOBJRST` does not allow an object that adopts authority.
pub const NOT_RESTORED_ADOPTS: &str = "CPD373F";
/// `QVFYOBJRST`: unsigned, or a signature that does not verify.
pub const NOT_RESTORED_SIGNATURE: &str = "CPD37C0";
/// Existing spooled files not restored again [27].
pub const SPOOL_EXISTS: &str = "CPI3733";

/// The old names, kept so callers read as before.
pub const NOT_SAVED_IN_USE: &str = IN_USE;
/// See [`IN_USE`].
pub const NOT_RESTORED_IN_USE: &str = IN_USE;
/// See [`NOT_AUTHORIZED`].
pub const NOT_SAVED_AUTH: &str = NOT_AUTHORIZED;
/// See [`NOT_AUTHORIZED`].
pub const NOT_RESTORED_AUTH: &str = NOT_AUTHORIZED;

/// Message data built field by field, as an entry declares it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Data(Vec<u8>);

impl Data {
    /// No fields yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `*CHAR n`: CP037, padded with blanks or cut.
    #[must_use]
    pub fn char(mut self, text: &str, n: usize) -> Self {
        let mut b = ebcdic(text);
        b.resize(n, 0x40);
        self.0.extend_from_slice(&b);
        self
    }

    /// `*BIN 2`.
    #[must_use]
    pub fn bin2(mut self, v: i16) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// `*BIN 4` from a count.
    #[must_use]
    pub fn bin4(mut self, v: usize) -> Self {
        let v = i32::try_from(v).unwrap_or(i32::MAX);
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }

    /// `*DTS` from Unix seconds: microseconds, as `msgf` reads it.
    #[must_use]
    pub fn dts(mut self, secs: u64) -> Self {
        let micros = i64::try_from(secs).unwrap_or(0).saturating_mul(1_000_000);
        self.0.extend_from_slice(&micros.to_be_bytes());
        self
    }

    /// `*CHAR *VARY 4`: the length, then the text.
    #[must_use]
    pub fn vary4(mut self, text: &str) -> Self {
        let b = ebcdic(text);
        let n = u32::try_from(b.len()).unwrap_or(0);
        self.0.extend_from_slice(&n.to_be_bytes());
        self.0.extend_from_slice(&b);
        self
    }

    /// The bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

fn ebcdic(text: &str) -> Vec<u8> {
    ccsid::to_ebcdic(text).unwrap_or_else(|_| {
        text.chars()
            .map(|c| ccsid::byte_of(c).unwrap_or(0x40))
            .collect()
    })
}

/// A type's name: `*FILE` with the star, `FILE` without.
#[must_use]
pub fn type_name(code: u16, star: bool) -> String {
    let name = store::obj::type_by_code(code).map_or("", |t| t.name);
    if star {
        name.to_string()
    } else {
        name.trim_start_matches('*').to_string()
    }
}

/// The data of a message about one object, by identifier: object,
/// type and library where IBM declares them, the object and library
/// where it declares two, the library alone for a library.
#[must_use]
pub fn object(msgid: &str, name: &str, type_code: u16, library: &str) -> Vec<u8> {
    let d = Data::new();
    match msgid {
        NO_LIBRARY | NONE_IN_SAVE => d.char(library, 10),
        JOURNAL_EXISTS
        | DTAQ_EXISTS
        | RECEIVER_ATTACHED
        | NOT_RESTORED_BASED_ON
        | NOT_RESTORED_MEMBERS
        | NOT_RESTORED_LEVEL
        | SAVF_NO_DATA => d.char(name, 10).char(library, 10),
        JOURNAL_MISSING => d.char(name, 10).char(library, 10).char("*N", 10),
        // These write the type as `*&2`.
        NOT_RESTORED_SYSTEM_STATE | NOT_RESTORED_ADOPTS => d
            .char(name, 10)
            .char(&type_name(type_code, false), 7)
            .char(library, 10),
        _ => d
            .char(name, 10)
            .char(&type_name(type_code, true), 7)
            .char(library, 10),
    }
    .into_bytes()
}

/// The data of an object's outcome message.
#[must_use]
pub fn of_result(o: &ObjResult) -> Option<(&'static str, Vec<u8>)> {
    o.msgid
        .map(|id| (id, object(id, &o.name, o.type_code, &o.library)))
}

/// A message the engine sends beside an object's outcome: a default
/// owner, an owner or group kept, a file renamed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Note {
    /// The message.
    pub msgid: &'static str,
    /// The object.
    pub name: String,
    /// Its type code.
    pub type_code: u16,
    /// Its library.
    pub library: String,
    /// What is on the system: the owner, group or authorization list
    /// kept, the name the existing file was given.
    pub existing: String,
    /// What the save had.
    pub saved: String,
}

impl Note {
    /// A note about an object.
    #[must_use]
    pub fn new(msgid: &'static str, name: &str, type_code: u16, library: &str) -> Self {
        Self {
            msgid,
            name: name.into(),
            type_code,
            library: library.into(),
            ..Self::default()
        }
    }

    /// With what the system and the save have.
    #[must_use]
    pub fn values(mut self, existing: &str, saved: &str) -> Self {
        self.existing = existing.into();
        self.saved = saved.into();
        self
    }

    /// Its data, as the entry declares it.
    #[must_use]
    pub fn data(&self) -> Vec<u8> {
        let d = Data::new();
        match self.msgid {
            FILE_RENAMED => d
                .char(&self.name, 10)
                .char(&self.library, 10)
                .char(&self.existing, 10)
                .into_bytes(),
            AUTL_MISSING => d
                .char(&self.name, 10)
                .char(&type_name(self.type_code, true), 7)
                .char(&self.library, 10)
                .char(&self.existing, 10)
                .into_bytes(),
            OWNER_KEPT | PGP_KEPT => d
                .char(&self.name, 10)
                .char(&type_name(self.type_code, true), 7)
                .char(&self.library, 10)
                .char(&self.existing, 10)
                .char(&self.saved, 10)
                .into_bytes(),
            AUTL_KEPT => d
                .char(&type_name(self.type_code, true), 7)
                .vary4(&format!("{}/{}", self.library, self.name))
                .char(&self.existing, 10)
                .char(&self.saved, 10)
                .into_bytes(),
            id => object(id, &self.name, self.type_code, &self.library),
        }
    }
}

/// How a library's restore ended: `CPC3703` when everything was
/// restored, `CPF3773` when not, `CPF3770` when nothing, `CPF3780` when
/// nothing named was in the save. `saved` is the library saved, `to`
/// the library restored into, `spooled` the spooled files brought
/// back, `now` Unix seconds.
#[must_use]
pub fn restore_end(
    r: &RestoreReport,
    saved: &str,
    to: &str,
    spooled: usize,
    now: u64,
) -> (&'static str, Vec<u8>) {
    let id = r.completion();
    let data = match id {
        NONE_IN_SAVE => Data::new().char(saved, 10),
        NONE_FOR_LIB => Data::new().char(to, 10).dts(now),
        _ => {
            let d = Data::new()
                .bin4(r.restored())
                .bin4(r.not_restored())
                .char(saved, 10)
                .char(to, 10)
                .dts(now)
                .char("", 4)
                .char("", 10)
                .char("", 97)
                .char("", 32)
                .bin4(spooled);
            if id == RESTORED_LIB_PARTIAL {
                d.bin4(0)
            } else {
                d
            }
        }
    };
    (id, data.into_bytes())
}

/// Where a save went: a save file or volumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dest {
    /// A save file and its library.
    Savf(String, String),
    /// The volumes written, the device, and the volume it ended on.
    Media {
        /// The volumes, blank separated.
        volumes: String,
        /// The device.
        device: String,
        /// The last volume.
        last: String,
    },
}

/// How a library's save ended: `CPC3722` (`CPC3701` to media) when
/// everything was saved, `CPF3701` (`CPF3771`) when not, `CPF3770` when
/// nothing.
#[must_use]
pub fn save_end(r: &SaveReport, dest: &Dest, now: u64) -> (&'static str, Vec<u8>) {
    let (saved, not) = (r.saved(), r.not_saved());
    let id = r.completion();
    if id == NONE_FOR_LIB {
        return (id, Data::new().char(&r.library, 10).dts(now).into_bytes());
    }
    let partial = id == SAVED_LIB_PARTIAL;
    match dest {
        Dest::Savf(file, lib) => {
            let d = Data::new()
                .bin4(saved)
                .bin4(not)
                .char(&r.library, 10)
                .char(file, 10)
                .char(lib, 10)
                .char("", 50)
                .dts(now)
                .char("", 618)
                .bin4(0);
            let d = if partial { d.bin4(0) } else { d };
            (id, d.into_bytes())
        }
        Dest::Media {
            volumes,
            device,
            last,
        } => {
            let d = Data::new()
                .bin4(saved)
                .bin4(not)
                .char(&r.library, 10)
                .char(volumes, 70)
                .dts(now)
                .char("", 4)
                .char("", 15)
                .bin4(0)
                .char("", 6)
                .char(device, 10)
                .char("", 82)
                .char("", 455)
                .char(last, 32)
                .char("", 10)
                .bin4(0);
            let d = if partial { d.bin4(0) } else { d };
            let id = if partial {
                SAVED_LIB_PARTIAL_MEDIA
            } else {
                SAVED_LIB_MEDIA
            };
            (id, d.into_bytes())
        }
    }
}

/// `CPF3777`: of several libraries, `full` saved, `partial` saved in
/// part, `none` not saved.
#[must_use]
pub fn libraries_end(full: usize, none: usize, partial: usize, now: u64) -> Vec<u8> {
    Data::new()
        .bin4(full)
        .bin4(none)
        .dts(now)
        .char("", 6)
        .char("", 10)
        .bin4(partial)
        .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_laid_out_in_order() {
        let d = Data::new()
            .bin4(3)
            .char("QGPL", 10)
            .dts(1)
            .vary4("A/B")
            .into_bytes();
        assert_eq!(&d[..4], &[0, 0, 0, 3]);
        assert_eq!(d[4], 0xd8, "Q in CP037");
        assert_eq!(&d[8..14], &[0x40; 6]);
        assert_eq!(i64::from_be_bytes(d[14..22].try_into().unwrap()), 1_000_000);
        assert_eq!(&d[22..26], &[0, 0, 0, 3]);
        assert_eq!(d.len(), 29);
    }

    #[test]
    fn an_object_message_takes_ibms_fields() {
        let d = object(NOT_RESTORED, "CUST", store::obj::types::FILE, "QGPL");
        assert_eq!(d.len(), 27);
        assert_eq!(ccsid::decode_trimmed(&d[10..17]), "*FILE");
        let d = object(NOT_RESTORED_ADOPTS, "P", store::obj::types::PGM, "L");
        assert_eq!(ccsid::decode_trimmed(&d[10..17]), "PGM");
        assert_eq!(object(NO_LIBRARY, "X", 0, "LIB").len(), 10);
        assert_eq!(object(DTAQ_EXISTS, "DQ", 0, "LIB").len(), 20);
    }
}
