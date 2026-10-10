// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `K` specifications.

use super::{
    FileKind, MAX_KEY_FIELDS, MAX_KEY_LEN, check_db_columns, count, level_error, spec_name,
};
use crate::diag::{Diagnostics, ids};
use crate::lex::Spec;
use crate::model::{DataType, KeyField, KeyTransform, RecordFormat};

/// Build a key field over `format`, whose keys so far are in
/// `format.keys`. `altseq` says whether the file has an `ALTSEQ` table.
pub fn build_key(
    spec: &Spec,
    format: &RecordFormat,
    altseq: bool,
    kind: FileKind,
    diags: &mut Diagnostics,
) -> Option<KeyField> {
    let name = spec_name(spec, "key", diags)?;
    if !spec.attributes_blank() {
        diags.push(ids::attributes_not_allowed(spec.line, "key"));
    }
    check_db_columns(spec, diags);
    let text = name.to_string();
    let Some((index, field)) = format.field(&name) else {
        diags.push(ids::key_not_in_format(spec.line, &text));
        return None;
    };
    if format.keys.iter().any(|k| k.name == name) {
        diags.push(ids::duplicate_key(spec.line, &text));
        return None;
    }
    let t = field.data_type;
    let mut key = KeyField {
        name,
        field: index,
        descend: false,
        transform: KeyTransform::default_for(t),
        noaltseq: false,
        line: spec.line,
    };
    let mut chosen: Option<&str> = None;
    for kw in &spec.keywords {
        let n = kw.name.as_str();
        let (transform, ok) = match n {
            "DESCEND" => {
                count(kw, 0, 0, diags);
                key.descend = true;
                continue;
            }
            "NOALTSEQ" => {
                count(kw, 0, 0, diags);
                if !altseq {
                    diags.push(ids::noaltseq_without_altseq(kw.line, &text));
                }
                key.noaltseq = true;
                continue;
            }
            "SIGNED" => (KeyTransform::Signed, t.is_numeric()),
            "ABSVAL" => (KeyTransform::Absval, t.is_numeric()),
            "UNSIGNED" => (KeyTransform::Unsigned, true),
            "ZONE" => (
                KeyTransform::Zone,
                matches!(t, DataType::Char | DataType::Hex),
            ),
            "DIGIT" => (
                KeyTransform::Digit,
                matches!(t, DataType::Char | DataType::Hex),
            ),
            _ => {
                level_error(kw, "key", kind, diags);
                continue;
            }
        };
        count(kw, 0, 0, diags);
        if !ok {
            diags.push(ids::key_keyword_type(kw.line, n, &text, t.letter()));
            continue;
        }
        if let Some(prev) = chosen {
            diags.push(ids::conflict(kw.line, n, prev));
            continue;
        }
        chosen = Some(n);
        key.transform = transform;
    }
    Some(key)
}

/// Key count and length limits, reported on the last `K` line.
pub fn check_limits(format: &RecordFormat, diags: &mut Diagnostics) {
    let Some(last) = format.keys.last() else {
        return;
    };
    if format.keys.len() > MAX_KEY_FIELDS {
        diags.push(ids::too_many_keys(last.line, MAX_KEY_FIELDS));
    }
    let len = format.key_len();
    if len > MAX_KEY_LEN {
        diags.push(ids::key_too_long(last.line, len, MAX_KEY_LEN));
    }
}
