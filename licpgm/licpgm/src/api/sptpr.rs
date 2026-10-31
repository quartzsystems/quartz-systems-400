// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QSZSPTPR (Add or Remove Product Support): the supported flag of a
//! load and its registration.

use crate::api::{ApiError, char_at};
use crate::id::{self, LoadId, LoadType, OptionNo, ProductId};
use crate::model::definition::{RegType, Registration};
use crate::store::{ProductStore, find_definition};

/// QSZSPTPR: product information (`SPTP0100`), its length, the format
/// name and the action (1 add support, 0 remove it).
pub fn qszsptpr(
    store: &mut dyn ProductStore,
    info: &[u8],
    len: i32,
    format: &str,
    action: i32,
) -> Result<(), ApiError> {
    if format != "SPTP0100" {
        return Err(ApiError::new("CPF3C21").text(format, 8));
    }
    let n = usize::try_from(len).unwrap_or(0);
    if n < 53 || info.len() < 53 {
        return Err(ApiError::new("CPF3C1D"));
    }
    let add = match action {
        1 => true,
        0 => false,
        _ => return Err(ApiError::new("CPF3C1D")),
    };
    let pid = char_at(info, 0, 7);
    let product: ProductId = pid
        .parse()
        .map_err(|_| ApiError::new("CPF0C1F").text(&pid, 7))?;
    let rs = char_at(info, 7, 6);
    let release = id::parse_release(&rs).map_err(|_| ApiError::new("CPF0C1D").text(&rs, 6))?;
    let os = char_at(info, 13, 4);
    let option: OptionNo = os
        .parse()
        .map_err(|_| ApiError::new("CPF0C1F").text(&os, 4))?;
    let ls = char_at(info, 17, 10);
    let rt = char_at(info, 37, 2);
    let rv = char_at(info, 39, 14);
    let (_, def) = find_definition(&*store, &product, release)
        .ok_or_else(|| ApiError::new("CPF0C1F").text(&pid, 7))?;
    let (load_type, load_id) = if ls == "*CODE" {
        let f = def
            .option(option)
            .ok_or_else(|| ApiError::new("CPF0C1F").text(&os, 4))?;
        (LoadType::Code, f.feature.clone())
    } else {
        LoadId::feature(&ls)
            .map(|f| (LoadType::Code, f))
            .or_else(|_| LoadId::nlv(&ls).map(|n| (LoadType::Lng, n)))
            .map_err(|_| ApiError::new("CPF0C1F").text(&ls, 10))?
    };
    let (q, mut l) = store
        .loads()
        .into_iter()
        .find(|(_, l)| {
            l.key.product == product
                && l.key.release == release
                && l.key.option == option
                && l.key.load_type == load_type
                && l.key.load_id == load_id
        })
        .ok_or_else(|| {
            ApiError::new("CPF0C1F")
                .text(&pid, 7)
                .text(&rs, 6)
                .text(&os, 4)
        })?;
    l.supported = add;
    if add && !rt.is_empty() {
        let kind = RegType::from_code(&rt).ok_or_else(|| ApiError::new("CPF0C86"))?;
        l.registration = Some(Registration {
            kind,
            value: if kind == RegType::OsSame {
                String::new()
            } else {
                rv
            },
        });
    }
    store
        .update_load(&q, &l)
        .map_err(|e| crate::api::crtpd::store_error(&e))
}
