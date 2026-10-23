// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The display-file compiler: lex with the `dds` front end, build the
//! file, record and field levels, then check the whole file and lay
//! out the buffers. What the three levels share lives here: the
//! keyword table lookup and the parameter helpers.

mod check;
mod field;
mod file;
mod record;

use dds::diag::{Diagnostics, ids as dds_ids};
use dds::lex::{Keyword, NameType, Spec};
use dds::model::FileRef;
use dds::{Compilation, CompileOpts, RefResolver, Source};

use crate::cond::{self, Cond, Entry};
use crate::diag::ids;
use crate::kw::{self, KwInfo, Level, Support};
use crate::listing;
use crate::model::{DisplayFile, DisplaySize, FileAttrs, KeywordUse};

/// Compile display file DDS.
pub fn compile_dspf(
    source: &Source,
    resolver: &dyn RefResolver,
    opts: &CompileOpts,
) -> Compilation<DisplayFile> {
    let mut diags = Diagnostics::default();
    let specs = dds::lex::lex(source, &mut diags);
    let built = build(source, &specs, resolver, &mut diags);
    let listing = listing::render(source, built.as_ref(), diags.as_slice(), opts);
    let definition = built.filter(|_| !diags.blocks_create());
    Compilation {
        definition,
        diagnostics: diags.into_vec(),
        listing,
    }
}

/// What every level needs.
pub(crate) struct Cx<'a> {
    pub source: &'a Source,
    pub resolver: &'a dyn RefResolver,
    /// `DSPSIZ`, the primary size first.
    pub sizes: Vec<DisplaySize>,
    /// The `*DSn` names of those sizes.
    pub names: Vec<String>,
    /// `REF`.
    pub ref_file: Option<FileRef>,
}

impl<'a> Cx<'a> {
    /// A context over the file-level attributes as they stand.
    pub fn new(source: &'a Source, resolver: &'a dyn RefResolver, file: &FileAttrs) -> Self {
        Self {
            source,
            resolver,
            sizes: file.sizes.clone(),
            names: file.size_names(),
            ref_file: file.ref_file.clone(),
        }
    }

    /// The condition on `line` of `entry`.
    pub fn cond(&self, entry: Entry, line: usize, diags: &mut Diagnostics) -> Cond {
        cond::parse(self.source, entry, line, &self.names, diags)
    }
}

/// The line ranges of the specifications.
fn entries(specs: &[Spec], len: usize) -> Vec<Entry> {
    specs
        .iter()
        .enumerate()
        .map(|(i, s)| Entry {
            first: s.line,
            last: specs
                .get(i + 1)
                .map_or(len.saturating_sub(1), |n| n.line.saturating_sub(1)),
        })
        .collect()
}

fn build(
    source: &Source,
    specs: &[Spec],
    resolver: &dyn RefResolver,
    diags: &mut Diagnostics,
) -> Option<DisplayFile> {
    let mut df = DisplayFile {
        source: source.clone(),
        ..DisplayFile::default()
    };
    let ranges = entries(specs, source.len());
    let mut cx = Cx::new(source, resolver, &df.file);
    let mut skipping = false;
    for (spec, entry) in specs.iter().zip(ranges) {
        if spec.file_level {
            file::apply(&mut df.file, spec, entry, &cx, diags);
            cx = Cx::new(source, resolver, &df.file);
            continue;
        }
        match spec.name_type {
            Some(NameType::Record) => {
                let started = record::start(&cx, spec, entry, &mut df.file, diags);
                skipping = started.is_none();
                if let Some(r) = started {
                    if df.record(&r.name).is_some() {
                        diags.push(ids::duplicate_record(spec.line, &r.name));
                    }
                    df.records.push(r);
                }
            }
            Some(other) => diags.push(ids::spec_not_in_dspf(spec.line, other.what())),
            None if skipping => {}
            None => {
                let Some(rec) = df.records.last_mut() else {
                    diags.push(dds_ids::spec_before_format(spec.line, "field"));
                    continue;
                };
                if field::is_alt_location(spec) {
                    field::alt_location(&cx, rec, spec, entry, &mut df.file, diags);
                } else if let Some(f) = field::build(&cx, rec, spec, entry, &mut df.file, diags) {
                    rec.fields.push(f);
                }
            }
        }
    }
    if df.records.is_empty() {
        diags.push(ids::no_record_format());
        return None;
    }
    check::file(&mut df, diags);
    Some(df)
}

/// The constant `'text'`, which the lexer names `'`: a field-level
/// pseudo-keyword outside the table.
const CONSTANT_INFO: KwInfo = KwInfo {
    name: dds::lex::keyword::CONSTANT,
    levels: kw::D,
    support: Support::Supported,
    parms: (1, 1),
    sfl: kw::SflSite::Any,
    cond: true,
};

/// Look a keyword up for `level`, reporting what is wrong with it;
/// `None` when it must be skipped.
pub(crate) fn classify(
    k: &Keyword,
    level: Level,
    diags: &mut Diagnostics,
) -> Option<&'static KwInfo> {
    let found = if k.name == dds::lex::keyword::CONSTANT {
        Some(&CONSTANT_INFO)
    } else {
        kw::lookup(&k.name)
    };
    let Some(info) = found else {
        if kw::is_cmd_key(&k.name) {
            diags.push(ids::cmd_key_range(k.line, &k.name));
        } else {
            diags.push(dds_ids::unknown_keyword(k.line, &k.name));
        }
        return None;
    };
    if info.support == Support::Rejected {
        diags.push(ids::rejected(
            k.line,
            &k.name,
            "it has no place in this release",
        ));
        return None;
    }
    if !info.allows(level) {
        diags.push(dds_ids::not_at_level(
            k.line,
            &k.name,
            level.name(),
            "display",
        ));
        return None;
    }
    let n = k.parms.len();
    let (min, max) = (usize::from(info.parms.0), usize::from(info.parms.1));
    if n < min || (max != 255 && n > max) {
        let want = if min == max {
            min.to_string()
        } else if max == 255 {
            format!("at least {min}")
        } else {
            format!("{min} to {max}")
        };
        diags.push(dds_ids::parm_count(k.line, &k.name, &want, n));
        return None;
    }
    Some(info)
}

/// The keyword with its condition, checked against the table: a
/// stored keyword gets its note, one that takes no option indicators
/// gets its error.
pub(crate) fn accept(
    cx: &Cx<'_>,
    k: &Keyword,
    info: &KwInfo,
    entry: Entry,
    diags: &mut Diagnostics,
) -> KeywordUse {
    let cond = cx.cond(entry, k.line, diags);
    if !info.cond && !cond.any.is_empty() {
        diags.push(ids::no_option_indicators(k.line, &k.name));
    }
    if info.support == Support::Stored {
        diags.push(ids::stored_only(k.line, &k.name));
    }
    KeywordUse {
        name: k.name.clone(),
        parms: k.parms.clone(),
        cond,
        line: k.line,
    }
}

/// Parameter `i` as a number.
pub(crate) fn number(k: &Keyword, i: usize) -> Option<u32> {
    k.parms.get(i)?.as_word()?.parse().ok()
}

/// Parameter `i` as a number, reported when it is not one in `lo..=hi`.
pub(crate) fn number_in(
    k: &Keyword,
    i: usize,
    lo: u32,
    hi: u32,
    diags: &mut Diagnostics,
) -> Option<u32> {
    match number(k, i) {
        Some(n) if (lo..=hi).contains(&n) => Some(n),
        _ => {
            let shown = k
                .parms
                .get(i)
                .map_or_else(|| "(none)".to_string(), ToString::to_string);
            diags.push(dds_ids::bad_parm(
                k.line,
                &k.name,
                shown,
                format!("must be a number from {lo} to {hi}"),
            ));
            None
        }
    }
}

/// Parameter `i` as quoted text of at most `max` characters.
pub(crate) fn quoted(k: &Keyword, i: usize, max: usize, diags: &mut Diagnostics) -> Option<String> {
    let Some(p) = k.parms.get(i) else {
        diags.push(dds_ids::parm_count(
            k.line,
            &k.name,
            &(i + 1).to_string(),
            k.parms.len(),
        ));
        return None;
    };
    match p.as_quoted() {
        Some(q) if q.chars().count() <= max => Some(q.to_string()),
        Some(_) => {
            diags.push(dds_ids::bad_parm(
                k.line,
                &k.name,
                p,
                format!("longer than {max} characters"),
            ));
            None
        }
        None => {
            diags.push(dds_ids::bad_parm(k.line, &k.name, p, "must be quoted"));
            None
        }
    }
}

/// An object name without quotes, uppercased.
pub(crate) fn name_of(text: &str) -> Result<String, String> {
    if text.starts_with('"') {
        return Err("quoted names are not allowed".to_string());
    }
    qsname::Name::parse(text)
        .map(|n| n.to_string())
        .map_err(|e| e.to_string())
}

/// Parameter `i` as an object name.
pub(crate) fn name_parm(k: &Keyword, i: usize, diags: &mut Diagnostics) -> Option<String> {
    let Some(p) = k.parms.get(i) else {
        diags.push(dds_ids::parm_count(
            k.line,
            &k.name,
            &(i + 1).to_string(),
            k.parms.len(),
        ));
        return None;
    };
    let parsed = p
        .as_word()
        .ok_or_else(|| "must be a name".to_string())
        .and_then(name_of);
    match parsed {
        Ok(n) => Some(n),
        Err(why) => {
            diags.push(dds_ids::bad_parm(k.line, &k.name, p, why));
            None
        }
    }
}

/// Parameter `i` as `&FIELD`: the field's name.
pub(crate) fn field_parm(k: &Keyword, i: usize, diags: &mut Diagnostics) -> Option<String> {
    let Some(p) = k.parms.get(i) else {
        diags.push(dds_ids::parm_count(
            k.line,
            &k.name,
            &(i + 1).to_string(),
            k.parms.len(),
        ));
        return None;
    };
    let parsed = p
        .as_word()
        .and_then(|w| w.strip_prefix('&'))
        .ok_or_else(|| "must be &field".to_string())
        .and_then(name_of);
    match parsed {
        Ok(n) => Some(n),
        Err(why) => {
            diags.push(dds_ids::bad_parm(k.line, &k.name, p, why));
            None
        }
    }
}

/// Parameter `i` as `LIB/FILE` or `FILE`.
pub(crate) fn file_parm(k: &Keyword, i: usize, diags: &mut Diagnostics) -> Option<FileRef> {
    let Some(p) = k.parms.get(i) else {
        diags.push(dds_ids::parm_count(
            k.line,
            &k.name,
            &(i + 1).to_string(),
            k.parms.len(),
        ));
        return None;
    };
    let parsed = p
        .as_word()
        .ok_or_else(|| "must be a file name".to_string())
        .and_then(FileRef::parse);
    match parsed {
        Ok(r) => Some(r),
        Err(why) => {
            diags.push(dds_ids::bad_parm(k.line, &k.name, p, why));
            None
        }
    }
}

/// `[response-indicator ['text']]` from parameter `from` on: the
/// indicator and the text. `None` when what is there is wrong.
pub(crate) fn ind_text(
    k: &Keyword,
    from: usize,
    diags: &mut Diagnostics,
) -> Option<(Option<u8>, Option<String>)> {
    let mut ind = None;
    let mut text = None;
    let mut at = from;
    if let Some(p) = k.parms.get(at)
        && let Some(w) = p.as_word()
    {
        match w.parse::<u8>() {
            Ok(n) if (1..=99).contains(&n) => ind = Some(n),
            _ => {
                diags.push(ids::response_indicator(k.line, &k.name, "must be 01 to 99"));
                return None;
            }
        }
        at += 1;
    }
    if let Some(p) = k.parms.get(at) {
        match p.as_quoted() {
            Some(q) if ind.is_some() => text = Some(q.to_string()),
            Some(_) => {
                diags.push(ids::response_indicator(
                    k.line,
                    &k.name,
                    "must come before the indicator text",
                ));
                return None;
            }
            None => {
                diags.push(dds_ids::bad_parm(k.line, &k.name, p, "must be quoted text"));
                return None;
            }
        }
        at += 1;
    }
    if at < k.parms.len() {
        diags.push(dds_ids::parm_count(
            k.line,
            &k.name,
            &format!("at most {at}"),
            k.parms.len(),
        ));
        return None;
    }
    Some((ind, text))
}
