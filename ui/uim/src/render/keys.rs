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

/// The rows the key area keeps: two when the keys with a condition,
/// shown or not, would need them (dspusrprf-objaut keeps row 23 blank
/// under one row of keys because F11 has two texts); else the rows
/// shown.
pub(super) fn reserved(keyl: &KeyList, limit: usize, shown: usize) -> usize {
    if keyl.keys.iter().any(|k| k.action == Action::MoreKeys)
        || !keyl.keys.iter().any(|k| k.cond.is_some())
    {
        return shown;
    }
    let widths: Vec<usize> = keyl
        .keys
        .iter()
        .filter(|k| !k.text.trim().is_empty())
        .map(|k| bare(&k.text).chars().count())
        .collect();
    layout::flow(&widths, 2, limit).len().clamp(shown, 2)
}

/// A key's text without the blanks around it; a non-breaking blank at
/// its end stays, holding the next key at the column the system shows.
fn bare(text: &str) -> &str {
    text.trim_matches(|c: char| c.is_whitespace() && c != '\u{a0}')
}

/// The texts of the set to show: the key list is cut into sets at
/// each `F24=More keys`; F24 moves to the next set and comes back to
/// the first after the last. Each set ends with the key.
fn set(ctx: &Ctx, keyl: &KeyList) -> Vec<String> {
    let visible: Vec<(&Action, String)> = keyl
        .keys
        .iter()
        .filter(|k| !k.text.trim().is_empty() && ctx.cond(k.cond.as_deref()))
        .map(|k| (&k.action, bare(&k.text).to_string()))
        .collect();
    let Some(first) = visible.iter().position(|(a, _)| **a == Action::MoreKeys) else {
        return visible.into_iter().map(|(_, t)| t).collect();
    };
    let more = visible[first].1.clone();
    let mut sets: Vec<Vec<String>> = vec![Vec::new()];
    for (a, t) in &visible {
        if **a == Action::MoreKeys {
            sets.push(Vec::new());
        } else {
            sets.last_mut().expect("a set").push(t.clone());
        }
    }
    sets.retain(|s| !s.is_empty());
    if sets.is_empty() {
        return vec![more];
    }
    let pressed = if ctx.opts.key_set > 0 {
        ctx.opts.key_set
    } else {
        usize::from(ctx.opts.more_keys)
    };
    let mut chosen = sets[pressed % sets.len()].clone();
    chosen.push(more);
    chosen
}
