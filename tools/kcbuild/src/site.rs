// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The site: home, command finder and command pages, message finder and
//! message pages, documents, and the conformance matrix.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::io;
use std::path::Path;

use cmd::def::{CmdDef, Parm};
use sys_cpps::Site;

use crate::md::{self, esc, inline};
use crate::page::Page;

/// The shared stylesheet of the web products.
const CSS: &str = include_str!("../../../net/qsweb/assets/qs400.css");

/// What a build wrote.
#[derive(Debug, Default)]
pub struct Summary {
    /// Command pages.
    pub commands: usize,
    /// Messages described.
    pub messages: usize,
    /// Documents.
    pub docs: usize,
    /// Pages written in all.
    pub pages: usize,
}

fn write(out: &Path, rel: &str, html: &str, n: &mut Summary) -> io::Result<()> {
    let p = out.join(rel);
    if let Some(d) = p.parent() {
        fs::create_dir_all(d)?;
    }
    fs::write(p, html)?;
    n.pages += 1;
    Ok(())
}

fn site_text(s: Site) -> &'static str {
    match s {
        Site::Service => "System (storage service)",
        Site::Job => "Job",
        Site::Stub => "Not yet available",
    }
}

fn parm_type(p: &Parm) -> String {
    let t = format!("{:?}", p.ty).to_ascii_uppercase();
    match &p.len {
        Some(l) if l.dec > 0 => format!("*{t} ({} {})", l.len, l.dec),
        Some(l) => format!("*{t} ({})", l.len),
        None => format!("*{t}"),
    }
}

fn command_page(def: &CmdDef, entry: Option<&sys_cpps::Entry>) -> String {
    let mut b = String::new();
    let _ = writeln!(
        b,
        "<table class=\"props\" cellspacing=\"0\">\
         <tr><th>Command</th><td class=\"mono\">{}</td></tr>\
         <tr><th>Description</th><td>{}</td></tr>\
         <tr><th>Runs in</th><td>{}</td></tr>\
         <tr><th>Threadsafe</th><td>{:?}</td></tr></table>",
        esc(&def.name),
        esc(&def.prompt),
        entry.map_or("System (storage service)", |e| site_text(e.site)),
        def.thdsafe,
    );
    if let Some(e) = entry.filter(|e| e.site == Site::Stub) {
        let _ = writeln!(
            b,
            "<p>This command is defined but not yet available: it ends with message <span class=\"mono\">{}</span>.</p>",
            esc(e.stub.as_deref().unwrap_or("CPF9899"))
        );
    }
    b.push_str("<h2>Parameters</h2>\n");
    if def.parms.is_empty() {
        b.push_str("<p>None.</p>\n");
    } else {
        b.push_str(
            "<table class=\"list\" cellspacing=\"0\"><tr><th>Keyword</th><th>Description</th>\
             <th>Type</th><th>Default</th><th>Values</th><th>Required</th></tr>\n",
        );
        for (i, p) in def.parms.iter().enumerate() {
            let prompt = p.prompt.as_ref().map_or("", |(t, _)| t.as_str());
            let mut values: Vec<String> = p
                .sngval
                .iter()
                .chain(&p.spcval)
                .map(|s| s.from.clone())
                .collect();
            values.extend(p.values.iter().cloned());
            if let Some((lo, hi)) = &p.range {
                values.push(format!("{lo} to {hi}"));
            }
            let _ = writeln!(
                b,
                "<tr{}><td class=\"mono\"><a id=\"{kw}\"></a>{kw}</td><td>{}</td><td class=\"mono\">{}</td>\
                 <td class=\"mono\">{}</td><td class=\"mono\">{}</td><td>{}</td></tr>",
                if i % 2 == 1 { " class=\"alt\"" } else { "" },
                esc(prompt),
                esc(&parm_type(p)),
                esc(p.dft.as_deref().unwrap_or("")),
                esc(&values.join(" ")),
                if p.min > 0 { "Yes" } else { "No" },
                kw = esc(&p.kwd),
            );
        }
        b.push_str("</table>\n");
    }
    b
}

fn commands(out: &Path, n: &mut Summary) -> io::Result<Vec<(String, String, Site)>> {
    let mut rows = Vec::new();
    for (name, source) in sys_cpps::defs::DEFS {
        let Ok(def) = cmd::source::compile(name, source) else {
            continue;
        };
        let entry = sys_cpps::entry(name);
        let body = command_page(&def, entry);
        let html = Page {
            depth: 1,
            crumb: &[("Command finder", "commands/index.html"), (name, "")],
            title: &format!("{name}: {}", def.prompt),
            desc: "",
            body: &body,
        }
        .render();
        write(out, &format!("commands/{name}.html"), &html, n)?;
        n.commands += 1;
        rows.push((
            name.to_string(),
            def.prompt.clone(),
            entry.map_or(Site::Service, |e| e.site),
        ));
    }
    // The finder: every command by its first letter.
    let mut b = String::from("<p>");
    let letters: Vec<char> = {
        let mut v: Vec<char> = rows.iter().filter_map(|r| r.0.chars().next()).collect();
        v.dedup();
        v
    };
    for c in &letters {
        let _ = write!(b, "<a href=\"#{c}\">{c}</a> ");
    }
    b.push_str("</p>\n<table class=\"list\" cellspacing=\"0\"><tr><th>Command</th><th>Description</th><th>Runs in</th></tr>\n");
    let mut last = ' ';
    for (i, (name, prompt, site)) in rows.iter().enumerate() {
        let first = name.chars().next().unwrap_or(' ');
        let anchor = if first == last {
            String::new()
        } else {
            last = first;
            format!("<a id=\"{first}\"></a>")
        };
        let _ = writeln!(
            b,
            "<tr{}><td class=\"mono\">{anchor}<a href=\"{name}.html\">{name}</a></td><td>{}</td><td>{}</td></tr>",
            if i % 2 == 1 { " class=\"alt\"" } else { "" },
            esc(prompt),
            site_text(*site)
        );
    }
    b.push_str("</table>\n");
    let html = Page {
        depth: 1,
        crumb: &[("Command finder", "")],
        title: "Command finder",
        desc: &format!("{} commands shipped with QS/400.", rows.len()),
        body: &b,
    }
    .render();
    write(out, "commands/index.html", &html, n)?;
    Ok(rows)
}

/// Second-level text with its formatting codes as HTML.
fn help_text(s: &str) -> String {
    let e = esc(s);
    e.replace("&amp;N ", "<br />")
        .replace("&amp;P ", "<br />&nbsp;&nbsp;")
        .replace("&amp;B ", "<br />&nbsp;&nbsp;&nbsp;&nbsp;")
}

fn messages(out: &Path, n: &mut Summary) -> io::Result<()> {
    let mut files: Vec<(String, String, msgf::source::MsgFile)> = msgf::source::builtin()
        .into_iter()
        .map(|f| ("QSYS".to_string(), f.name.clone(), f))
        .collect();
    for (lib, _, f) in msgf::source::product_files() {
        files.push((lib.to_string(), f.name.clone(), f));
    }
    let mut index = String::from(
        "<table class=\"list\" cellspacing=\"0\"><tr><th>Message file</th><th>Description</th><th>Messages</th><th>Ranges</th></tr>\n",
    );
    for (i, (lib, name, f)) in files.iter().enumerate() {
        let mut blocks: BTreeMap<String, Vec<&msgf::desc::MsgDesc>> = BTreeMap::new();
        for m in &f.msgs {
            blocks
                .entry(m.id.chars().take(5).collect())
                .or_default()
                .push(m);
        }
        let mut links = String::new();
        for (blk, msgs) in &blocks {
            let mut b = String::new();
            for m in msgs {
                let _ = writeln!(
                    b,
                    "<h3 id=\"{id}\">{id}</h3>\n<table class=\"props\" cellspacing=\"0\">\
                     <tr><th>Message</th><td>{}</td></tr><tr><th>Severity</th><td>{}</td></tr>\
                     <tr><th>Cause and recovery</th><td>{}</td></tr></table>",
                    esc(&m.text),
                    m.severity,
                    help_text(&m.help),
                    id = esc(&m.id)
                );
                n.messages += 1;
            }
            let title = format!("{name}: {blk}xx");
            let html = Page {
                depth: 2,
                crumb: &[("Message finder", "messages/index.html"), (&title, "")],
                title: &title,
                desc: &format!("Messages {blk}00 to {blk}FF in {lib}/{name}."),
                body: &b,
            }
            .render();
            write(out, &format!("messages/{name}/{blk}.html"), &html, n)?;
            let _ = write!(links, "<a href=\"{name}/{blk}.html\">{blk}</a> ");
        }
        let _ = writeln!(
            index,
            "<tr{}><td class=\"mono\">{lib}/{name}</td><td>{}</td><td class=\"num\">{}</td><td class=\"mono\">{links}</td></tr>",
            if i % 2 == 1 { " class=\"alt\"" } else { "" },
            esc(&f.text),
            f.msgs.len()
        );
    }
    index.push_str("</table>\n");
    let html = Page {
        depth: 1,
        crumb: &[("Message finder", "")],
        title: "Message finder",
        desc: "Every message in the message files QS/400 ships, by identifier.",
        body: &index,
    }
    .render();
    write(out, "messages/index.html", &html, n)
}

fn docs(repo: &Path, out: &Path, n: &mut Summary) -> io::Result<()> {
    let dir = repo.join("docs");
    let mut list: Vec<(String, String)> = Vec::new();
    let mut names: Vec<_> = fs::read_dir(&dir)?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    names.sort();
    for p in names {
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let text = fs::read_to_string(&p)?;
        let (title, body) = md::to_html(&text);
        let title = if title.is_empty() {
            stem.clone()
        } else {
            title
        };
        let html = Page {
            depth: 1,
            crumb: &[("Documents", "docs/index.html"), (&title, "")],
            title: &title,
            desc: "",
            body: &body,
        }
        .render();
        write(out, &format!("docs/{stem}.html"), &html, n)?;
        n.docs += 1;
        list.push((stem, title));
    }
    let images = dir.join("images");
    if images.is_dir() {
        for e in fs::read_dir(&images)?.flatten() {
            let to = out.join("docs/images");
            fs::create_dir_all(&to)?;
            fs::copy(e.path(), to.join(e.file_name()))?;
        }
    }
    let mut b = String::from(
        "<table class=\"list\" cellspacing=\"0\"><tr><th>Document</th><th>Title</th></tr>\n",
    );
    for (i, (stem, title)) in list.iter().enumerate() {
        let _ = writeln!(
            b,
            "<tr{}><td class=\"mono\"><a href=\"{stem}.html\">{stem}</a></td><td>{}</td></tr>",
            if i % 2 == 1 { " class=\"alt\"" } else { "" },
            inline(title)
        );
    }
    b.push_str("</table>\n");
    let html = Page {
        depth: 1,
        crumb: &[("Documents", "")],
        title: "Documents",
        desc: "The design and reference documents of QS/400.",
        body: &b,
    }
    .render();
    write(out, "docs/index.html", &html, n)
}

fn conformance(out: &Path, rows: &[(String, String, Site)], n: &mut Summary) -> io::Result<()> {
    let count = |s: Site| rows.iter().filter(|r| r.2 == s).count();
    let mut b = format!(
        "<table class=\"status\" cellspacing=\"0\">\
         <tr><td class=\"lbl\">Commands shipped</td><td>{}</td></tr>\
         <tr><td class=\"lbl\">Run by the system</td><td>{}</td></tr>\
         <tr><td class=\"lbl\">Run in the job</td><td>{}</td></tr>\
         <tr><td class=\"lbl\">Defined, not yet available</td><td>{}</td></tr></table>\n\
         <p>The acceptance state of each phase, the oracle captures and the known differences from \
         IBM i are in the <a href=\"docs/index.html\">documents</a> (PHASE0 to PHASE12, the release notes).</p>\n",
        rows.len(),
        count(Site::Service),
        count(Site::Job),
        count(Site::Stub),
    );
    b.push_str("<table class=\"list\" cellspacing=\"0\"><tr><th>Command</th><th>Runs in</th><th>Processing program</th></tr>\n");
    for (i, (name, _, site)) in rows.iter().enumerate() {
        let cpp = sys_cpps::entry(name).map_or(String::new(), |e| e.cpp.clone());
        let _ = writeln!(
            b,
            "<tr{}><td class=\"mono\"><a href=\"commands/{name}.html\">{name}</a></td><td>{}</td><td class=\"mono\">{}</td></tr>",
            if i % 2 == 1 { " class=\"alt\"" } else { "" },
            site_text(*site),
            esc(&cpp)
        );
    }
    b.push_str("</table>\n");
    let html = Page {
        depth: 0,
        crumb: &[("Conformance matrix", "")],
        title: "Conformance matrix",
        desc: "Every shipped command and where it runs.",
        body: &b,
    }
    .render();
    write(out, "conformance.html", &html, n)
}

/// Builds the site into `out` from the tree at `repo`.
pub fn build(repo: &Path, out: &Path) -> io::Result<Summary> {
    let mut n = Summary::default();
    fs::create_dir_all(out)?;
    fs::write(out.join("qs400.css"), CSS)?;
    let rows = commands(out, &mut n)?;
    messages(out, &mut n)?;
    docs(repo, out, &mut n)?;
    conformance(out, &rows, &mut n)?;
    let body = format!(
        "<table class=\"home\" cellspacing=\"0\"><tr><td valign=\"top\">\
         <p><a class=\"big\" href=\"commands/index.html\">Command finder</a><br />\
         {} commands, each with its parameters.</p>\
         <p><a class=\"big\" href=\"messages/index.html\">Message finder</a><br />\
         {} messages with their cause and recovery.</p>\
         <p><a class=\"big\" href=\"docs/index.html\">Documents</a><br />\
         {} design and reference documents.</p>\
         <p><a class=\"big\" href=\"conformance.html\">Conformance matrix</a><br />\
         where each command runs.</p></td></tr></table>\n",
        n.commands, n.messages, n.docs
    );
    let html = Page {
        depth: 0,
        crumb: &[],
        title: &format!("QS/400 {}", release::RELEASE),
        desc: "Reference for the commands, messages and design of QS/400.",
        body: &body,
    }
    .render();
    write(out, "index.html", &html, &mut n)?;
    Ok(n)
}
