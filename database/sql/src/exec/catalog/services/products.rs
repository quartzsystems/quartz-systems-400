// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `SOFTWARE_PRODUCT_INFO` and `PTF_INFO`: the installed product loads
//! (`DSPSFWRSC`) and the PTF database (`DSPPTF`), as the licensed
//! program and PTF services keep them.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Out, ask};
use crate::error::{Result, SqlError};

/// `SOFTWARE_PRODUCT_INFO`'s `NOT NULL` columns with no value here:
/// QS/400 products carry no software registration.
pub const PRODUCT_NO_VALUE: &[&str] = &["REGISTRATION_TYPE", "REGISTRATION_VALUE"];

fn failed(e: String) -> SqlError {
    SqlError::Coded { code: -443, state: "38501", what: e }
}

/// `SOFTWARE_PRODUCT_INFO`.
pub fn products(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let list = ask(txn, ctx, "the licensed programs", |h| h.products())?.map_err(failed)?;
    for p in &list {
        out.row();
        out.text("PRODUCT_ID", p.id.as_str());
        out.text("PRODUCT_OPTION", p.option.as_str());
        out.text("LOAD_ID", p.load_id.as_str());
        out.text("LOAD_TYPE", if p.code { "CODE" } else { "LANGUAGE" });
        out.text("RELEASE_LEVEL", p.release.as_str());
        out.yes("INSTALLED", true);
        out.text("SYMBOLIC_LOAD_STATE", "INSTALLED");
        out.yes("LOAD_ERROR", false);
        out.text("LOAD_STATE", "90");
        out.yes("SUPPORTED", true);
        out.yes("COMPATIBLE", true);
        out.int("PRODUCT_LIBRARY_COUNT", usize::from(!p.library.is_empty()));
        out.text_or_null("PRODUCT_LIBRARIES", &p.library);
        out.int("PRODUCT_DIRECTORY_COUNT", 0);
        out.text_or_null("TEXT_DESCRIPTION", &p.text);
        out.text("MINIMUM_TARGET_RELEASE", p.release.as_str());
        out.text("MINIMUM_VRM_BASE", "*MATCH");
        out.int("REQUIREMENTS_MET", 1);
        out.yes("MIXED_RELEASES", false);
    }
    Ok(())
}

/// `PTF_INFO`.
pub fn ptfs(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let list = ask(txn, ctx, "the PTF database", |h| h.ptfs())?.map_err(failed)?;
    for p in &list {
        out.row();
        out.text("PTF_PRODUCT_ID", p.product.as_str());
        out.text("PTF_PRODUCT_OPTION", p.option.as_str());
        out.text("PTF_PRODUCT_RELEASE_LEVEL", p.release.as_str());
        out.text("PTF_IDENTIFIER", p.id.as_str());
        out.text("PTF_RELEASE_LEVEL", p.release.as_str());
        out.text("PTF_LOADED_STATUS", p.status.as_str());
        out.yes("PTF_SAVE_FILE", p.savf);
        out.yes("PTF_COVER_LETTER", false);
        out.yes("PTF_ON_ORDER", p.status == "ON ORDER");
        out.text("PTF_IPL_ACTION", p.ipl_action.as_str());
        out.yes("PTF_ACTION_PENDING", p.ipl_action != "NONE");
        out.text("PTF_IPL_REQUIRED", if p.delayed { "DELAYED" } else { "IMMEDIATE" });
        out.yes("PTF_IS_RELEASED", true);
        out.stamp("PTF_STATUS_TIMESTAMP", p.changed);
        out.text_or_null("PTF_SUPERSEDED_BY_PTF", &p.superseded_by);
        out.yes("PTF_TECHNOLOGY_REFRESH_PTF", false);
    }
    Ok(())
}
