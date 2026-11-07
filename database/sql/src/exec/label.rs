// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `LABEL ON` and `COMMENT ON`. A label is the object's text (50
//! characters), a column's heading (60, three lines of 20) or text (50);
//! a comment is the long comment (2000) the catalogs show as
//! `LONG_COMMENT`, kept in the object's `*SQLINF` (`COMMENT` entries).
//! The limits and codes are the primary's (`SQL0107`, `SQL0205`,
//! `SQL0159`).

use db::file;
use db::sqlinf::{self, Kind, SqlInfo, comment};
use qsname::Name;
use store::obj::{ObjectId, ops};
use store::txn::Txn;

use super::SqlContext;
use crate::ast::{Ident, Label, LabelTarget};
use crate::error::{Result, SqlError};

/// The longest object text and column text.
const TEXT_MAX: usize = 50;
/// The longest column heading.
const HEADING_MAX: usize = 60;
/// The longest long comment.
const COMMENT_MAX: usize = 2000;

/// What one text is given as.
#[derive(Clone, Copy, PartialEq, Eq)]
enum As {
    Heading,
    Text,
    Comment,
}

/// Refuse a text over `max` characters (`SQL0107`).
fn check_len(text: &str, max: usize) -> Result<()> {
    if text.chars().count() > max {
        return Err(SqlError::NameTooLong {
            name: text.into(),
            max,
        });
    }
    Ok(())
}

/// `LABEL ON` (`is_comment` false) or `COMMENT ON` (true).
pub fn label(txn: &mut Txn<'_>, ctx: &mut SqlContext, l: &Label, is_comment: bool) -> Result<()> {
    let (name, column) = match &l.target {
        LabelTarget::Table(n) | LabelTarget::Index(n) | LabelTarget::Alias(n) => (n, None),
        LabelTarget::Column(n, c) => (n, Some(c)),
        LabelTarget::Trigger(n) => return super::trigger::label(txn, ctx, n, l),
        LabelTarget::Sequence(n) => {
            return sequence(txn, ctx, n, l.text.as_deref().unwrap_or(""), is_comment);
        }
    };
    let id = ctx.resolve_file(txn, name)?;
    super::qtemp::refuse_declared(ctx, id, "LABEL ON or COMMENT ON")?;
    check_kind(txn, id, &l.target, name)?;
    db::auth::check(txn, "file.change", id)?;
    let now = ctx.clock();
    if column.is_none() && l.columns.is_empty() {
        let text = l.text.clone().unwrap_or_default();
        if is_comment {
            check_len(&text, COMMENT_MAX)?;
            return set_comment(txn, id, None, &text, now);
        }
        check_len(&text, TEXT_MAX)?;
        ops::change_text(txn, id, &text, now)?;
        return Ok(());
    }
    let how = |is_text: bool| match (is_comment, is_text) {
        (true, _) => As::Comment,
        (false, true) => As::Text,
        (false, false) => As::Heading,
    };
    let mut todo: Vec<(&Ident, &str, As)> = Vec::new();
    if let Some(c) = column {
        todo.push((c, l.text.as_deref().unwrap_or(""), how(l.is_text)));
    }
    for (c, t, is_text) in &l.columns {
        todo.push((c, t, how(*is_text)));
    }
    let desc = file::describe(txn, id)?;
    let mut formats = desc.formats.clone();
    let mut comments: Vec<(Name, &str)> = Vec::new();
    let mut fields_changed = false;
    for (c, text, how) in todo {
        check_len(
            text,
            match how {
                As::Heading => HEADING_MAX,
                As::Text => TEXT_MAX,
                As::Comment => COMMENT_MAX,
            },
        )?;
        // The column by its system name or its SQL name (`SQL0205`).
        let Some(f) = formats
            .iter_mut()
            .flat_map(|fm| fm.format.fields.iter_mut())
            .find(|f| {
                f.name.to_string() == c.value || f.alias.as_deref() == Some(c.value.as_str())
            })
        else {
            return Err(SqlError::Coded {
                code: -205,
                state: "42703",
                what: format!("column {} is not in table {}", c.value, name.name.value),
            });
        };
        fields_changed |= how != As::Comment;
        match how {
            As::Comment => comments.push((f.name, text)),
            As::Text => f.text = Some(text.to_string()).filter(|t| !t.trim().is_empty()),
            // A heading is three lines of twenty characters.
            As::Heading => {
                let chars: Vec<char> = text.chars().collect();
                f.colhdg = chars
                    .chunks(20)
                    .map(|ch| ch.iter().collect::<String>().trim_end().to_string())
                    .collect();
                while f.colhdg.last().is_some_and(String::is_empty) {
                    f.colhdg.pop();
                }
            }
        }
    }
    if fields_changed {
        file::replace_formats(txn, id, &formats)?;
    }
    for (column, text) in comments {
        set_comment(txn, id, Some(column), text, now)?;
    }
    Ok(())
}

/// `LABEL ON INDEX` names an index, `TABLE` a table or view, `ALIAS` an
/// alias (`SQL0159`).
fn check_kind(
    txn: &mut Txn<'_>,
    id: ObjectId,
    target: &LabelTarget,
    name: &crate::ast::ObjectName,
) -> Result<()> {
    let kind = super::names::kind_of(txn, id);
    let ok = match target {
        LabelTarget::Index(_) => kind == Some(Kind::Index),
        LabelTarget::Alias(_) => kind == Some(Kind::Alias),
        LabelTarget::Table(_) => !matches!(kind, Some(Kind::Index | Kind::Alias)),
        _ => true,
    };
    if ok {
        return Ok(());
    }
    Err(SqlError::Coded {
        code: -159,
        state: "42809",
        what: format!("{} is not of the type the statement names", name.name.value),
    })
}

/// Keep `text` as the long comment of `column` (the object's when
/// `None`) of the object `id`; an empty one drops it.
pub(crate) fn set_comment(
    txn: &mut Txn<'_>,
    id: ObjectId,
    column: Option<Name>,
    text: &str,
    now: u64,
) -> Result<()> {
    let mut info = sqlinf::load(txn, id)?.unwrap_or_else(|| SqlInfo::new(Kind::File));
    comment::set(
        &mut info,
        column,
        Some(text.to_string()).filter(|t| !t.is_empty()),
    );
    sqlinf::save(txn, id, &info, now)?;
    Ok(())
}

/// `LABEL ON SEQUENCE` (the `*DTAARA`'s text) and `COMMENT ON SEQUENCE`.
fn sequence(
    txn: &mut Txn<'_>,
    ctx: &mut SqlContext,
    name: &crate::ast::ObjectName,
    text: &str,
    is_comment: bool,
) -> Result<()> {
    let (id, _) = super::seq::resolve(txn, ctx, name)?;
    let now = ctx.clock();
    if is_comment {
        check_len(text, COMMENT_MAX)?;
        return set_comment(txn, id, None, text, now);
    }
    check_len(text, TEXT_MAX)?;
    ops::change_text(txn, id, text, now)?;
    Ok(())
}

/// The long comments of an object: the object's own (`None`) and its
/// columns', by system name.
#[must_use]
pub fn comments_of(info: Option<&SqlInfo>) -> Vec<comment::Comment> {
    info.map(|i| {
        i.all(sqlinf::tag::COMMENT)
            .filter_map(|b| comment::Comment::decode(b).ok())
            .collect()
    })
    .unwrap_or_default()
}
