// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The `*PRDDFN` payload (`docs/LICPGM.md` "Product definition").

use crate::id::{self, LoadId, OptionNo};
use crate::model::definition::{
    Compliance, LicenseDef, LngLoad, LngSel, OptionDef, Prereq, ProductDefinition, RegType,
    Registration, UsageType,
};
use crate::model::{Date, QualName};
use crate::payload::PayloadError;
use crate::payload::codec::{Reader, Writer};
use crate::payload::spec::{Layout, field};

/// Magic of the payload.
pub const MAGIC: &[u8; 8] = b"QSPRDDFN";
/// Format version.
pub const VERSION: u16 = 1;

/// The fixed header.
pub static HEADER: Layout = Layout {
    name: "PrdDfnHeader",
    size: Some(192),
    fields: &[
        field("magic", 0, 8),
        field("format_version", 8, 2),
        field("flags", 10, 2),
        field("product_id", 12, 7),
        field("release", 19, 6),
        field("ibm_alias", 25, 7),
        field("msgf_name", 32, 10),
        field("msgf_lib", 42, 10),
        field("first_copyright", 52, 2),
        field("current_copyright", 54, 2),
        field("release_date", 56, 4),
        field("registration_type", 60, 2),
        field("registration_value", 62, 14),
        field("text", 76, 50),
        field("public_aut", 126, 10),
        field("license_key", 136, 32),
        field("option_count", 168, 2),
        field("prereq_count", 170, 2),
        field("language_count", 172, 2),
        field("ccsid_count", 174, 2),
        field("license_count", 176, 2),
        field("reserved", 178, 14),
    ],
};

/// One option.
pub static OPTION: Layout = Layout {
    name: "PrdDfnOption",
    size: Some(80),
    fields: &[
        field("option", 0, 2),
        field("flags", 2, 2),
        field("msg_id", 4, 7),
        field("feature", 11, 4),
        field("min_base", 15, 6),
        field("text", 21, 50),
        field("reserved", 71, 9),
    ],
};

/// One prerequisite of an option.
pub static PREREQ: Layout = Layout {
    name: "PrdDfnPrereq",
    size: Some(24),
    fields: &[
        field("option", 0, 2),
        field("product_id", 2, 7),
        field("required_option", 9, 2),
        field("min_release", 11, 6),
        field("reserved", 17, 7),
    ],
};

/// One language load list entry.
pub static LANGUAGE: Layout = Layout {
    name: "PrdDfnLanguage",
    size: Some(12),
    fields: &[
        field("option", 0, 2),
        field("load", 2, 8),
        field("reserved", 10, 2),
    ],
};

/// One allowed CCSID.
pub static CCSID: Layout = Layout {
    name: "PrdDfnCcsid",
    size: Some(4),
    fields: &[field("ccsid", 0, 4)],
};

/// The license terms of one feature.
pub static LICENSE: Layout = Layout {
    name: "PrdDfnLicense",
    size: Some(32),
    fields: &[
        field("feature", 0, 4),
        field("term", 4, 6),
        field("usage_type", 10, 1),
        field("compliance", 11, 1),
        field("usage_limit", 12, 4),
        field("threshold", 16, 4),
        field("grace_days", 20, 2),
        field("reserved", 22, 10),
    ],
};

const F_MULTI: u16 = 1;
const F_MIXED: u16 = 2;
const F_LICENSED: u16 = 4;
const F_KEY: u16 = 8;
const OPT_DYN: u16 = 1;
const OPT_MINBASE: u16 = 2;
/// `*NOMAX` in a usage limit.
pub const NOMAX: u32 = u32::MAX;

fn rel_text(r: Option<release::Release>) -> String {
    r.map(id::release_text).unwrap_or_default()
}

fn rel_parse(s: &str, f: &'static str) -> Result<Option<release::Release>, PayloadError> {
    if s.is_empty() {
        Ok(None)
    } else {
        id::parse_release(s)
            .map(Some)
            .map_err(|_| PayloadError::Bad(f))
    }
}

/// Encode a definition.
pub fn encode(d: &ProductDefinition) -> Result<Vec<u8>, PayloadError> {
    let mut w = Writer::default();
    w.bytes(MAGIC);
    w.u16(VERSION);
    let mut flags = 0;
    for (on, bit) in [
        (d.allow_multiple_releases, F_MULTI),
        (d.allow_mixed_releases, F_MIXED),
        (d.licensed, F_LICENSED),
        (d.license_key.is_some(), F_KEY),
    ] {
        if on {
            flags |= bit;
        }
    }
    w.u16(flags);
    w.char(d.product.as_str(), 7, "product_id")?;
    w.char(&id::release_text(d.release), 6, "release")?;
    w.char(
        d.ibm_alias.as_ref().map_or("", |a| a.as_str()),
        7,
        "ibm_alias",
    )?;
    let (mn, ml) = d.msgf.as_ref().map_or((String::new(), String::new()), |q| {
        (q.name.clone(), q.lib.clone())
    });
    w.char(&mn, 10, "msgf_name")?;
    w.char(&ml, 10, "msgf_lib")?;
    w.u16(d.first_copyright.unwrap_or(0));
    w.u16(d.current_copyright.unwrap_or(0));
    w.u32(d.release_date.map_or(0, Date::ymd));
    w.char(d.registration.kind.code(), 2, "registration_type")?;
    w.char(&d.registration.value, 14, "registration_value")?;
    w.char(&d.text, 50, "text")?;
    w.char(&d.public_aut, 10, "public_aut")?;
    w.bytes(&d.license_key.unwrap_or([0; 32]));
    let prereqs: Vec<(OptionNo, &Prereq)> = d
        .options
        .iter()
        .flat_map(|o| o.requires.iter().map(move |p| (o.option, p)))
        .collect();
    w.count16(d.options.len(), "option_count")?;
    w.count16(prereqs.len(), "prereq_count")?;
    w.count16(d.languages.len(), "language_count")?;
    w.count16(d.ccsids.len(), "ccsid_count")?;
    w.count16(d.licenses.len(), "license_count")?;
    w.zeros(14);
    debug_assert_eq!(Some(w.buf.len()), HEADER.size);
    for o in &d.options {
        w.u16(u16::from(o.option.0));
        let mut f = 0;
        if o.dynamic_naming {
            f |= OPT_DYN;
        }
        if o.min_base.is_some() {
            f |= OPT_MINBASE;
        }
        w.u16(f);
        w.char(&o.msg_id, 7, "msg_id")?;
        w.char(o.feature.as_str(), 4, "feature")?;
        w.char(&rel_text(o.min_base), 6, "min_base")?;
        w.char(&o.text, 50, "option text")?;
        w.zeros(9);
    }
    for (opt, p) in prereqs {
        w.u16(u16::from(opt.0));
        w.char(p.product.as_str(), 7, "prereq product")?;
        w.u16(u16::from(p.option.0));
        w.char(&rel_text(p.min_release), 6, "min_release")?;
        w.zeros(7);
    }
    for l in &d.languages {
        w.u16(u16::from(l.option.0));
        w.char(l.load.text(), 8, "load")?;
        w.zeros(2);
    }
    for c in &d.ccsids {
        w.u32(*c);
    }
    for l in &d.licenses {
        w.char(l.feature.as_str(), 4, "feature")?;
        w.char(l.term.as_str(), 6, "term")?;
        w.u8(match l.usage_type {
            UsageType::Concurrent => 0,
            UsageType::Registered => 1,
            UsageType::Processor => 2,
        });
        w.u8(match l.compliance {
            Compliance::OprAction => 0,
            Compliance::Warning => 1,
            Compliance::Keyed => 2,
        });
        w.u32(l.usage_limit.unwrap_or(NOMAX));
        w.u32(l.threshold.unwrap_or(0));
        w.u16(l.grace_days);
        w.zeros(10);
    }
    Ok(w.buf)
}

fn opt(n: u16) -> Result<OptionNo, PayloadError> {
    u8::try_from(n)
        .ok()
        .filter(|n| *n <= 99)
        .map(OptionNo)
        .ok_or(PayloadError::Bad("option"))
}

/// Decode a definition.
pub fn decode(b: &[u8]) -> Result<ProductDefinition, PayloadError> {
    let mut r = Reader::new(b);
    if r.take(8)? != MAGIC {
        return Err(PayloadError::Magic);
    }
    let v = r.u16()?;
    if v != VERSION {
        return Err(PayloadError::Version(v));
    }
    let flags = r.u16()?;
    let product = r
        .char(7)?
        .parse()
        .map_err(|_| PayloadError::Bad("product_id"))?;
    let release = rel_parse(&r.char(6)?, "release")?.ok_or(PayloadError::Bad("release"))?;
    let alias = r.char(7)?;
    let ibm_alias = if alias.is_empty() {
        None
    } else {
        Some(alias.parse().map_err(|_| PayloadError::Bad("ibm_alias"))?)
    };
    let mn = r.char(10)?;
    let ml = r.char(10)?;
    let msgf = (!mn.is_empty()).then(|| QualName::new(&mn, &ml));
    let nz = |v: u16| (v != 0).then_some(v);
    let first_copyright = nz(r.u16()?);
    let current_copyright = nz(r.u16()?);
    let rd = r.u32()?;
    let release_date = if rd == 0 {
        None
    } else {
        Some(Date::from_ymd(rd).ok_or(PayloadError::Bad("release_date"))?)
    };
    let kind = RegType::from_code(&r.char(2)?).ok_or(PayloadError::Bad("registration_type"))?;
    let value = r.char(14)?;
    let text = r.char(50)?;
    let public_aut = r.char(10)?;
    let key: [u8; 32] = r.take(32)?.try_into().expect("32 bytes");
    let n_opt = r.u16()?;
    let n_pre = r.u16()?;
    let n_lng = r.u16()?;
    let n_ccsid = r.u16()?;
    let n_lic = r.u16()?;
    r.skip(14)?;
    let mut options = Vec::new();
    for _ in 0..n_opt {
        let option = opt(r.u16()?)?;
        let f = r.u16()?;
        let msg_id = r.char(7)?;
        let feature = LoadId::feature(&r.char(4)?).map_err(|_| PayloadError::Bad("feature"))?;
        let min_base = rel_parse(&r.char(6)?, "min_base")?;
        let otext = r.char(50)?;
        r.skip(9)?;
        options.push(OptionDef {
            option,
            text: otext,
            msg_id,
            dynamic_naming: f & OPT_DYN != 0,
            feature,
            min_base: if f & OPT_MINBASE != 0 { min_base } else { None },
            requires: Vec::new(),
        });
    }
    for _ in 0..n_pre {
        let on = opt(r.u16()?)?;
        let product = r
            .char(7)?
            .parse()
            .map_err(|_| PayloadError::Bad("prereq"))?;
        let option = opt(r.u16()?)?;
        let min_release = rel_parse(&r.char(6)?, "min_release")?;
        r.skip(7)?;
        let o = options
            .iter_mut()
            .find(|o| o.option == on)
            .ok_or(PayloadError::Bad("prereq option"))?;
        o.requires.push(Prereq {
            product,
            option,
            min_release,
        });
    }
    let mut languages = Vec::new();
    for _ in 0..n_lng {
        let option = opt(r.u16()?)?;
        let load = LngSel::parse(&r.char(8)?).map_err(|_| PayloadError::Bad("load"))?;
        r.skip(2)?;
        languages.push(LngLoad { option, load });
    }
    let mut ccsids = Vec::new();
    for _ in 0..n_ccsid {
        ccsids.push(r.u32()?);
    }
    let mut licenses = Vec::new();
    for _ in 0..n_lic {
        let feature = LoadId::feature(&r.char(4)?).map_err(|_| PayloadError::Bad("feature"))?;
        let term = r.char(6)?.parse().map_err(|_| PayloadError::Bad("term"))?;
        let usage_type = match r.u8()? {
            0 => UsageType::Concurrent,
            1 => UsageType::Registered,
            2 => UsageType::Processor,
            _ => return Err(PayloadError::Bad("usage_type")),
        };
        let compliance = match r.u8()? {
            0 => Compliance::OprAction,
            1 => Compliance::Warning,
            2 => Compliance::Keyed,
            _ => return Err(PayloadError::Bad("compliance")),
        };
        let lim = r.u32()?;
        let th = r.u32()?;
        let grace_days = r.u16()?;
        r.skip(10)?;
        licenses.push(LicenseDef {
            feature,
            term,
            usage_type,
            compliance,
            usage_limit: (lim != NOMAX).then_some(lim),
            grace_days,
            threshold: (th != 0).then_some(th),
        });
    }
    if r.pos() != b.len() {
        return Err(PayloadError::Trailing(b.len() - r.pos()));
    }
    Ok(ProductDefinition {
        product,
        release,
        ibm_alias,
        text,
        msgf,
        first_copyright,
        current_copyright,
        release_date,
        allow_multiple_releases: flags & F_MULTI != 0,
        allow_mixed_releases: flags & F_MIXED != 0,
        registration: Registration { kind, value },
        public_aut,
        options,
        languages,
        ccsids,
        licensed: flags & F_LICENSED != 0,
        license_key: (flags & F_KEY != 0).then_some(key),
        licenses,
    })
}
