// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The patterns every page is built from (`shared/components.html`):
//! message rows, the filter row, list tables, the paging line,
//! properties and wizards. Cell and value HTML passed in is trusted;
//! text is escaped here.

use std::fmt::Write as _;

use crate::frame::{Ctx, Product};
use crate::html::{esc, query};
use crate::msg::Message;

/// The rows-per-page choices.
pub const ROW_CHOICES: [usize; 4] = [10, 25, 50, 100];
/// The default rows per page.
pub const DEFAULT_ROWS: usize = 25;

/// A message ID as a link to its second-level text.
pub fn msgid(product: &Product, id: &str) -> String {
    format!(
        "<a class=\"msgid\" href=\"{}\" target=\"_blank\">{}</a>",
        esc(&product.msg(id)),
        esc(id)
    )
}

/// One message row; the link carries the substitution values
/// (`?d=...`), so the second-level text shows them.
pub fn message_row(product: &Product, m: &Message) -> String {
    let mut href = product.msg(&m.id);
    for (i, d) in m.data.iter().enumerate() {
        href.push(if i == 0 { '?' } else { '&' });
        href.push_str("d=");
        href.push_str(&crate::http::form::encode(d));
    }
    format!(
        "<table class=\"msg\"><tr><td class=\"{}\"><a class=\"msgid\" href=\"{}\" target=\"_blank\">{}</a> - {}</td></tr></table>",
        m.kind.class(),
        esc(&href),
        esc(&m.id),
        esc(&m.text)
    )
}

/// The message area.
pub fn messages(product: &Product, ms: &[Message]) -> String {
    ms.iter().map(|m| message_row(product, m)).collect()
}

/// A small `?` opening help for `key` in a new window.
pub fn help_link(product: &Product, key: &str) -> String {
    format!(
        "<a class=\"help\" href=\"{}\" target=\"_blank\">?</a>",
        esc(&product.help(key))
    )
}

/// A status word: good (green), bad (red) or neither (black).
pub fn status(word: &str, good: Option<bool>) -> String {
    match good {
        Some(true) => format!("<span class=\"ok\">{}</span>", esc(word)),
        Some(false) => format!("<span class=\"err\">{}</span>", esc(word)),
        None => esc(word),
    }
}

/// A link.
pub fn link(href: &str, text: &str) -> String {
    format!("<a href=\"{}\">{}</a>", esc(href), esc(text))
}

/// Action links separated by ` | `.
pub fn actions(links: &[(String, &str)]) -> String {
    links.iter().map(|(h, t)| link(h, t)).collect::<Vec<_>>().join(" | ")
}

/// A field of the filter row.
#[derive(Clone, Debug)]
pub enum FilterField {
    /// A select: label, name, options, the chosen one ("" for none).
    Select(String, String, Vec<String>, String),
    /// A text field: label, name, value, size.
    Text(String, String, String, u32),
}

/// The filter row: fields, Go, and Refresh when the data is live.
pub fn filter(fields: &[FilterField], refresh: bool) -> String {
    let mut s = String::from("<form method=\"get\"><table class=\"filter\" cellspacing=\"0\"><tr>");
    for f in fields {
        match f {
            FilterField::Select(label, name, opts, chosen) => {
                let _ = write!(s, "<td>{}:</td><td><select name=\"{}\">", esc(label), esc(name));
                for o in opts {
                    let sel = if o == chosen { " selected" } else { "" };
                    let _ = write!(s, "<option{sel}>{}</option>", esc(o));
                }
                s.push_str("</select></td>");
            }
            FilterField::Text(label, name, value, size) => {
                let _ = write!(
                    s,
                    "<td>{}:</td><td><input type=\"text\" name=\"{}\" value=\"{}\" size=\"{size}\"></td>",
                    esc(label),
                    esc(name),
                    esc(value)
                );
            }
        }
    }
    s.push_str("<td><input type=\"submit\" value=\"Go\">");
    if refresh {
        s.push_str(" <input type=\"submit\" name=\"refresh\" value=\"Refresh\">");
    }
    s.push_str("</td></tr></table></form>");
    s
}

/// A list column: its label and, if sortable, its sort key.
#[derive(Clone, Debug)]
pub struct Col {
    /// The header text.
    pub label: String,
    /// The `?sort=` value.
    pub sort: Option<String>,
}

impl Col {
    /// A column that does not sort.
    pub fn plain(label: &str) -> Self {
        Self {
            label: label.into(),
            sort: None,
        }
    }

    /// A column sorted by `key`.
    pub fn sorted(label: &str, key: &str) -> Self {
        Self {
            label: label.into(),
            sort: Some(key.into()),
        }
    }
}

/// A cell: its class and HTML.
#[derive(Clone, Debug, Default)]
pub struct Cell {
    /// `act`, `mono`, `num`, or none.
    pub class: Option<&'static str>,
    /// The content, HTML.
    pub html: String,
}

impl Cell {
    /// Text.
    pub fn text(s: &str) -> Self {
        Self {
            class: None,
            html: esc(s),
        }
    }

    /// HTML.
    pub fn html(s: String) -> Self {
        Self { class: None, html: s }
    }

    /// Text in Courier (names, numbers, paths, commands).
    pub fn mono(s: &str) -> Self {
        Self {
            class: Some("mono"),
            html: esc(s),
        }
    }

    /// HTML in Courier.
    pub fn mono_html(s: String) -> Self {
        Self {
            class: Some("mono"),
            html: s,
        }
    }

    /// A right-aligned number.
    pub fn num(s: &str) -> Self {
        Self {
            class: Some("num"),
            html: esc(s),
        }
    }

    /// The Actions cell.
    pub fn act(s: String) -> Self {
        Self {
            class: Some("act"),
            html: s,
        }
    }
}

/// A list table.
#[derive(Clone, Debug, Default)]
pub struct List {
    /// The columns.
    pub cols: Vec<Col>,
    /// The rows' cells.
    pub rows: Vec<Vec<Cell>>,
    /// The one italic row when there are none.
    pub empty: String,
    /// The filter values sort links keep (name, value).
    pub keep: Vec<(String, String)>,
}

/// Render a list table.
pub fn list_table(l: &List) -> String {
    let mut s = String::from("<table class=\"list\" width=\"100%\" cellspacing=\"0\" cellpadding=\"0\">\n<tr>\n");
    for c in &l.cols {
        match &c.sort {
            Some(k) => {
                let mut q: Vec<(&str, &str)> = l.keep.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                q.push(("sort", k));
                let _ = writeln!(s, "<th><a href=\"{}\">{}</a></th>", esc(&query(&q)), esc(&c.label));
            }
            None => {
                let _ = writeln!(s, "<th>{}</th>", esc(&c.label));
            }
        }
    }
    s.push_str("</tr>\n");
    if l.rows.is_empty() {
        let _ = writeln!(
            s,
            "<tr><td class=\"empty\" colspan=\"{}\">{}</td></tr>",
            l.cols.len(),
            esc(&l.empty)
        );
    }
    for (i, row) in l.rows.iter().enumerate() {
        s.push_str(if i % 2 == 1 { "<tr class=\"alt\">\n" } else { "<tr>\n" });
        for c in row {
            match c.class {
                Some(cls) => {
                    let _ = writeln!(s, "<td class=\"{cls}\">{}</td>", c.html);
                }
                None => {
                    let _ = writeln!(s, "<td>{}</td>", c.html);
                }
            }
        }
        s.push_str("</tr>\n");
    }
    s.push_str("</table>");
    s
}

/// Where a list is: page (1-based), rows per page, total entries.
#[derive(Clone, Debug)]
pub struct Paging {
    /// The page shown.
    pub page: usize,
    /// Rows per page.
    pub rows: usize,
    /// Entries in all.
    pub total: usize,
    /// The filter and sort values links keep (name, value).
    pub keep: Vec<(String, String)>,
}

impl Paging {
    /// Pages in all (at least 1).
    pub fn pages(&self) -> usize {
        self.total.div_ceil(self.rows.max(1)).max(1)
    }

    /// From `?page=` and `?rows=`, clamped.
    pub fn from_query(page: &str, rows: &str, total: usize, keep: Vec<(String, String)>) -> Self {
        let rows = rows.parse().ok().filter(|r| ROW_CHOICES.contains(r)).unwrap_or(DEFAULT_ROWS);
        let mut p = Self {
            page: 1,
            rows,
            total,
            keep,
        };
        p.page = page.parse().unwrap_or(1).clamp(1, p.pages());
        p
    }

    /// The range of entries on this page.
    pub fn range(&self) -> std::ops::Range<usize> {
        let start = (self.page - 1) * self.rows;
        start.min(self.total)..(start + self.rows).min(self.total)
    }

    fn href(&self, page: usize) -> String {
        let mut q: Vec<(&str, &str)> = self.keep.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        let p = page.to_string();
        let r = self.rows.to_string();
        if self.rows != DEFAULT_ROWS {
            q.push(("rows", &r));
        }
        q.push(("page", &p));
        query(&q)
    }
}

/// The paging line under a list.
pub fn paging(p: &Paging) -> String {
    let of = p.pages();
    let prev = if p.page > 1 {
        link(&p.href(p.page - 1), "< Previous")
    } else {
        "<span class=\"grey\">&lt; Previous</span>".into()
    };
    let next = if p.page < of {
        link(&p.href(p.page + 1), "Next >")
    } else {
        "<span class=\"grey\">Next &gt;</span>".into()
    };
    let mut s = format!(
        "<form method=\"get\" class=\"paging\">Page {} of {of} &nbsp; {prev} &nbsp; {next} &nbsp;&nbsp; Rows per page: <select name=\"rows\">",
        p.page
    );
    for r in ROW_CHOICES {
        let sel = if r == p.rows { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{r}</option>");
    }
    s.push_str("</select> ");
    for (k, v) in &p.keep {
        if !v.is_empty() {
            let _ = write!(s, "<input type=\"hidden\" name=\"{}\" value=\"{}\">", esc(k), esc(v));
        }
    }
    let _ = write!(
        s,
        "<input type=\"submit\" value=\"Go\"> &nbsp; {} {}</form>",
        p.total,
        if p.total == 1 { "entry" } else { "entries" }
    );
    s
}

/// A properties row.
#[derive(Clone, Debug, Default)]
pub struct Prop {
    /// The grey label.
    pub label: String,
    /// A red `*` after the label.
    pub required: bool,
    /// The help key of a `?` link after the label.
    pub help: Option<String>,
    /// The value, HTML.
    pub value: String,
    /// The value cell's class (`mono`).
    pub class: Option<&'static str>,
}

impl Prop {
    /// A row with HTML `value`.
    pub fn new(label: &str, value: String) -> Self {
        Self {
            label: label.into(),
            value,
            ..Self::default()
        }
    }

    /// With a `?` help link.
    #[must_use]
    pub fn help(mut self, key: &str) -> Self {
        self.help = Some(key.into());
        self
    }

    /// Marked required.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// The value cell in Courier.
    #[must_use]
    pub fn mono(mut self) -> Self {
        self.class = Some("mono");
        self
    }
}

/// A group of properties under a bold subheading (none for `None`).
#[derive(Clone, Debug, Default)]
pub struct PropGroup {
    /// The subheading.
    pub title: Option<String>,
    /// The rows.
    pub rows: Vec<Prop>,
}

/// A button: name, label, disabled.
#[derive(Clone, Debug)]
pub struct Button {
    /// The field name sent when pressed.
    pub name: Option<String>,
    /// The label.
    pub value: String,
    /// Greyed out.
    pub disabled: bool,
}

impl Button {
    /// A named button.
    pub fn new(name: &str, value: &str) -> Self {
        Self {
            name: Some(name.into()),
            value: value.into(),
            disabled: false,
        }
    }

    /// A button without a name.
    pub fn bare(value: &str) -> Self {
        Self {
            name: None,
            value: value.into(),
            disabled: false,
        }
    }

    /// Greyed out.
    #[must_use]
    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

/// The buttons row.
pub fn buttons(bs: &[Button]) -> String {
    let mut s = String::from("<div class=\"buttons\">");
    for b in bs {
        s.push_str("<input type=\"submit\"");
        if let Some(n) = &b.name {
            let _ = write!(s, " name=\"{}\"", esc(n));
        }
        let _ = write!(s, " value=\"{}\"", esc(&b.value));
        if b.disabled {
            s.push_str(" disabled");
        }
        s.push('>');
    }
    s.push_str("</div>");
    s
}

/// A properties table; `spaced` adds `cellspacing="0"` as standalone
/// tables have it (wizard bodies leave it out).
pub fn props_table(product: &Product, groups: &[PropGroup], spaced: bool) -> String {
    let mut s = String::from(if spaced {
        "<table class=\"props\" cellspacing=\"0\">\n"
    } else {
        "<table class=\"props\">"
    });
    let nl = if spaced { "\n" } else { "" };
    for g in groups {
        if let Some(t) = &g.title {
            let _ = write!(s, "<tr class=\"subrow\"><td colspan=\"2\">{}</td></tr>{nl}", esc(t));
        }
        for r in &g.rows {
            let mut label = esc(&r.label);
            if r.required {
                label.push_str(" <span class=\"req\">*</span>");
            }
            if let Some(h) = &r.help {
                label.push(' ');
                label.push_str(&help_link(product, h));
            }
            match r.class {
                Some(c) => {
                    let _ = write!(s, "<tr><th>{label}</th><td class=\"{c}\">{}</td></tr>{nl}", r.value);
                }
                None => {
                    let _ = write!(s, "<tr><th>{label}</th><td>{}</td></tr>{nl}", r.value);
                }
            }
        }
    }
    s.push_str("</table>");
    s
}

/// A POST form to `action` ("" posts back to the page) with the CSRF
/// field, around `inner`.
pub fn form(ctx: &Ctx, action: &str, inner: &str) -> String {
    let action = if action.is_empty() {
        String::new()
    } else {
        format!(" action=\"{}\"", esc(action))
    };
    format!("<form method=\"post\"{action}>{}{inner}</form>", ctx.csrf_field())
}

/// A multipart POST form (uploads).
pub fn upload_form(ctx: &Ctx, action: &str, inner: &str) -> String {
    let action = if action.is_empty() {
        String::new()
    } else {
        format!(" action=\"{}\"", esc(action))
    };
    format!(
        "<form method=\"post\" enctype=\"multipart/form-data\"{action}>{}{inner}</form>",
        ctx.csrf_field()
    )
}

/// The properties pattern: a form with the table and its buttons.
pub fn props(ctx: &Ctx, groups: &[PropGroup], bs: &[Button]) -> String {
    form(
        ctx,
        "",
        &format!("{}\n{}", props_table(ctx.product, groups, true), buttons(bs)),
    )
}

/// Text tabs: the current one bold, the others links (label, href).
pub fn tabs(tabs: &[(&str, String)], current: usize) -> String {
    let parts: Vec<String> = tabs
        .iter()
        .enumerate()
        .map(|(i, (label, href))| {
            if i == current {
                format!("<b>{}</b>", esc(label))
            } else {
                link(href, label)
            }
        })
        .collect();
    format!("<div class=\"tabs\">{}</div>", parts.join(" | "))
}

/// The wizard pattern: numbered steps (`current` is 1-based) beside the
/// body, the buttons under it.
pub fn wizard(ctx: &Ctx, steps: &[&str], current: usize, body: &str, bs: &[Button]) -> String {
    let mut s = String::from("<table class=\"wizard\" cellspacing=\"0\"><tr><td class=\"wsteps\">");
    for (i, st) in steps.iter().enumerate() {
        let n = i + 1;
        let cls = match n.cmp(&current) {
            std::cmp::Ordering::Less => "done",
            std::cmp::Ordering::Equal => "cur",
            std::cmp::Ordering::Greater => "",
        };
        let _ = write!(s, "<div class=\"wstep {cls}\">{n}. {}</div>", esc(st));
    }
    let _ = write!(s, "</td><td class=\"wbody\">{body}{}</td></tr></table>", buttons(bs));
    form(ctx, "", &s)
}

/// A text input.
pub fn input(name: &str, value: &str, size: u32) -> String {
    format!(
        "<input type=\"text\" name=\"{}\" value=\"{}\" size=\"{size}\">",
        esc(name),
        esc(value)
    )
}

/// A text input in Courier (paths, commands).
pub fn input_cmd(name: &str, value: &str, size: u32) -> String {
    format!(
        "<input type=\"text\" name=\"{}\" value=\"{}\" size=\"{size}\" class=\"cmd\">",
        esc(name),
        esc(value)
    )
}

/// A checkbox.
pub fn checkbox(name: &str, checked: bool) -> String {
    format!(
        "<input type=\"checkbox\" name=\"{}\"{}>",
        esc(name),
        if checked { " checked" } else { "" }
    )
}

/// A radio button.
pub fn radio(name: &str, value: &str, checked: bool) -> String {
    format!(
        "<input type=\"radio\" name=\"{}\" value=\"{}\"{}>",
        esc(name),
        esc(value),
        if checked { " checked" } else { "" }
    )
}

/// A select of `options`, `chosen` selected.
pub fn select(name: &str, options: &[&str], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for o in options {
        let sel = if *o == chosen { " selected" } else { "" };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

/// A `<span class="mono">`.
pub fn mono(s: &str) -> String {
    format!("<span class=\"mono\">{}</span>", esc(s))
}

/// Grey text.
pub fn grey(s: &str) -> String {
    format!("<span class=\"grey\">{}</span>", esc(s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::Kind;

    const P: Product = Product {
        wordmark: "QS/400 Test",
        crumb_root: "Test",
        prefix: "/t",
        nav: &[],
        system_label: "System",
    };

    #[test]
    fn rows_and_paging() {
        let m = message_row(&P, &Message::new(Kind::Done, "HTP8001", "a < b"));
        assert_eq!(
            m,
            "<table class=\"msg\"><tr><td class=\"done\"><a class=\"msgid\" href=\"/t/msg/HTP8001\" target=\"_blank\">HTP8001</a> - a &lt; b</td></tr></table>"
        );
        let p = Paging::from_query("9", "10", 25, vec![("sort".into(), "name".into())]);
        assert_eq!((p.page, p.pages(), p.range()), (3, 3, 20..25));
        let line = paging(&p);
        assert!(line.contains("<a href=\"?sort=name&amp;rows=10&amp;page=2\">&lt; Previous</a>"));
        assert!(line.contains("<span class=\"grey\">Next &gt;</span>"));
        assert!(line.contains("<option selected>10</option>"));
        let p = Paging::from_query("", "7", 0, Vec::new());
        assert_eq!((p.rows, p.pages(), p.range()), (25, 1, 0..0));
    }

    #[test]
    fn list_and_wizard() {
        let l = List {
            cols: vec![Col::plain("A"), Col::sorted("B", "b")],
            rows: vec![vec![Cell::text("1"), Cell::num("2")], vec![Cell::mono("x"), Cell::act("y".into())]],
            empty: "none".into(),
            keep: vec![("status".into(), "Running".into())],
        };
        let t = list_table(&l);
        assert!(t.contains("<th><a href=\"?status=Running&amp;sort=b\">B</a></th>"));
        assert!(t.contains("<tr class=\"alt\">\n<td class=\"mono\">x</td>"));
        let l = List {
            cols: vec![Col::plain("A")],
            empty: "No rows.".into(),
            ..List::default()
        };
        assert!(list_table(&l).contains("<td class=\"empty\" colspan=\"1\">No rows.</td>"));
        let ctx = Ctx {
            product: &P,
            system: "S".into(),
            user: "U".into(),
            csrf: "tok".into(),
        };
        let w = wizard(&ctx, &["One", "Two", "Three"], 2, "<p>b</p>", &[Button::new("next", "Next >")]);
        assert!(w.starts_with("<form method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"tok\">"));
        assert!(w.contains("<div class=\"wstep done\">1. One</div><div class=\"wstep cur\">2. Two</div><div class=\"wstep \">3. Three</div>"));
        assert!(w.contains("<input type=\"submit\" name=\"next\" value=\"Next &gt;\">"));
    }
}
