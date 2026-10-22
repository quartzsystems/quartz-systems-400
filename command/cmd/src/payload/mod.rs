// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*CMD` payload: a fixed header (`CMD.md` §3), then the compiled
//! definition, the definition source and the `CHGCMDDFT` overlay as
//! sections. A proxy command carries its target in the header and no
//! definition.

use ipc::{Dec, Wire, wire_struct};

use crate::def::{CmdDef, Qual};
use crate::error::{CmdError, Result};

/// The header's magic.
pub const MAGIC: &[u8; 8] = b"QS400CMD";
/// The payload version.
pub const VERSION: u32 = 1;
/// The header's size.
pub const HDR_LEN: usize = 256;

/// The header's fields: name, offset, size; `CMD.md` §3 has the same
/// table and `tests/spec.rs` compares them.
pub const LAYOUT: &[(&str, usize, usize)] = &[
    ("Magic", 0, 8),
    ("Version", 8, 4),
    ("Flags", 12, 4),
    ("CppName", 16, 10),
    ("CppLib", 26, 10),
    ("TargetName", 36, 10),
    ("TargetLib", 46, 10),
    ("Allow", 56, 2),
    ("Mode", 58, 1),
    ("MaxPos", 59, 1),
    ("ThdSafe", 60, 1),
    ("AlwLmtUsr", 61, 1),
    ("Reserved", 62, 2),
    ("Created", 64, 8),
    ("DefOffset", 72, 4),
    ("DefLen", 76, 4),
    ("SrcOffset", 80, 4),
    ("SrcLen", 84, 4),
    ("DftOffset", 88, 4),
    ("DftLen", 92, 4),
    ("Reserved2", 96, 160),
];

/// Flag bit: a proxy command.
pub const FLAG_PROXY: u32 = 1;
/// Flag bit: a `CHGCMDDFT` overlay is present.
pub const FLAG_OVERLAY: u32 = 2;

/// A default changed by `CHGCMDDFT`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Override {
    /// The keyword.
    pub kwd: String,
    /// The new default, as source text.
    pub dft: String,
}

wire_struct!(Override { kwd, dft });

/// A `*CMD` object's contents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CmdObject {
    /// The definition as compiled (defaults as the source gave them).
    pub def: CmdDef,
    /// The definition source, kept for `DSPCMD` and `RTVCMDSRC`-style
    /// use; empty when it was not kept.
    pub source: String,
    /// `CHGCMDDFT` overrides, latest last.
    pub overrides: Vec<Override>,
    /// The target of a proxy command.
    pub proxy: Option<Qual>,
    /// Creation time, Unix seconds.
    pub created: u64,
}

impl CmdObject {
    /// A command from its definition.
    #[must_use]
    pub fn new(def: CmdDef, source: String, created: u64) -> Self {
        Self {
            def,
            source,
            overrides: Vec::new(),
            proxy: None,
            created,
        }
    }

    /// A proxy for `target`.
    #[must_use]
    pub fn proxy(name: &str, target: Qual, text: &str, created: u64) -> Self {
        Self {
            def: CmdDef {
                name: name.to_ascii_uppercase(),
                text: text.into(),
                ..CmdDef::default()
            },
            source: String::new(),
            overrides: Vec::new(),
            proxy: Some(target),
            created,
        }
    }

    /// The definition with the overrides applied.
    #[must_use]
    pub fn effective(&self) -> CmdDef {
        let mut d = self.def.clone();
        for o in &self.overrides {
            if let Some(p) = d.parms.iter_mut().find(|p| p.kwd == o.kwd) {
                p.dft = Some(o.dft.clone());
            }
        }
        d
    }

    /// Encode.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut def = Vec::new();
        self.def.put(&mut def);
        let src = self.source.as_bytes();
        let mut dft = Vec::new();
        if !self.overrides.is_empty() {
            self.overrides.put(&mut dft);
        }
        let mut out = vec![0u8; HDR_LEN];
        out[0..8].copy_from_slice(MAGIC);
        out[8..12].copy_from_slice(&VERSION.to_le_bytes());
        let mut flags = 0;
        if self.proxy.is_some() {
            flags |= FLAG_PROXY;
        }
        if !self.overrides.is_empty() {
            flags |= FLAG_OVERLAY;
        }
        out[12..16].copy_from_slice(&flags.to_le_bytes());
        put_name(&mut out[16..26], &self.def.cpp.name);
        put_name(&mut out[26..36], &self.def.cpp.lib);
        if let Some(t) = &self.proxy {
            put_name(&mut out[36..46], &t.name);
            put_name(&mut out[46..56], &t.lib);
        } else {
            out[36..56].fill(0x40);
        }
        out[56..58].copy_from_slice(&self.def.allow.0.to_le_bytes());
        out[58] = self.def.mode.0;
        out[59] = self.def.maxpos.unwrap_or(255);
        out[60] = match self.def.thdsafe {
            crate::def::ThdSafe::No => 0,
            crate::def::ThdSafe::Cond => 1,
            crate::def::ThdSafe::Yes => 2,
        };
        out[61] = u8::from(self.def.alwlmtusr);
        out[64..72].copy_from_slice(&self.created.to_le_bytes());
        let mut at = HDR_LEN;
        let mut section = |out: &mut Vec<u8>, off: usize, data: &[u8]| {
            let start = u32::try_from(at).unwrap_or(0);
            let len = u32::try_from(data.len()).unwrap_or(0);
            out[off..off + 4].copy_from_slice(&start.to_le_bytes());
            out[off + 4..off + 8].copy_from_slice(&len.to_le_bytes());
            out.extend_from_slice(data);
            at += data.len();
        };
        section(&mut out, 72, &def);
        section(&mut out, 80, src);
        section(&mut out, 88, &dft);
        out
    }

    /// Decode.
    pub fn decode(b: &[u8]) -> Result<Self> {
        if b.len() < HDR_LEN || &b[0..8] != MAGIC {
            return Err(CmdError::detail("CPF0006", "not a command object"));
        }
        let version = u32::from_le_bytes(b[8..12].try_into().expect("sized"));
        if version > VERSION {
            return Err(CmdError::detail(
                "CPF0006",
                format!("command object version {version} is newer than this system's"),
            ));
        }
        let flags = u32::from_le_bytes(b[12..16].try_into().expect("sized"));
        let section = |off: usize| -> Result<&[u8]> {
            let start = u32::from_le_bytes(b[off..off + 4].try_into().expect("sized")) as usize;
            let len = u32::from_le_bytes(b[off + 4..off + 8].try_into().expect("sized")) as usize;
            b.get(start..start + len)
                .ok_or_else(|| CmdError::detail("CPF0006", "command object truncated"))
        };
        let def = CmdDef::get(&mut Dec::new(section(72)?))?;
        let source = String::from_utf8_lossy(section(80)?).into_owned();
        let overrides = if flags & FLAG_OVERLAY != 0 {
            Vec::<Override>::get(&mut Dec::new(section(88)?))?
        } else {
            Vec::new()
        };
        let proxy = if flags & FLAG_PROXY != 0 {
            Some(Qual::new(
                &ccsid::decode_trimmed(&b[36..46]),
                &ccsid::decode_trimmed(&b[46..56]),
            ))
        } else {
            None
        };
        let created = u64::from_le_bytes(b[64..72].try_into().expect("sized"));
        Ok(Self {
            def,
            source,
            overrides,
            proxy,
            created,
        })
    }
}

/// A CP037 name into a header field.
fn put_name(field: &mut [u8], name: &str) {
    let mut b = ccsid::to_ebcdic(name).unwrap_or_default();
    b.resize(field.len(), 0x40);
    field.copy_from_slice(&b);
}

/// What the header says about a command without decoding the sections:
/// the processing program, the proxy target and `ALLOW`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Head {
    /// The processing program.
    pub cpp: Qual,
    /// The target of a proxy.
    pub proxy: Option<Qual>,
    /// `ALLOW` bits.
    pub allow: u16,
}

/// Read the header alone.
pub fn head(b: &[u8]) -> Result<Head> {
    if b.len() < HDR_LEN || &b[0..8] != MAGIC {
        return Err(CmdError::detail("CPF0006", "not a command object"));
    }
    let flags = u32::from_le_bytes(b[12..16].try_into().expect("sized"));
    Ok(Head {
        cpp: Qual::new(
            &ccsid::decode_trimmed(&b[16..26]),
            &ccsid::decode_trimmed(&b[26..36]),
        ),
        proxy: (flags & FLAG_PROXY != 0).then(|| {
            Qual::new(
                &ccsid::decode_trimmed(&b[36..46]),
                &ccsid::decode_trimmed(&b[46..56]),
            )
        }),
        allow: u16::from_le_bytes(b[56..58].try_into().expect("sized")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::def::{Parm, ParmType};

    #[test]
    fn round_trip() {
        let mut def = CmdDef {
            name: "CRTLIB".into(),
            prompt: "Create Library".into(),
            maxpos: Some(2),
            cpp: Qual::new("QLICRLIB", "QSYS"),
            ..CmdDef::default()
        };
        def.parms.push(Parm {
            kwd: "LIB".into(),
            ty: ParmType::SName,
            min: 1,
            ..Parm::default()
        });
        def.parms.push(Parm {
            kwd: "TYPE".into(),
            dft: Some("*PROD".into()),
            values: vec!["*PROD".into(), "*TEST".into()],
            ..Parm::default()
        });
        let mut o = CmdObject::new(def, "CMD PROMPT('Create Library')".into(), 7);
        o.overrides.push(Override {
            kwd: "TYPE".into(),
            dft: "*TEST".into(),
        });
        let b = o.encode();
        assert_eq!(&b[0..8], MAGIC);
        let back = CmdObject::decode(&b).unwrap();
        assert_eq!(back, o);
        assert_eq!(
            back.effective().parm("TYPE").unwrap().dft.as_deref(),
            Some("*TEST")
        );
        let h = head(&b).unwrap();
        assert_eq!(h.cpp, Qual::new("QLICRLIB", "QSYS"));
        assert_eq!(h.proxy, None);
        let p = CmdObject::proxy("MYLIB", Qual::new("CRTLIB", "QSYS"), "proxy", 1);
        let pb = p.encode();
        assert_eq!(head(&pb).unwrap().proxy, Some(Qual::new("CRTLIB", "QSYS")));
        assert_eq!(CmdObject::decode(&pb).unwrap(), p);
        assert!(CmdObject::decode(b"nope").is_err());
    }
}
