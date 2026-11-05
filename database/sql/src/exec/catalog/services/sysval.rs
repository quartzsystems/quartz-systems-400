// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `QSYS2.SYSTEM_VALUE_INFO`: every system value the catalog has, its
//! current value as a number or as text, our description, its group.

use store::txn::Txn;

use super::super::super::SqlContext;
use super::{Out, ask};
use crate::error::Result;

/// The rows.
pub fn rows(txn: &mut Txn<'_>, ctx: &SqlContext, out: &mut Out) -> Result<()> {
    let mut vals = ask(txn, ctx, "the system values", |h| h.system_values())?;
    vals.sort_by(|a, b| a.name.cmp(&b.name));
    for v in vals {
        out.row();
        out.text("SYSTEM_VALUE_NAME", v.name.as_str());
        let current = match (&v.number, &v.text) {
            (Some(n), _) => n.to_string(),
            (None, Some(t)) => t.clone(),
            (None, None) => String::new(),
        };
        out.text("SYSTEM_VALUE", current);
        if let Some(n) = v.number {
            out.int("CURRENT_NUMERIC_VALUE", n);
        }
        if let Some(t) = &v.text {
            out.text("CURRENT_CHARACTER_VALUE", t.as_str());
        }
        out.text("TEXT_DESCRIPTION", v.description.as_str());
        out.text("CATEGORY", v.group.as_str());
        out.yes("CHANGEABLE", v.changeable);
        out.text("SHIPPED_DEFAULT_VALUE", v.shipped.as_str());
    }
    Ok(())
}
