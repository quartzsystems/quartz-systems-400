// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZCRTPD (Create Product Definition) and QSZDLTPD (Delete Product
//! Definition).

use crate::api::{ApiError, blank_at, char_at, yes_no};
use crate::id::{self, LoadId, OptionNo, ProductId};
use crate::model::definition::{
    LngLoad, LngSel, OptionDef, ProductDefinition, RegType, Registration,
};
use crate::model::{Date, QualName};
use crate::store::{ProductStore, StoreError, find_definition};

/// The store's failure as an API message.
pub(crate) fn store_error(e: &StoreError) -> ApiError {
    match e {
        StoreError::Exists(q) => ApiError::new("CPF0C16").text(&q.name, 10).text(&q.lib, 10),
        StoreError::NotFound(q) => ApiError::new("CPF2105").text(&q.name, 10).text(&q.lib, 10),
        StoreError::NoLibrary(l) => ApiError::new("CPF9810").text(l, 10),
        StoreError::Other(s) => ApiError::new("CPF0C4D").text(s, 50),
    }
}

/// The qualified name of a `Char(20)` parameter, `*CURLIB` resolved.
pub(crate) fn qual20(b: &[u8], cur_lib: &str) -> Result<QualName, ApiError> {
    let name = char_at(b, 0, 10);
    let mut lib = char_at(b, 10, 10);
    if lib == "*CURLIB" {
        lib = cur_lib.to_string();
    }
    if !id::is_name(&name) {
        return Err(ApiError::new("CPF3C29").text(&name, 10));
    }
    if !id::is_name(&lib) {
        return Err(ApiError::new("CPF9810").text(&lib, 10));
    }
    Ok(QualName::new(&name, &lib))
}

/// Check a definition the way QSZCRTPD does and create it.
pub fn create_definition(
    store: &mut dyn ProductStore,
    at: &QualName,
    d: &ProductDefinition,
) -> Result<(), ApiError> {
    if d.options.is_empty() || d.options.len() > 100 {
        return Err(ApiError::new("CPF0CBA"));
    }
    if !d.options[0].option.is_base() {
        return Err(ApiError::new("CPF0CAA"));
    }
    for (i, o) in d.options.iter().enumerate() {
        if d.options[..i].iter().any(|x| x.option == o.option) {
            return Err(ApiError::new("CPF0CAF").text(&o.option.api(), 4));
        }
    }
    if d.languages.is_empty() || d.languages.len() > 139 {
        return Err(ApiError::new("CPF0CB9"));
    }
    for (i, l) in d.languages.iter().enumerate() {
        if d.option(l.option).is_none() {
            return Err(ApiError::new("CPF0CAE").text(&l.option.api(), 4));
        }
        let wrong = match l.load {
            LngSel::IbmLng => !l.option.is_base(),
            LngSel::BaseOpt => l.option.is_base(),
            _ => false,
        };
        if wrong {
            return Err(ApiError::new("CPF0CAB").text(l.load.text(), 8));
        }
        if d.languages[..i].iter().any(|x| x == l) {
            return Err(ApiError::new("CPF0CAD").text(l.load.text(), 8));
        }
    }
    let base_lngs = d.languages.iter().filter(|l| l.option.is_base()).count();
    if base_lngs > 40 {
        return Err(ApiError::new("CPF0CB9"));
    }
    if let (Some(a), Some(b)) = (d.first_copyright, d.current_copyright)
        && a > b
    {
        return Err(ApiError::new("CPF0CB0"));
    }
    if find_definition(&*store, &d.product, d.release).is_some() {
        return Err(ApiError::new("CPF0C17")
            .text(d.product.as_str(), 7)
            .text(&id::release_text(d.release), 6));
    }
    if !store.library_exists(&at.lib) {
        return Err(ApiError::new("CPF9810").text(&at.lib, 10));
    }
    store.create_definition(at, d).map_err(|e| store_error(&e))
}

fn copyright(s: &str) -> Result<Option<u16>, ApiError> {
    match s {
        "*NONE" => Ok(None),
        y if y.len() == 4 => y.parse().map(Some).map_err(|_| ApiError::new("CPF0CB5")),
        _ => Err(ApiError::new("CPF0CB5")),
    }
}

/// QSZCRTPD over its parameter bytes. `today` resolves `*CURRENT`
/// copyright years; `cur_lib` resolves `*CURLIB`.
pub fn qszcrtpd(
    store: &mut dyn ProductStore,
    qual: &[u8],
    info: &[u8],
    options: &[u8],
    n_options: i32,
    lngs: &[u8],
    n_lngs: i32,
    text: &[u8],
    public_aut: &[u8],
    cur_lib: &str,
    today: Date,
) -> Result<(), ApiError> {
    let at = qual20(qual, cur_lib)?;
    if info.len() < 106 {
        return Err(ApiError::new("CPF24B4"));
    }
    let pid = char_at(info, 0, 7);
    let product: ProductId = pid
        .parse()
        .map_err(|_| ApiError::new("CPF0CB2").text(&pid, 7))?;
    let rs = char_at(info, 7, 6);
    let release = id::parse_release(&rs).map_err(|_| ApiError::new("CPF358A").text(&rs, 6))?;
    let msgf_name = char_at(info, 13, 10);
    let cur = |s: String| {
        if s == "*CURRENT" {
            today.year.to_string()
        } else {
            s
        }
    };
    let first_copyright = copyright(&cur(char_at(info, 23, 10)))?;
    let current_copyright = copyright(&cur(char_at(info, 33, 10)))?;
    let rd = char_at(info, 43, 6);
    let century = char_at(info, 77, 1);
    let release_date = if rd == "*NONE" {
        None
    } else {
        if rd.len() != 6 || !rd.bytes().all(|b| b.is_ascii_digit()) {
            return Err(ApiError::new("CPF0CBB"));
        }
        let yy: u16 = rd[..2].parse().unwrap_or(0);
        let c = match century.as_str() {
            "0" => 1900,
            "1" => 2000,
            "" if yy >= 40 => 1900,
            "" => 2000,
            _ => return Err(ApiError::new("CPF0CBE")),
        };
        Some(
            Date::new(
                c + yy,
                rd[2..4].parse().unwrap_or(0),
                rd[4..].parse().unwrap_or(0),
            )
            .ok_or_else(|| ApiError::new("CPF0CBB"))?,
        )
    };
    let multi = yes_no(&char_at(info, 49, 4), "CPF0CB6")?;
    let rt = char_at(info, 53, 10);
    let kind = match rt.as_str() {
        "*PHONE" => RegType::Phone,
        "*CUSTOMER" => RegType::Customer,
        _ => return Err(ApiError::new("CPF0CB1")),
    };
    let value = char_at(info, 63, 14);
    if !value
        .bytes()
        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(ApiError::new("CPF0CB1"));
    }
    if !blank_at(info, 78, 28) {
        return Err(ApiError::new("CPF0CB3"));
    }
    let n = usize::try_from(n_options).map_err(|_| ApiError::new("CPF0CBA"))?;
    if n == 0 || n > 100 || options.len() < n * 41 {
        return Err(ApiError::new("CPF0CBA"));
    }
    let mut opts = Vec::new();
    for i in 0..n {
        let e = &options[i * 41..(i + 1) * 41];
        let os = char_at(e, 0, 4);
        let option: OptionNo = os
            .parse()
            .map_err(|_| ApiError::new("CPF0C8A").text(&os, 4))?;
        let msg_id = char_at(e, 4, 7);
        let dynamic_naming = match char_at(e, 11, 10).as_str() {
            "*NODYNNAM" => false,
            "*ALWDYNNAM" => true,
            _ => return Err(ApiError::new("CPF0CB7")),
        };
        let fs = char_at(e, 21, 4);
        let feature = LoadId::feature(&fs).map_err(|_| ApiError::new("CPF0CBD").text(&fs, 4))?;
        if !blank_at(e, 25, 16) {
            return Err(ApiError::new("CPF0CB3"));
        }
        opts.push(OptionDef {
            option,
            text: String::new(),
            msg_id,
            dynamic_naming,
            feature,
            min_base: None,
            requires: Vec::new(),
        });
    }
    let nl = usize::try_from(n_lngs).map_err(|_| ApiError::new("CPF0CB9"))?;
    if nl == 0 || nl > 139 || lngs.len() < nl * 20 {
        return Err(ApiError::new("CPF0CB9"));
    }
    let mut languages = Vec::new();
    for i in 0..nl {
        let e = &lngs[i * 20..(i + 1) * 20];
        let ls = char_at(e, 0, 8);
        let load = LngSel::parse(&ls).map_err(|_| ApiError::new("CPF0CAB").text(&ls, 8))?;
        let os = char_at(e, 8, 4);
        let option: OptionNo = os
            .parse()
            .map_err(|_| ApiError::new("CPF0CB8").text(&os, 4))?;
        languages.push(LngLoad { option, load });
    }
    let aut = char_at(public_aut, 0, 10);
    if !matches!(
        aut.as_str(),
        "*ALL" | "*CHANGE" | "*EXCLUDE" | "*LIBCRTAUT" | "*USE"
    ) {
        return Err(ApiError::new("CPF0C9B").text(&aut, 10));
    }
    let d = ProductDefinition {
        product,
        release,
        ibm_alias: None,
        text: char_at(text, 0, 50),
        msgf: (!msgf_name.is_empty()).then(|| QualName::new(&msgf_name, "*LIBL")),
        first_copyright,
        current_copyright,
        release_date,
        allow_multiple_releases: multi,
        allow_mixed_releases: false,
        registration: Registration { kind, value },
        public_aut: aut,
        options: opts,
        languages,
        ccsids: Vec::new(),
        licensed: false,
        license_key: None,
        licenses: Vec::new(),
    };
    create_definition(store, &at, &d)
}

/// QSZDLTPD: delete a product definition. One whose product still has
/// load objects at its release is refused with `CPF0C52`.
pub fn qszdltpd(store: &mut dyn ProductStore, qual: &[u8]) -> Result<(), ApiError> {
    let at = qual20(qual, "")?;
    let Some((_, d)) = store.definitions().into_iter().find(|(q, _)| q == &at) else {
        return Err(ApiError::new("CPF2105")
            .text(&at.name, 10)
            .text(&at.lib, 10));
    };
    let has_loads = store
        .loads()
        .iter()
        .any(|(_, l)| l.key.product == d.product && l.key.release == d.release);
    if has_loads {
        return Err(ApiError::new("CPF0C52")
            .text(&at.name, 10)
            .text(&at.lib, 10));
    }
    store.delete_definition(&at).map_err(|e| store_error(&e))
}
