// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSPKGPRD: build a product's `*PRDDFN` and `*PRDLOD`s from a TOML
//! manifest in the IFS. The build pipeline and the shipped catalog use
//! the same format.

pub mod schema;

use std::fmt;

use release::Release;

use crate::id::{self, LicTerm, LoadId, LoadType, OptionNo, ProductId};
use crate::model::definition::{
    Compliance, LicenseDef, LngLoad, LngSel, OptionDef, Prereq, ProductDefinition, RegType,
    Registration, UsageType,
};
use crate::model::load::{DirEntry, LibEntry, LibType, LoadKey, ObjEntry, ProductLoad};
use crate::model::{Date, QualName};
use crate::payload::{PayloadError, prddfn, prdlod};

/// One problem in a manifest: where, and what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestError {
    /// `product.id`, `option[2].objects[0].lib`, ...
    pub at: String,
    /// What is wrong, in our words.
    pub message: String,
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.at, self.message)
    }
}

/// How strictly to read a manifest.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Allow the Quartz-reserved `QS` product IDs (the build pipeline
    /// and the shipped catalog).
    pub allow_quartz: bool,
    /// Allow the catalog's `status` field.
    pub catalog: bool,
}

/// Whether a catalog product is shipped with the system or only has
/// its IDs reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// Installed with the operating system.
    Shipped,
    /// Registered, not installed: a future product.
    Reserved,
}

/// What a manifest builds.
#[derive(Debug, Clone, PartialEq)]
pub struct Package {
    /// The `*PRDDFN` object's name.
    pub definition_name: QualName,
    /// The definition.
    pub definition: ProductDefinition,
    /// The `*PRDLOD` objects, code loads first per option.
    pub loads: Vec<(QualName, ProductLoad)>,
    /// The agreement text, when the manifest holds it.
    pub agreement: Option<String>,
    /// Catalog status.
    pub status: Status,
    /// Catalog notes per option: QS/400 components and IBM's analogue.
    pub notes: Vec<(OptionNo, Vec<String>, Option<String>)>,
}

impl Package {
    /// The `*PRDDFN` payload.
    pub fn definition_payload(&self) -> Result<Vec<u8>, PayloadError> {
        prddfn::encode(&self.definition)
    }

    /// Each `*PRDLOD` payload with its name.
    pub fn load_payloads(&self) -> Result<Vec<(QualName, Vec<u8>)>, PayloadError> {
        self.loads
            .iter()
            .map(|(q, l)| Ok((q.clone(), prdlod::encode(l)?)))
            .collect()
    }
}

/// Where agreement documents written from manifest text go.
#[must_use]
pub fn agreement_path(p: &ProductId, r: Release, lng: &str) -> String {
    format!(
        "/QIBM/ProdData/LicenseDoc/{p}_{}/{lng}.txt",
        id::release_text(r)
    )
}

/// Parse and check a manifest.
pub fn parse(text: &str, opts: Options) -> Result<Package, Vec<ManifestError>> {
    let m: schema::Manifest = toml::from_str(text).map_err(|e| {
        vec![ManifestError {
            at: "manifest".into(),
            message: format!("not a valid manifest: {}", e.message()),
        }]
    })?;
    build_manifest(&m, opts)
}

/// Collects errors while building.
struct Ctx {
    errs: Vec<ManifestError>,
}

impl Ctx {
    fn err(&mut self, at: impl Into<String>, message: impl Into<String>) {
        self.errs.push(ManifestError {
            at: at.into(),
            message: message.into(),
        });
    }

    fn text(&mut self, at: &str, s: &str, max: usize) {
        if s.chars().count() > max {
            self.err(at, format!("longer than {max} characters"));
        }
        if !s.chars().all(ccsid::is_representable) {
            self.err(at, "holds characters the system cannot store");
        }
    }

    fn name(&mut self, at: &str, s: &str) {
        if !id::is_name(s) {
            self.err(at, format!("{s:?} is not a valid object name"));
        }
    }

    fn release(&mut self, at: &str, s: &str) -> Option<Release> {
        id::parse_release(s)
            .map_err(|_| self.err(at, format!("{s:?} is not a release (VxRyMz)")))
            .ok()
    }

    fn qual(&mut self, at: &str, s: &str) -> QualName {
        let q = QualName::parse(s);
        if q.lib != "*LIBL" {
            self.name(at, &q.lib);
        }
        self.name(at, &q.name);
        q
    }
}

fn special_or_name(s: &str) -> bool {
    s == "*ALL" || id::is_name(s)
}

#[allow(clippy::too_many_lines)]
/// Check a parsed manifest and build its objects.
pub fn build_manifest(m: &schema::Manifest, opts: Options) -> Result<Package, Vec<ManifestError>> {
    let mut c = Ctx { errs: Vec::new() };
    let p = &m.product;
    let product: ProductId = p.id.parse().unwrap_or_else(|_| {
        c.err(
            "product.id",
            format!("{:?} is not a product ID (nxxxxxx)", p.id),
        );
        ProductId::OS.parse().expect("valid")
    });
    if product.is_quartz() && !opts.allow_quartz {
        c.err(
            "product.id",
            "IDs starting QS are reserved for Quartz products",
        );
    }
    let rel = match &p.release {
        Some(r) => c.release("product.release", r).unwrap_or(release::RELEASE),
        None => release::RELEASE,
    };
    let status = match (p.status.as_deref(), opts.catalog) {
        (None | Some("shipped"), true) | (None, false) => Status::Shipped,
        (Some("reserved"), true) => Status::Reserved,
        (Some(s), true) => {
            c.err(
                "product.status",
                format!("{s:?} is not shipped or reserved"),
            );
            Status::Shipped
        }
        (Some(_), false) => {
            c.err(
                "product.status",
                "only the shipped catalog may set a status",
            );
            Status::Shipped
        }
    };
    c.text("product.text", &p.text, 50);
    c.name("product.library", &p.library);
    let ibm_alias = p.ibm_alias.as_ref().and_then(|a| {
        a.parse::<ProductId>()
            .map_err(|_| c.err("product.ibm_alias", format!("{a:?} is not a product ID")))
            .ok()
    });
    let obj_name = p.object.clone().unwrap_or_else(|| p.id.clone());
    c.name("product.object", &obj_name);
    let msgf = p.msgf.as_ref().map(|s| c.qual("product.msgf", s));
    if let (Some(a), Some(b)) = (p.first_copyright, p.current_copyright)
        && a > b
    {
        c.err(
            "product.first_copyright",
            "is after the current copyright year",
        );
    }
    let release_date = p.release_date.as_ref().and_then(|s| {
        Date::parse(s)
            .ok_or_else(|| c.err("product.release_date", format!("{s:?} is not YYYY-MM-DD")))
            .ok()
    });
    let kind = match p.registration_type.as_deref() {
        None => RegType::OsSame,
        Some(t) => RegType::from_name(t).unwrap_or_else(|| {
            c.err(
                "product.registration_type",
                format!("{t:?} is not *PHONE, *CUSTOMER or *OPSYS"),
            );
            RegType::OsSame
        }),
    };
    let rv = &p.registration_value;
    if rv.len() > 14
        || !rv
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        c.err(
            "product.registration_value",
            "must be up to 14 of A-Z and 0-9",
        );
    }
    let license_key = p.license_key.as_ref().and_then(|h| {
        let k = parse_key(h);
        if k.is_none() {
            c.err(
                "product.license_key",
                "is not an ed25519 public key in 64 hex digits",
            );
        }
        k
    });
    let public_aut = p.public_aut.clone().unwrap_or_else(|| "*USE".into());
    if !matches!(
        public_aut.as_str(),
        "*ALL" | "*CHANGE" | "*EXCLUDE" | "*LIBCRTAUT" | "*USE"
    ) {
        c.err(
            "product.public_aut",
            format!("{public_aut:?} is not a public authority"),
        );
    }
    let lng_list = |c: &mut Ctx, at: &str, v: &[String]| -> Vec<LoadId> {
        v.iter()
            .enumerate()
            .filter_map(|(i, s)| {
                LoadId::nlv(s)
                    .map_err(|_| {
                        c.err(
                            format!("{at}[{i}]"),
                            format!("{s:?} is not a language feature (29xx)"),
                        );
                    })
                    .ok()
            })
            .collect()
    };
    let base_lngs = match &p.languages {
        Some(v) => lng_list(&mut c, "product.languages", v),
        None => vec![LoadId::nlv(LoadId::ENGLISH).expect("2924")],
    };
    if base_lngs.len() > 40 {
        c.err("product.languages", "more than 40 languages for *BASE");
    }

    if m.option.is_empty() {
        c.err("option", "a product needs at least its *BASE option");
    }
    if m.option.len() > 100 {
        c.err("option", "more than 100 options");
    }
    let mut options = Vec::new();
    let mut languages = Vec::new();
    let mut loads = Vec::new();
    let mut notes = Vec::new();
    for (i, o) in m.option.iter().enumerate() {
        let at = format!("option[{i}]");
        let ot = o.option.text();
        let Ok(option) = ot.parse::<OptionNo>() else {
            c.err(
                format!("{at}.option"),
                format!("{ot:?} is not *BASE or 1 to 99"),
            );
            continue;
        };
        if i == 0 && !option.is_base() {
            c.err(format!("{at}.option"), "the first option must be *BASE");
        }
        if options.iter().any(|d: &OptionDef| d.option == option) {
            c.err(
                format!("{at}.option"),
                format!("option {option} appears twice"),
            );
            continue;
        }
        c.text(&format!("{at}.text"), &o.text, 50);
        if !o.msgid.is_empty()
            && (o.msgid.len() != 7 || !o.msgid.bytes().all(|b| b.is_ascii_alphanumeric()))
        {
            c.err(
                format!("{at}.msgid"),
                "a message ID is seven letters and digits",
            );
        }
        let feature = match &o.feature {
            None => LoadId::feature(LoadId::CODE_DEFAULT).expect("5001"),
            Some(f) => LoadId::feature(f).unwrap_or_else(|_| {
                c.err(
                    format!("{at}.feature"),
                    format!("{f:?} is not a feature code 5001 to 9999"),
                );
                LoadId::feature(LoadId::CODE_DEFAULT).expect("5001")
            }),
        };
        let min_base = o
            .min_base
            .as_ref()
            .and_then(|s| c.release(&format!("{at}.min_base"), s));
        let mut requires = Vec::new();
        for (j, r) in o.requires.iter().enumerate() {
            match parse_prereq(r) {
                Some(p) => requires.push(p),
                None => c.err(
                    format!("{at}.requires[{j}]"),
                    format!("{r:?} is not PRODUCT:OPTION or PRODUCT:OPTION:VxRyMz"),
                ),
            }
        }
        let lngs = match &o.languages {
            Some(v) => {
                let l = lng_list(&mut c, &format!("{at}.languages"), v);
                for id in &l {
                    languages.push(LngLoad {
                        option,
                        load: LngSel::Nlv(id.clone()),
                    });
                }
                if l.is_empty() {
                    languages.push(LngLoad {
                        option,
                        load: LngSel::None,
                    });
                }
                l
            }
            None if option.is_base() => {
                for id in &base_lngs {
                    languages.push(LngLoad {
                        option,
                        load: LngSel::Nlv(id.clone()),
                    });
                }
                base_lngs.clone()
            }
            None => {
                languages.push(LngLoad {
                    option,
                    load: LngSel::BaseOpt,
                });
                base_lngs.clone()
            }
        };
        let mut libs = Vec::new();
        for (j, l) in o.libraries.iter().enumerate() {
            let la = format!("{at}.libraries[{j}]");
            c.name(&format!("{la}.name"), &l.name);
            let primary = l.primary.clone().unwrap_or_else(|| l.name.clone());
            c.name(&format!("{la}.primary"), &primary);
            if let Some(pe) = &l.post_exit {
                c.name(&format!("{la}.post_exit"), pe);
            }
            let lib_type = match l.lib_type.as_deref() {
                None | Some("*PROD") => LibType::Prod,
                Some("*TEST") => LibType::Test,
                Some(t) => {
                    c.err(format!("{la}.type"), format!("{t:?} is not *PROD or *TEST"));
                    LibType::Prod
                }
            };
            libs.push(LibEntry {
                dev: l.name.clone(),
                installed: primary.clone(),
                primary,
                post_exit: l.post_exit.clone(),
                lib_type,
                aut: l.aut.clone().unwrap_or_else(|| "*USE".into()),
                crt_aut: l.crt_aut.clone().unwrap_or_else(|| "*SYSVAL".into()),
            });
        }
        if libs.is_empty() {
            libs.push(LibEntry::new(&p.library));
        }
        let principal = libs[0].primary.clone();
        let pre_exits: Vec<QualName> = o
            .pre_exits
            .iter()
            .enumerate()
            .map(|(j, s)| {
                let mut q = c.qual(&format!("{at}.pre_exits[{j}]"), s);
                if q.lib == "*LIBL" {
                    q.lib.clone_from(&principal);
                }
                q
            })
            .collect();
        let objs = |c: &mut Ctx, field: &str, v: &[schema::Obj]| -> Vec<ObjEntry> {
            let mut out = Vec::new();
            for (j, ob) in v.iter().enumerate() {
                let oa = format!("{at}.{field}[{j}]");
                let lib = ob.lib.clone().unwrap_or_else(|| principal.clone());
                if !libs.iter().any(|l| l.primary == lib || l.dev == lib) {
                    c.err(
                        format!("{oa}.lib"),
                        format!("{lib} is not one of the option's libraries"),
                    );
                }
                if !special_or_name(&ob.name) {
                    c.err(
                        format!("{oa}.name"),
                        format!("{:?} is not a valid object name", ob.name),
                    );
                }
                let typ = ob.typ.to_ascii_uppercase();
                if !typ.starts_with('*') || typ.len() > 10 || typ.len() < 3 {
                    c.err(
                        format!("{oa}.type"),
                        format!("{:?} is not an object type", ob.typ),
                    );
                }
                out.push(ObjEntry {
                    lib,
                    name: ob.name.clone(),
                    typ,
                });
            }
            out
        };
        let objects = objs(&mut c, "objects", &o.objects);
        let lng_objects = objs(&mut c, "lng_objects", &o.lng_objects);
        if let Some(l) = &o.lng_library {
            c.name(&format!("{at}.lng_library"), l);
        }
        let mut directories = Vec::new();
        for (j, d) in o.directories.iter().enumerate() {
            let da = format!("{at}.directories[{j}]");
            if !d.path.starts_with('/') || d.path.contains("//") {
                c.err(format!("{da}.path"), "must be an absolute path");
            }
            let home = d
                .home
                .clone()
                .unwrap_or_else(|| d.path.rsplit('/').next().unwrap_or("").to_string());
            let data_aut = d.data_aut.clone().unwrap_or_else(|| "*RX".into());
            if !matches!(
                data_aut.as_str(),
                "*RWX" | "*RW" | "*RX" | "*WX" | "*R" | "*W" | "*X" | "*EXCLUDE" | "*NONE"
            ) {
                c.err(
                    format!("{da}.data_aut"),
                    format!("{data_aut:?} is not a data authority"),
                );
            }
            for a in &d.obj_auts {
                if !matches!(
                    a.as_str(),
                    "*OBJEXIST" | "*OBJMGT" | "*OBJALTER" | "*OBJREF" | "*NONE"
                ) {
                    c.err(
                        format!("{da}.obj_auts"),
                        format!("{a:?} is not an object authority"),
                    );
                }
            }
            directories.push(DirEntry {
                primary: d.path.clone(),
                home: home.clone(),
                installed: d.path.clone(),
                installed_home: home,
                data_aut,
                obj_auts: d.obj_auts.clone(),
            });
        }
        for (j, path) in o.paths.iter().enumerate() {
            let under = directories.iter().any(|d| {
                path.strip_prefix(d.primary.as_str())
                    .is_some_and(|r| r.starts_with('/'))
            });
            if !under {
                c.err(
                    format!("{at}.paths[{j}]"),
                    format!("{path} is not below one of the option's directories"),
                );
            }
        }
        let code_key = LoadKey {
            product: product.clone(),
            release: rel,
            option,
            load_type: LoadType::Code,
            load_id: feature.clone(),
        };
        let mut code = ProductLoad::new(code_key.clone(), &principal, &o.text);
        code.libraries.clone_from(&libs);
        code.pre_exits.clone_from(&pre_exits);
        code.directories.clone_from(&directories);
        code.objects = objects;
        code.paths.clone_from(&o.paths);
        code.public_aut.clone_from(&public_aut);
        match ProductLoad::default_name(&code_key) {
            Some(n) => loads.push((QualName::new(&n, &p.library), code)),
            None => c.err(at.clone(), "the product ID gives no valid load object name"),
        }
        for l in lngs {
            let key = LoadKey {
                load_type: LoadType::Lng,
                load_id: l,
                ..code_key.clone()
            };
            let mut lng = ProductLoad::new(key.clone(), &principal, &o.text);
            lng.secondary_lib.clone_from(&o.lng_library);
            lng.objects.clone_from(&lng_objects);
            lng.public_aut.clone_from(&public_aut);
            if let Some(n) = ProductLoad::default_name(&key) {
                loads.push((QualName::new(&n, &p.library), lng));
            }
        }
        notes.push((option, o.components.clone(), o.ibm.clone()));
        options.push(OptionDef {
            option,
            text: o.text.clone(),
            msg_id: o.msgid.clone(),
            dynamic_naming: o.dynamic_naming,
            feature,
            min_base,
            requires,
        });
    }
    if languages.len() > 139 {
        c.err("option", "more than 139 language loads");
    }

    let mut licenses = Vec::new();
    for (i, l) in m.license.iter().enumerate() {
        let at = format!("license[{i}]");
        let Ok(feature) = LoadId::feature(&l.feature) else {
            c.err(
                format!("{at}.feature"),
                format!("{:?} is not a feature code", l.feature),
            );
            continue;
        };
        if !options.iter().any(|o| o.feature == feature) {
            c.err(
                format!("{at}.feature"),
                format!("no option is in feature {feature}"),
            );
        }
        let term = match &l.term {
            None => LicTerm::version_of(rel),
            Some(t) => t.parse().unwrap_or_else(|_| {
                c.err(
                    format!("{at}.term"),
                    format!("{t:?} is not a license term (V1, V1R0, V1R0M0)"),
                );
                LicTerm::version_of(rel)
            }),
        };
        let usage_type = match &l.usage_type {
            None => UsageType::Concurrent,
            Some(s) => UsageType::parse(s).unwrap_or_else(|| {
                c.err(
                    format!("{at}.usage_type"),
                    format!("{s:?} is not *CONCURRENT, *REGISTERED or *PROCESSOR"),
                );
                UsageType::Concurrent
            }),
        };
        let compliance = match &l.compliance {
            None => Compliance::Warning,
            Some(s) => Compliance::parse(s).unwrap_or_else(|| {
                c.err(
                    format!("{at}.compliance"),
                    format!("{s:?} is not *OPRACTION, *WARNING or *KEYED"),
                );
                Compliance::Warning
            }),
        };
        if compliance == Compliance::Keyed && license_key.is_none() && !product.is_quartz() {
            c.err(
                format!("{at}.compliance"),
                "*KEYED needs product.license_key to check keys with",
            );
        }
        let usage_limit = match &l.usage_limit {
            None => None,
            Some(v) => match v.text().as_str() {
                "*NOMAX" => None,
                t => match t.parse::<u32>() {
                    Ok(n) if n <= 999_999 => Some(n),
                    _ => {
                        c.err(
                            format!("{at}.usage_limit"),
                            format!("{t:?} is not 0 to 999999 or *NOMAX"),
                        );
                        None
                    }
                },
            },
        };
        licenses.push(LicenseDef {
            feature,
            term,
            usage_type,
            compliance,
            usage_limit,
            grace_days: l.grace_days,
            threshold: l.threshold,
        });
    }

    let mut agreement = None;
    if let Some(a) = &m.agreement {
        match (&a.text, &a.document) {
            (Some(t), None) => {
                agreement = Some(t.clone());
                let path = agreement_path(&product, rel, LoadId::ENGLISH);
                for (_, l) in &mut loads {
                    if l.key.load_type == LoadType::Code && l.key.option.is_base() {
                        l.agreements.push(path.clone());
                    }
                }
            }
            (None, Some(d)) if d.starts_with('/') => {
                for (_, l) in &mut loads {
                    if l.key.load_type == LoadType::Code && l.key.option.is_base() {
                        l.agreements.push(d.clone());
                    }
                }
            }
            (None, Some(_)) => c.err("agreement.document", "must be an absolute path"),
            _ => c.err("agreement", "give either text or document"),
        }
    }

    if !c.errs.is_empty() {
        return Err(c.errs);
    }
    let definition = ProductDefinition {
        product,
        release: rel,
        ibm_alias,
        text: p.text.clone(),
        msgf,
        first_copyright: p.first_copyright,
        current_copyright: p.current_copyright,
        release_date,
        allow_multiple_releases: p.allow_multiple_releases,
        allow_mixed_releases: p.allow_mixed_releases,
        registration: Registration {
            kind,
            value: rv.clone(),
        },
        public_aut,
        options,
        languages,
        ccsids: p.ccsids.clone(),
        licensed: !licenses.is_empty(),
        license_key,
        licenses,
    };
    Ok(Package {
        definition_name: QualName::new(&obj_name, &p.library),
        definition,
        loads,
        agreement,
        status,
        notes,
    })
}

/// `PRODUCT:OPTION[:VxRyMz]`.
fn parse_prereq(s: &str) -> Option<Prereq> {
    let mut it = s.split(':');
    let product = it.next()?.parse().ok()?;
    let option = it.next()?.parse().ok()?;
    let min_release = match it.next() {
        Some(r) => Some(id::parse_release(r).ok()?),
        None => None,
    };
    if it.next().is_some() {
        return None;
    }
    Some(Prereq {
        product,
        option,
        min_release,
    })
}

/// 64 hex digits that make an ed25519 public key.
fn parse_key(h: &str) -> Option<[u8; 32]> {
    if h.len() != 64 {
        return None;
    }
    let mut k = [0u8; 32];
    for (i, b) in k.iter_mut().enumerate() {
        *b = u8::from_str_radix(h.get(2 * i..2 * i + 2)?, 16).ok()?;
    }
    ed25519_dalek::VerifyingKey::from_bytes(&k).ok()?;
    Some(k)
}
