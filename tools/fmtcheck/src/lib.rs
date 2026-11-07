// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The format freeze check. `formats.toml` names every persistent or wire
//! format frozen at a release: the source that defines it and the constant
//! that carries its version. `formats.lock` records, per format, that
//! version and a hash of the definitions. [`check`] fails when the hash
//! moved and the version did not, or when the version moved and the lock
//! was not updated with it.
//!
//! The hash is over Rust tokens, so comments, doc comments, formatting and
//! `#[cfg(test)]` items do not count; anything the compiler sees does.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The manifest, relative to the repository root.
pub const MANIFEST: &str = "tools/fmtcheck/formats.toml";
/// The lock, relative to the repository root.
pub const LOCK: &str = "tools/fmtcheck/formats.lock";

/// What went wrong reading the manifest or the sources.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read.
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    /// A TOML file did not parse.
    #[error("{0}: {1}")]
    Toml(PathBuf, String),
    /// A Rust source did not parse.
    #[error("{0}: {1}")]
    Parse(PathBuf, String),
    /// A named item or the version constant is missing.
    #[error("{0}: no item `{1}`")]
    Missing(PathBuf, String),
}

/// `formats.toml`.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    /// Every frozen format.
    pub format: Vec<Format>,
}

/// One frozen format.
#[derive(Debug, Deserialize)]
pub struct Format {
    /// Its name in the lock and the reports.
    pub name: String,
    /// The release it was frozen at.
    pub frozen_at: String,
    /// The document that specifies it; it must say it is frozen.
    #[serde(default)]
    pub doc: Option<String>,
    /// Draft: hashed and reported, never failed.
    #[serde(default)]
    pub draft: bool,
    /// The constants that carry its version, as `file::NAME`.
    pub version: Vec<String>,
    /// Whole files whose every item is part of the format.
    #[serde(default)]
    pub files: Vec<String>,
    /// Single items: `file::kind Name` (`struct`, `enum`, `const`,
    /// `static`, `fn`, `type`, `impl`, `mod`).
    #[serde(default)]
    pub items: Vec<String>,
    /// Items inside `files` that are not part of the format (shipped
    /// data such as `fn shipped`): `file::kind Name`.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// `formats.lock`.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Lock {
    /// Per format, by name.
    #[serde(default)]
    pub format: BTreeMap<String, Locked>,
}

/// What the lock holds for one format.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Locked {
    /// The release it was frozen at.
    pub frozen_at: String,
    /// The version constants' values, joined by `/`.
    pub version: String,
    /// SHA-256 of the definitions.
    pub hash: String,
}

/// One format's state against the lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Same version and hash.
    Same,
    /// Not in the lock yet.
    New,
    /// The definitions changed, the version did not.
    ChangedWithoutBump,
    /// The version moved; the lock must be updated (with a migration).
    Bumped {
        /// The version in the lock.
        from: String,
    },
}

/// One line of the report.
#[derive(Debug, Clone)]
pub struct Row {
    /// The format.
    pub name: String,
    /// Draft formats never fail.
    pub draft: bool,
    /// What it is now.
    pub now: Locked,
    /// Against the lock.
    pub state: State,
    /// The document lacks its frozen section.
    pub doc_unmarked: bool,
}

impl Row {
    /// Whether this row fails the check.
    pub fn fails(&self) -> bool {
        !self.draft && (self.state != State::Same || self.doc_unmarked)
    }
}

fn read(root: &Path, rel: &str) -> Result<String, Error> {
    let p = root.join(rel);
    fs::read_to_string(&p).map_err(|e| Error::Io(p, e))
}

/// Reads the manifest.
pub fn manifest(root: &Path) -> Result<Manifest, Error> {
    let s = read(root, MANIFEST)?;
    toml::from_str(&s).map_err(|e| Error::Toml(root.join(MANIFEST), e.to_string()))
}

/// Reads the lock; a missing one is empty.
pub fn lock(root: &Path) -> Result<Lock, Error> {
    let p = root.join(LOCK);
    if !p.exists() {
        return Ok(Lock::default());
    }
    let s = read(root, LOCK)?;
    toml::from_str(&s).map_err(|e| Error::Toml(p, e.to_string()))
}

/// Drops doc attributes (`#[doc = ...]`, `#![doc = ...]`) at every depth.
fn strip_docs(ts: TokenStream) -> TokenStream {
    let toks: Vec<TokenTree> = ts.into_iter().collect();
    let mut out = Vec::with_capacity(toks.len());
    let mut i = 0;
    while i < toks.len() {
        if let TokenTree::Punct(p) = &toks[i]
            && p.as_char() == '#'
        {
            let mut j = i + 1;
            if let Some(TokenTree::Punct(b)) = toks.get(j)
                && b.as_char() == '!'
            {
                j += 1;
            }
            if let Some(TokenTree::Group(g)) = toks.get(j)
                && g.delimiter() == Delimiter::Bracket
                && matches!(g.stream().into_iter().next(), Some(TokenTree::Ident(id)) if id == "doc")
            {
                i = j + 1;
                continue;
            }
        }
        match &toks[i] {
            TokenTree::Group(g) => {
                let mut ng = proc_macro2::Group::new(g.delimiter(), strip_docs(g.stream()));
                ng.set_span(g.span());
                out.push(TokenTree::Group(ng));
            }
            t => out.push(t.clone()),
        }
        i += 1;
    }
    out.into_iter().collect()
}

fn is_cfg_test(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.meta.to_token_stream().to_string().replace(' ', "") == "cfg(test)"
    })
}

fn attrs_of(item: &syn::Item) -> &[syn::Attribute] {
    use syn::Item as I;
    match item {
        I::Const(i) => &i.attrs,
        I::Enum(i) => &i.attrs,
        I::Fn(i) => &i.attrs,
        I::Impl(i) => &i.attrs,
        I::Mod(i) => &i.attrs,
        I::Static(i) => &i.attrs,
        I::Struct(i) => &i.attrs,
        I::Trait(i) => &i.attrs,
        I::Type(i) => &i.attrs,
        I::Union(i) => &i.attrs,
        I::Use(i) => &i.attrs,
        I::Macro(i) => &i.attrs,
        _ => &[],
    }
}

/// Whether `item` is the one `kind name` names. `impl Name` matches every
/// impl block whose self type is `Name` (with or without a trait).
fn matches(item: &syn::Item, kind: &str, name: &str) -> bool {
    use syn::Item as I;
    match (kind, item) {
        ("const", I::Const(i)) => i.ident == name,
        ("enum", I::Enum(i)) => i.ident == name,
        ("fn", I::Fn(i)) => i.sig.ident == name,
        ("mod", I::Mod(i)) => i.ident == name,
        ("static", I::Static(i)) => i.ident == name,
        ("struct", I::Struct(i)) => i.ident == name,
        ("trait", I::Trait(i)) => i.ident == name,
        ("type", I::Type(i)) => i.ident == name,
        ("union", I::Union(i)) => i.ident == name,
        ("impl", I::Impl(i)) => match &*i.self_ty {
            syn::Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == name),
            _ => false,
        },
        ("macro", I::Macro(i)) => {
            i.mac.path.is_ident(name) || i.ident.as_ref().is_some_and(|id| id == name)
        }
        _ => false,
    }
}

fn parse(root: &Path, rel: &str) -> Result<syn::File, Error> {
    let src = read(root, rel)?;
    syn::parse_file(&src).map_err(|e| Error::Parse(root.join(rel), e.to_string()))
}

fn tokens(item: &syn::Item) -> String {
    strip_docs(item.to_token_stream()).to_string()
}

/// The canonical text of a whole file: every non-test item's tokens.
fn file_text(root: &Path, rel: &str, exclude: &[String]) -> Result<String, Error> {
    let f = parse(root, rel)?;
    let skip: Vec<(&str, &str)> = exclude
        .iter()
        .filter_map(|r| {
            let (file, what) = split_ref(r);
            (file == rel).then(|| what.split_once(' ').unwrap_or(("", what)))
        })
        .collect();
    let mut s = String::new();
    for item in &f.items {
        if is_cfg_test(attrs_of(item)) || skip.iter().any(|(k, n)| matches(item, k, n)) {
            continue;
        }
        s.push_str(&tokens(item));
        s.push('\n');
    }
    Ok(s)
}

fn split_ref(r: &str) -> (&str, &str) {
    r.rsplit_once("::").unwrap_or((r, ""))
}

/// The canonical text of `file::kind Name`.
fn item_text(root: &Path, r: &str) -> Result<String, Error> {
    let (file, what) = split_ref(r);
    let (kind, name) = what.split_once(' ').unwrap_or(("", what));
    let f = parse(root, file)?;
    let mut s = String::new();
    for item in f.items.iter().filter(|i| matches(i, kind, name)) {
        s.push_str(&tokens(item));
        s.push('\n');
    }
    if s.is_empty() {
        return Err(Error::Missing(root.join(file), what.to_string()));
    }
    Ok(s)
}

/// A version constant's value as written (`2`, `b"QSAV"`).
fn version_value(root: &Path, r: &str) -> Result<String, Error> {
    let (file, name) = split_ref(r);
    let f = parse(root, file)?;
    for item in &f.items {
        match item {
            syn::Item::Const(c) if c.ident == name => {
                return Ok(c.expr.to_token_stream().to_string());
            }
            syn::Item::Static(c) if c.ident == name => {
                return Ok(c.expr.to_token_stream().to_string());
            }
            _ => {}
        }
    }
    Err(Error::Missing(root.join(file), name.to_string()))
}

/// What a format is now.
pub fn current(root: &Path, f: &Format) -> Result<Locked, Error> {
    let mut text = String::new();
    for file in &f.files {
        let _ = writeln!(text, "== {file}");
        text.push_str(&file_text(root, file, &f.exclude)?);
    }
    for r in &f.items {
        let _ = writeln!(text, "== {r}");
        text.push_str(&item_text(root, r)?);
    }
    let version = f
        .version
        .iter()
        .map(|v| version_value(root, v))
        .collect::<Result<Vec<_>, _>>()?
        .join("/");
    Ok(Locked {
        frozen_at: f.frozen_at.clone(),
        version,
        hash: hex::encode(Sha256::digest(text.as_bytes())),
    })
}

/// The marker a frozen format's document must carry.
pub fn doc_marker(release: &str) -> String {
    format!("Frozen at {release}")
}

/// Every format against the lock.
pub fn check(root: &Path) -> Result<Vec<Row>, Error> {
    let m = manifest(root)?;
    let l = lock(root)?;
    let mut rows = Vec::new();
    for f in &m.format {
        let now = current(root, f)?;
        let state = match l.format.get(&f.name) {
            None => State::New,
            Some(old) if old.version != now.version => State::Bumped {
                from: old.version.clone(),
            },
            Some(old) if old.hash != now.hash => State::ChangedWithoutBump,
            Some(_) => State::Same,
        };
        let doc_unmarked = match &f.doc {
            Some(d) => !read(root, d)?.contains(&doc_marker(&f.frozen_at)),
            None => false,
        };
        rows.push(Row {
            name: f.name.clone(),
            draft: f.draft,
            now,
            state,
            doc_unmarked,
        });
    }
    Ok(rows)
}

/// Rewrites the lock from the sources. Only for a first freeze or after a
/// version bump with its migration; a change without a bump stays refused
/// unless `force`.
pub fn update(root: &Path, force: bool) -> Result<Vec<Row>, Error> {
    let rows = check(root)?;
    let mut l = lock(root)?;
    for r in &rows {
        if r.state == State::ChangedWithoutBump && !force && !r.draft {
            continue;
        }
        l.format.insert(r.name.clone(), r.now.clone());
    }
    let s = toml::to_string(&l).map_err(|e| Error::Toml(root.join(LOCK), e.to_string()))?;
    let p = root.join(LOCK);
    let head = "# Generated by `cargo run -p fmtcheck -- update`. Do not edit.\n";
    fs::write(&p, format!("{head}{s}")).map_err(|e| Error::Io(p, e))?;
    Ok(rows)
}

/// The repository root: the nearest ancestor with `tools/fmtcheck/formats.toml`.
pub fn find_root(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .find(|p| p.join(MANIFEST).exists())
        .map(Path::to_path_buf)
}

/// Parses `src` as Rust tokens with docs removed (for tests).
pub fn canonical(src: &str) -> Option<String> {
    TokenStream::from_str(src)
        .ok()
        .map(|t| strip_docs(t).to_string())
}
