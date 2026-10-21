// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The logical file compiler: formats over `PFILE` files, join formats
//! over `JFILE` files, field subsets and derivations, keys, select/omit.

mod fields;
mod join;

use crate::compile::filekw::{self, FileKeywords};
use crate::compile::{
    self, Compilation, CompileOpts, FileKind, MAX_FIELDS, MAX_RECORD_LEN, field, keys, selomit,
};
use crate::diag::{Diagnostics, ids};
use crate::lex::{self, NameType, Spec};
use crate::listing;
use crate::model::{FieldSource, FileRef, LfDefinition, LfFormat, RecordFormat};
use crate::refs::RefResolver;
use crate::source::Source;

/// Compile logical file DDS.
pub fn compile_lf(
    source: &Source,
    resolver: &dyn RefResolver,
    opts: &CompileOpts,
) -> Compilation<LfDefinition> {
    let mut diags = Diagnostics::default();
    let specs = lex::lex(source, &mut diags);
    let built = build(&specs, resolver, &mut diags);
    let summary = built.as_ref().map(listing::summary::lf);
    let listing = listing::render(
        source,
        summary.as_ref(),
        diags.as_slice(),
        opts,
        "Logical file (LF)",
    );
    let definition = built.filter(|_| !diags.blocks_create());
    Compilation {
        definition,
        diagnostics: diags.into_vec(),
        listing,
    }
}

/// A based-on file with the record format the logical file sees.
pub(crate) struct Base {
    pub(crate) file: FileRef,
    pub(crate) format: RecordFormat,
}

/// Where in a format the compiler is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Fields,
    Keys,
    SelOmit,
}

/// The format being built.
pub(crate) struct Current {
    pub(crate) lf: LfFormat,
    pub(crate) bases: Vec<Base>,
    phase: Phase,
    fields_given: bool,
}

struct Work<'a> {
    resolver: &'a dyn RefResolver,
    fk: FileKeywords,
    formats: Vec<LfFormat>,
    cur: Option<Current>,
}

fn build(
    specs: &[Spec],
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<LfDefinition> {
    let mut w = Work {
        resolver,
        fk: FileKeywords::default(),
        formats: Vec::new(),
        cur: None,
    };
    for spec in specs {
        if spec.file_level {
            filekw::apply(&mut w.fk, spec, FileKind::Logical, diags);
            continue;
        }
        if spec.name_type == Some(NameType::Record) {
            w.finish_current(diags);
            w.cur = start_format(spec, resolver, diags);
            continue;
        }
        let Some(cur) = w.cur.as_mut() else {
            let what = spec.name_type.map_or("field", NameType::what);
            diags.push(ids::spec_before_format(spec.line, what));
            continue;
        };
        match spec.name_type {
            Some(NameType::Join) => join::add(spec, cur, diags),
            Some(NameType::Key) => key(spec, cur, &w.fk, diags),
            Some(NameType::Select | NameType::Omit) => {
                cur.ensure_fields();
                cur.phase = Phase::SelOmit;
                let select = spec.name_type == Some(NameType::Select);
                if let Some(g) = selomit::start(spec, select, &cur.lf.format.fields, diags) {
                    cur.lf.select_omit.push(g);
                }
            }
            _ => match cur.phase {
                Phase::SelOmit => match cur.lf.select_omit.last_mut() {
                    Some(g) => selomit::and_condition(spec, g, &cur.lf.format.fields, diags),
                    None => diags.push(ids::condition_without_group(spec.line)),
                },
                Phase::Keys => {
                    let name = spec.name.as_deref().unwrap_or("");
                    diags.push(ids::field_after_keys(spec.line, name));
                }
                Phase::Fields => {
                    if cur.lf.format.from.is_some() {
                        diags.push(ids::fields_with_format(spec.line));
                    } else {
                        cur.fields_given = true;
                        fields::add(spec, cur, diags);
                    }
                }
            },
        }
    }
    w.finish_current(diags);
    w.finish(diags)
}

fn key(spec: &Spec, cur: &mut Current, fk: &FileKeywords, diags: &mut Diagnostics) {
    if cur.phase == Phase::SelOmit {
        diags.push(ids::key_after_selomit(spec.line));
        return;
    }
    cur.ensure_fields();
    cur.phase = Phase::Keys;
    if fk.refaccpth.is_some() {
        diags.push(ids::keys_with_refaccpth(spec.line));
        return;
    }
    if spec.name.as_deref() == Some("*NONE") {
        if !cur.lf.format.keys.is_empty() || cur.lf.arrival {
            diags.push(ids::none_with_keys(spec.line));
        }
        cur.lf.arrival = true;
        return;
    }
    if cur.lf.arrival {
        diags.push(ids::none_with_keys(spec.line));
        return;
    }
    let altseq = fk.altseq.is_some();
    let Some(k) = keys::build_key(spec, &cur.lf.format, altseq, FileKind::Logical, diags) else {
        return;
    };
    if cur.lf.join && cur.lf.sources[k.field].file() != 0 {
        diags.push(ids::join_key_not_primary(spec.line, &k.name.to_string()));
        return;
    }
    cur.lf.format.keys.push(k);
}

impl Current {
    /// A format with no field lines takes every field of its first
    /// based-on file.
    fn ensure_fields(&mut self) {
        if self.fields_given || self.lf.format.from.is_some() {
            return;
        }
        self.fields_given = true;
        let Some(base) = self.bases.first() else {
            return;
        };
        for f in &base.format.fields {
            let mut f = f.clone();
            f.line = self.lf.format.line;
            self.lf.sources.push(FieldSource::Base {
                file: 0,
                field: f.name,
            });
            self.lf.format.fields.push(f);
        }
    }
}

trait SourceFile {
    fn file(&self) -> usize;
}

impl SourceFile for FieldSource {
    fn file(&self) -> usize {
        match self {
            Self::Base { file, .. } | Self::Concat { file, .. } | Self::Substring { file, .. } => {
                *file
            }
        }
    }
}

/// The `R` line: name, `PFILE` or `JFILE`, `FORMAT`, `TEXT`, and the
/// based-on files resolved.
fn start_format(
    spec: &Spec,
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<Current> {
    let name = compile::spec_name(spec, "record format", diags)?;
    if !spec.attributes_blank() {
        diags.push(ids::attributes_not_allowed(spec.line, "record format"));
    }
    compile::check_db_columns(spec, diags);
    let mut fmt = RecordFormat::new(name);
    fmt.line = spec.line;
    let mut files: Option<(Vec<FileRef>, bool)> = None;
    let mut from: Option<FileRef> = None;
    for kw in &spec.keywords {
        match kw.name.as_str() {
            "PFILE" | "JFILE" => {
                if let Some((_, join)) = files {
                    let other = if join { "JFILE" } else { "PFILE" };
                    if other == kw.name {
                        diags.push(ids::duplicate_keyword(kw.line, &kw.name));
                    } else {
                        diags.push(ids::conflict(kw.line, &kw.name, other));
                    }
                    continue;
                }
                let join = kw.name == "JFILE";
                if !compile::count(kw, 1, 32, diags) {
                    continue;
                }
                let list: Option<Vec<FileRef>> = (0..kw.parms.len())
                    .map(|i| compile::file_ref_parm(kw, i, diags))
                    .collect();
                let Some(list) = list else {
                    continue;
                };
                if join && list.len() < 2 {
                    diags.push(ids::jfile_count(kw.line));
                    continue;
                }
                files = Some((list, join));
            }
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
            _ => compile::level_error(kw, "record", FileKind::Logical, diags),
        }
    }
    let Some((files, join)) = files else {
        diags.push(ids::lf_needs_pfile(spec.line));
        return None;
    };
    let mut bases = Vec::new();
    for fr in &files {
        let formats = field::resolve(spec.line, fr, resolver, diags)?;
        let Some(format) = formats.into_iter().next() else {
            diags.push(ids::ref_format_not_found(spec.line, fr, ""));
            return None;
        };
        bases.push(Base {
            file: fr.clone(),
            format,
        });
    }
    let mut cur = Current {
        lf: LfFormat {
            format: fmt,
            based_on: files,
            base_formats: bases.iter().map(|b| b.format.name).collect(),
            sources: Vec::new(),
            select_omit: Vec::new(),
            arrival: false,
            join,
            joins: Vec::new(),
        },
        bases,
        phase: Phase::Fields,
        fields_given: false,
    };
    if let Some(fr) = from {
        fields::copy_format(&mut cur, &fr, resolver, spec.line, diags);
    }
    Some(cur)
}

impl Work<'_> {
    fn finish_current(&mut self, diags: &mut Diagnostics) {
        let Some(mut cur) = self.cur.take() else {
            return;
        };
        cur.ensure_fields();
        join::check(&cur, diags);
        let fmt = &mut cur.lf.format;
        fmt.lay_out();
        if fmt.fields.is_empty() {
            diags.push(ids::empty_format(fmt.line, &fmt.name.to_string()));
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
        self.formats.push(cur.lf);
    }

    fn finish(self, diags: &mut Diagnostics) -> Option<LfDefinition> {
        let Work {
            resolver,
            fk,
            mut formats,
            ..
        } = self;
        if formats.is_empty() {
            diags.push(ids::no_record_format());
            return None;
        }
        if formats.iter().any(|f| f.join)
            && let Some(second) = formats.get(1)
        {
            diags.push(ids::join_one_format(second.format.line));
        }
        if let Some((fr, line)) = &fk.refaccpth {
            for f in &mut formats {
                keys::copy_keys(&mut f.format, fr, *line, resolver, diags);
            }
        }
        let keyed = formats.iter().any(|f| !f.format.keys.is_empty());
        if keyed {
            for f in formats.iter().filter(|f| f.format.keys.is_empty()) {
                diags.push(ids::keyed_mix(f.format.line, &f.format.name.to_string()));
            }
        } else {
            if let Some((_, line)) = fk.unique {
                diags.push(ids::needs_keys(line, "UNIQUE"));
            }
            if let Some((order, line)) = fk.dup_order {
                diags.push(ids::needs_keys(line, order.name()));
            }
        }
        if let Some(line) = fk.jdftval
            && !formats.iter().any(|f| f.join)
        {
            diags.push(ids::jdftval_without_join(line));
        }
        Some(LfDefinition {
            formats,
            unique: fk.unique.map(|(n, _)| n),
            dup_order: fk.dup_order.map(|(o, _)| o),
            refaccpth: fk.refaccpth.map(|(r, _)| r),
            altseq: fk.altseq,
            text: fk.text,
            dynslt: fk.dynslt.is_some(),
            jdftval: fk.jdftval.is_some(),
        })
    }
}
