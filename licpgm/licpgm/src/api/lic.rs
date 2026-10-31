// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! QLZAREQ (Request License) and QLZARLS (Release License) over their
//! parameter bytes: product identification `LICP0100`, license user
//! `LICL0100` or `LICL0200`.

use release::Release;

use crate::api::{ApiError, bin4_at, char_at};
use crate::id::{self, LoadId, ProductId};
use crate::license::repo::LicenseRepo;
use crate::license::usage::{Notice, Request};
use crate::model::Date;
use crate::model::definition::ProductDefinition;
use crate::system::SystemInfo;

/// A product, release and feature read from `LICP0100`.
fn licp0100(
    b: &[u8],
    fmt: &str,
    defs: &[ProductDefinition],
) -> Result<(ProductId, Release, LoadId), ApiError> {
    if fmt != "LICP0100" {
        return Err(ApiError::new("CPF3C21").text(fmt, 8));
    }
    if b.len() < 17 {
        return Err(ApiError::new("CPF24B4"));
    }
    let pid = char_at(b, 0, 7);
    let product: ProductId = pid
        .parse()
        .map_err(|_| ApiError::new("CPF9E11").text(&pid, 7))?;
    let rs = char_at(b, 7, 6);
    let release = if rs == "*ONLY" {
        let rels: Vec<Release> = defs
            .iter()
            .filter(|d| d.product == product)
            .map(|d| d.release)
            .collect();
        match rels.as_slice() {
            [r] => *r,
            [] => return Err(ApiError::new("CPF9E12").text(&pid, 7)),
            _ => return Err(ApiError::new("CPF9E13").text(&pid, 7)),
        }
    } else {
        id::parse_release(&rs).map_err(|_| ApiError::new("CPF9E11").text(&rs, 6))?
    };
    let fs = char_at(b, 13, 4);
    let feature = LoadId::feature(&fs).map_err(|_| ApiError::new("CPF9E11").text(&fs, 4))?;
    Ok((product, release, feature))
}

/// How many bytes of a `LICL0200` user parameter to read, from its
/// first 28.
#[must_use]
pub fn licl0200_extent(head: &[u8]) -> usize {
    let end = |o: usize, l: usize| {
        let off = usize::try_from(bin4_at(head, o)).unwrap_or(0);
        let len = usize::try_from(bin4_at(head, l)).unwrap_or(0);
        off + len
    };
    end(0, 4).max(end(16, 20)).max(28)
}

/// The user name, handle and number of uses.
fn user(b: &[u8], fmt: &str, job: &str) -> Result<(String, [u8; 8], u32), ApiError> {
    let (name, handle, uses) = match fmt {
        "LICL0100" => {
            if b.len() < 10 {
                return Err(ApiError::new("CPF24B4"));
            }
            (char_at(b, 0, 10), [ccsid::SPACE; 8], 1)
        }
        "LICL0200" => {
            if b.len() < 28 {
                return Err(ApiError::new("CPF24B4"));
            }
            let off = usize::try_from(bin4_at(b, 0)).map_err(|_| ApiError::new("CPF9E1C"))?;
            let len = usize::try_from(bin4_at(b, 4)).map_err(|_| ApiError::new("CPF9E1E"))?;
            if !(1..=80).contains(&len) {
                return Err(ApiError::new("CPF9E1E"));
            }
            if b[24..28] != [0; 4] {
                return Err(ApiError::new("CPF9E1C"));
            }
            let name = b
                .get(off..off + len)
                .map(ccsid::decode_trimmed)
                .ok_or_else(|| ApiError::new("CPF9E1C"))?;
            let handle: [u8; 8] = b[8..16].try_into().expect("8 bytes");
            let aoff = usize::try_from(bin4_at(b, 16)).unwrap_or(0);
            let alen = usize::try_from(bin4_at(b, 20)).unwrap_or(0);
            let uses = if alen >= 4 {
                u32::try_from(bin4_at(b, aoff)).map_err(|_| ApiError::new("CPF9E1C"))?
            } else {
                1
            };
            (name, handle, uses)
        }
        f => return Err(ApiError::new("CPF3C21").text(f, 8)),
    };
    if name.is_empty() {
        return Err(ApiError::new("CPF9E91"));
    }
    let name = if name == "*JOB" {
        job.to_string()
    } else {
        name
    };
    Ok((name, handle, uses))
}

fn lic_error(e: &crate::license::LicError) -> ApiError {
    ApiError::new(e.msg_id())
}

/// QLZAREQ: grant uses; the notices are for the caller to send.
pub fn qlzareq(
    repo: &mut LicenseRepo,
    defs: &[ProductDefinition],
    sys: &SystemInfo,
    today: Date,
    product: &[u8],
    product_fmt: &str,
    lic_user: &[u8],
    user_fmt: &str,
    job: &str,
) -> Result<Vec<Notice>, ApiError> {
    let (product, release, feature) = licp0100(product, product_fmt, defs)?;
    let (user, handle, uses) = user(lic_user, user_fmt, job)?;
    let req = Request {
        product,
        release,
        feature,
        user,
        handle,
        uses,
    };
    repo.request(&req, sys, today).map_err(|e| lic_error(&e))
}

/// QLZARLS: release a user's uses.
pub fn qlzarls(
    repo: &mut LicenseRepo,
    defs: &[ProductDefinition],
    product: &[u8],
    product_fmt: &str,
    lic_user: &[u8],
    user_fmt: &str,
    job: &str,
) -> Result<(), ApiError> {
    let (product, release, feature) = licp0100(product, product_fmt, defs)?;
    let (user, handle, _) = user(lic_user, user_fmt, job)?;
    repo.release(&product, release, &feature, &user, handle)
        .map_err(|e| lic_error(&e))
}
