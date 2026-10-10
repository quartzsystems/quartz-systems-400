// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The physical file compiler: file-level keywords, one record format,
//! its fields and keys.

use crate::compile::field::{self, FieldContext};
use crate::compile::filekw::{self, FileKeywords};
use crate::compile::{self, Compilation, CompileOpts, FileKind, MAX_FIELDS, MAX_RECORD_LEN, keys};
use crate::diag::{Diagnostics, ids};
use crate::lex::{self, NameType, Spec};
use crate::listing;
use crate::model::{FileRef, KeyField, PfDefinition, RecordFormat};
use crate::refs::RefResolver;
use crate::source::Source;

/// Compile physical file DDS.
pub fn compile_pf(
    source: &Source,
    resolver: &dyn RefResolver,
    opts: &CompileOpts,
) -> Compilation<PfDefinition> {
    let mut diags = Diagnostics::default();
    let specs = lex::lex(source, &mut diags);
    let built = build(&specs, resolver, &mut diags);
    let listing = listing::render(
        source,
        built.as_ref(),
        diags.as_slice(),
        opts,
        "Physical file (PF)",
    );
    let definition = built.filter(|_| !diags.blocks_create());
    Compilation {
        definition,
        diagnostics: diags.into_vec(),
        listing,
    }
}

fn build(
    specs: &[Spec],
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<PfDefinition> {
    let kind = FileKind::Physical;
    let mut fk = FileKeywords::default();
    let mut format: Option<RecordFormat> = None;
    let mut keys_started = false;
    let mut skipping = false;
    for spec in specs {
        if spec.file_level {
            filekw::apply(&mut fk, spec, kind, diags);
            continue;
        }
        match spec.name_type {
            Some(NameType::Record) => {
                if format.is_some() {
                    let name = spec.name.as_deref().unwrap_or("");
                    diags.push(ids::second_format(spec.line, name));
                    skipping = true;
                    continue;
                }
                format = start_format(spec, resolver, diags);
                skipping = format.is_none();
            }
            _ if skipping => {}
            Some(NameType::Key) => {
                let Some(fmt) = format.as_mut() else {
                    diags.push(ids::spec_before_format(spec.line, "key"));
                    continue;
                };
                if fk.refaccpth.is_some() {
                    diags.push(ids::keys_with_refaccpth(spec.line));
                    continue;
                }
                keys_started = true;
                if let Some(k) = keys::build_key(spec, fmt, fk.altseq.is_some(), kind, diags) {
                    fmt.keys.push(k);
                }
            }
            Some(other) => diags.push(ids::spec_not_in_pf(spec.line, other.what())),
            None => {
                let Some(fmt) = format.as_mut() else {
                    diags.push(ids::spec_before_format(spec.line, "field"));
                    continue;
                };
                if keys_started {
                    let name = spec.name.as_deref().unwrap_or("");
                    diags.push(ids::field_after_keys(spec.line, name));
                    continue;
                }
                if fmt.from.is_some() {
                    diags.push(ids::fields_with_format(spec.line));
                    continue;
                }
                let ctx = FieldContext {
                    kind,
                    file_ref: fk.reference.as_ref(),
                    resolver,
                    earlier: &fmt.fields,
                };
                if let Some(f) = field::build_field(spec, &ctx, diags) {
                    if fmt.field(&f.name).is_some() {
                        diags.push(ids::duplicate_field(spec.line, &f.name.to_string()));
                    } else {
                        fmt.fields.push(f);
                    }
                }
            }
        }
    }
    let Some(mut fmt) = format else {
        diags.push(ids::no_record_format());
        return None;
    };
    finish(&mut fmt, &fk, resolver, diags);
    Some(PfDefinition {
        format: fmt,
        unique: fk.unique.map(|(n, _)| n),
        dup_order: fk.dup_order.map(|(o, _)| o),
        reference: fk.reference,
        refaccpth: fk.refaccpth.map(|(r, _)| r),
        altseq: fk.altseq,
        ccsid: fk.ccsid,
        text: fk.text,
    })
}

/// The `R` line: name, `TEXT`, and `FORMAT` copying another file's fields.
fn start_format(
    spec: &Spec,
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<RecordFormat> {
    let name = compile::spec_name(spec, "record format", diags)?;
    if !spec.attributes_blank() {
        diags.push(ids::attributes_not_allowed(spec.line, "record format"));
    }
    compile::check_db_columns(spec, diags);
    let mut fmt = RecordFormat::new(name);
    fmt.line = spec.line;
    let mut from: Option<FileRef> = None;
    for kw in &spec.keywords {
        match kw.name.as_str() {
            "FORMAT" => {
                if compile::seen_before(from.is_some(), kw, diags)
                    || !compile::count(kw, 1, 1, diags)
                {
                    continue;
                }
                from = compile::file_ref_parm(kw, 0, diags);
            }
            "TEXT" => {
                if compile::seen_before(fmt.text.is_some(), kw, diags)
                    || !compile::count(kw, 1, 1, diags)
                {
                    continue;
                }
                fmt.text = compile::quoted_parm(kw, 0, 50, diags);
            }
            _ => compile::level_error(kw, "record", FileKind::Physical, diags),
        }
    }
    if let Some(fr) = from {
        copy_format(&mut fmt, &fr, resolver, spec.line, diags);
    }
    Some(fmt)
}

fn copy_format(
    fmt: &mut RecordFormat,
    fr: &FileRef,
    resolver: &dyn RefResolver,
    line: usize,
    diags: &mut Diagnostics,
) {
    fmt.from = Some(fr.clone());
    let Some(formats) = field::resolve(line, fr, resolver, diags) else {
        return;
    };
    match formats.into_iter().find(|r| r.name == fmt.name) {
        Some(r) => {
            fmt.fields = r.fields;
            for f in &mut fmt.fields {
                f.line = line;
            }
        }
        None => diags.push(ids::ref_format_not_found(line, fr, &fmt.name.to_string())),
    }
}

/// Whole-file checks once every specification is in.
fn finish(
    fmt: &mut RecordFormat,
    fk: &FileKeywords,
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) {
    if let Some((fr, line)) = &fk.refaccpth {
        copy_keys(fmt, fr, *line, resolver, diags);
    }
    fmt.lay_out();
    let name = fmt.name.to_string();
    if fmt.fields.is_empty() {
        diags.push(ids::empty_format(fmt.line, &name));
    }
    if fmt.fields.len() > MAX_FIELDS {
        diags.push(ids::too_many_fields(fmt.line, MAX_FIELDS));
    }
    if fmt.record_len > MAX_RECORD_LEN {
        diags.push(ids::record_too_long(
            fmt.line,
            fmt.record_len,
            MAX_RECORD_LEN,
        ));
    }
    keys::check_limits(fmt, diags);
    if fmt.keys.is_empty() {
        if let Some((_, line)) = fk.unique {
            diags.push(ids::needs_keys(line, "UNIQUE"));
        }
        if let Some((order, line)) = fk.dup_order {
            diags.push(ids::needs_keys(line, order.name()));
        }
    }
}

/// `REFACCPTH`: the keys of another file's format, by field name.
fn copy_keys(
    fmt: &mut RecordFormat,
    fr: &FileRef,
    line: usize,
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) {
    let Some(formats) = field::resolve(line, fr, resolver, diags) else {
        return;
    };
    let source = match &fr.format {
        Some(n) => formats.iter().find(|r| r.name == *n),
        None => formats.first(),
    };
    let Some(source) = source else {
        let want = fr.format.map(|n| n.to_string()).unwrap_or_default();
        diags.push(ids::ref_format_not_found(line, fr, &want));
        return;
    };
    if source.keys.is_empty() {
        diags.push(ids::refaccpth_no_keys(line, fr));
        return;
    }
    for k in &source.keys {
        match fmt.field(&k.name) {
            Some((index, _)) => fmt.keys.push(KeyField {
                field: index,
                line,
                ..k.clone()
            }),
            None => diags.push(ids::key_not_in_format(line, &k.name.to_string())),
        }
    }
}
