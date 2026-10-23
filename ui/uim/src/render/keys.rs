// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The function-key area (R6, R7): the key list's texts, conditions
//! applied, split into sets at `MOREKEYS`, flowed over at most two rows.

use super::ctx::Ctx;
use super::layout;
use crate::model::{Action, KeyList};

/// The key rows as `(column, text)` items, bottom row last.
pub(super) fn rows(ctx: &Ctx, keyl: &KeyList, limit: usize) -> Vec<Vec<(usize, String)>> {
    let texts = set(ctx, keyl);
    let widths: Vec<usize> = texts.iter().map(|t| t.chars().count()).collect();
    let mut out: Vec<Vec<(usize, String)>> = layout::flow(&widths, 2, limit)
        .into_iter()
        .map(|row| {
            row.into_iter()
                .map(|(i, col)| (col, texts[i].clone()))
                .collect()
        })
        .collect();
    out.truncate(2);
    out
}

/// The texts of the set to show: the keys before `F24=More keys` with
/// it, or the keys after it with it.
fn set(ctx: &Ctx, keyl: &KeyList) -> Vec<String> {
    let visible: Vec<(&Action, String)> = keyl
        .keys
        .iter()
        .filter(|k| !k.text.trim().is_empty() && ctx.cond(k.cond.as_deref()))
        .map(|k| (&k.action, k.text.trim().to_string()))
        .collect();
    let Some(split) = visible.iter().position(|(a, _)| **a == Action::MoreKeys) else {
        return visible.into_iter().map(|(_, t)| t).collect();
    };
    let more = visible[split].1.clone();
    let after: Vec<String> = visible[split + 1..]
        .iter()
        .map(|(_, t)| t.clone())
        .collect();
    let mut chosen: Vec<String> = if ctx.opts.more_keys && !after.is_empty() {
        after
    } else {
        visible[..split].iter().map(|(_, t)| t.clone()).collect()
    };
    chosen.push(more);
    chosen
}
