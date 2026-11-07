// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The web F4 (`/cmd/prompt?cmd=`): the command's definition as the
//! 5250 prompter shows it (keyword, prompt text, a select for
//! restricted values, a field per list element with `+` for one more,
//! the choices text), filled from the command string; Show command
//! string rebuilds the string from the fields, Run runs it, Save keeps
//! it in My commands. Additional parameters show after `F10` (`?more=1`).

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::{esc, query};
use qsweb::http::{Form, Response};

use qsweb::partials::{Cell, Col, List, list_table};

use super::parse::{elements, parse};
use super::{keyword, name_of};
use crate::env::cmd::{CmdDef, How, Param};
use crate::routes::Rq;
use crate::{PRODUCT, policy};

/// The breadcrumb.
const CRUMB: &[&str] = &["Commands", "Run command", "Prompt"];

/// A parameter's values: one for a single value, the elements of a list.
type Values = Vec<Vec<String>>;

/// The field name of parameter `p`, element `i` of a list.
fn field(p: &Param, i: usize) -> String {
    let k = p.keyword.to_ascii_lowercase();
    if p.max > 1 {
        format!("{k}{}", i + 1)
    } else {
        k
    }
}

/// The values the command string gives each parameter (keyword or
/// position), the default where it gives none.
fn from_string(def: &CmdDef, s: &str) -> Values {
    let parsed = parse(s);
    let mut out: Vec<Option<String>> = vec![None; def.params.len()];
    let mut next = 0;
    for (kw, v) in parsed.values {
        let at = if let Some(k) = kw {
            def.params.iter().position(|p| p.keyword == k)
        } else {
            while next < out.len() && out[next].is_some() {
                next += 1;
            }
            (next < out.len()).then_some(next)
        };
        if let Some(i) = at {
            out[i] = Some(v);
        }
    }
    def.params
        .iter()
        .zip(out)
        .map(|(p, v)| {
            let v = v.or_else(|| p.default.clone()).unwrap_or_default();
            if p.max > 1 { elements(&v) } else { vec![v] }
        })
        .collect()
}

/// The values the form posted.
fn from_form(def: &CmdDef, f: &Form) -> Values {
    def.params
        .iter()
        .map(|p| {
            if p.max > 1 {
                (0..usize::from(p.max))
                    .map(|i| f.str(&field(p, i)).trim().to_string())
                    .filter(|v| !v.is_empty())
                    .collect()
            } else {
                vec![f.str(&field(p, 0)).trim().to_string()]
            }
        })
        .collect()
}

/// The command string the fields make: every parameter with a value.
fn string_of(def: &CmdDef, values: &Values) -> String {
    let mut s = def.name.clone();
    for (p, v) in def.params.iter().zip(values) {
        let v: Vec<&str> = v
            .iter()
            .map(String::as_str)
            .filter(|x| !x.is_empty())
            .collect();
        if v.is_empty() {
            continue;
        }
        s.push(' ');
        s.push_str(&keyword(&p.keyword, &v.join(" ")));
    }
    s
}

/// One parameter's value control.
fn control(p: &Param, v: &[String], shown: &str, more: usize) -> String {
    if !p.values.is_empty() {
        let cur = v.first().map_or("", String::as_str);
        let mut s = format!("<select name=\"{}\">", field(p, 0));
        for (i, o) in p.values.iter().enumerate() {
            let sel = if i > 0 && o.eq_ignore_ascii_case(cur) {
                " selected"
            } else {
                ""
            };
            let _ = write!(s, "<option{sel}>{}</option>", esc(o));
        }
        s.push_str("</select>");
        return s;
    }
    let input = |i: usize, val: &str| {
        format!(
            "<input type=\"text\" name=\"{}\" value=\"{}\" size=\"{}\" class=\"cmd\">",
            field(p, i),
            esc(val),
            p.size
        )
    };
    if p.max <= 1 {
        return input(0, v.first().map_or("", String::as_str));
    }
    let max = usize::from(p.max);
    let n = v.len().max(more).saturating_add(1).clamp(1, max);
    let mut s = input(0, v.first().map_or("", String::as_str));
    if n < max {
        let href = format!(
            "{}{}",
            PRODUCT.url("/cmd/prompt"),
            query(&[
                ("cmd", shown),
                (&format!("n.{}", p.keyword), &(n + 1).to_string())
            ])
        );
        let _ = write!(s, " <a href=\"{}\">+</a>", esc(&href));
    }
    let _ = write!(s, " <span class=\"small grey\">(list, up to {max})</span>");
    for i in 1..n {
        s.push_str("<br>");
        s.push_str(&input(i, v.get(i).map_or("", String::as_str)));
    }

    s
}

/// `GET /cmd/prompt?cmd=`; `POST` from Run command opens it for the
/// typed command, from the page itself Run, Show command string, Save.
#[allow(clippy::too_many_lines)]
pub fn page(rq: &Rq<'_>) -> Response {
    if let Err(r) = rq.policy(policy::CMD_PROMPT, CRUMB) {
        return r;
    }
    let f = rq.form();
    if rq.post() && f.has("prompt") {
        // Prompt from Run command: the page for the typed string.
        return rq.redirect(&format!(
            "/cmd/prompt{}",
            query(&[("cmd", f.str("cmd").trim())])
        ));
    }
    let q = rq.query();
    let given = q.str("cmd").trim().to_string();
    let name = parse(&given).name;
    if name.is_empty() {
        return rq.redirect("/cmd");
    }
    let def = match rq.env().cmd.describe(rq.user(), &name) {
        Ok(d) => d,
        Err(m) => return rq.done(&format!("/cmd{}", query(&[("c", &given)])), m),
    };
    let values = if rq.post() {
        from_form(&def, &f)
    } else {
        from_string(&def, &given)
    };
    let string = string_of(&def, &values);
    if rq.post() {
        if f.has("run") {
            if let Err(r) = rq.policy(policy::CMD_RUN, CRUMB) {
                return r;
            }
            let next = super::run(rq, &string, How::Interactive, "", false);
            let back = format!("/cmd{}", query(&[("c", &string)]));
            return match next {
                Some(m) => rq.done(&back, m),
                None => rq.redirect(&back),
            };
        }
        if f.has("save") {
            return super::save(rq, &string, How::Interactive, None);
        }
    }
    let more = q.str("more") == "1";
    let shown: Vec<(usize, &Param)> = def
        .params
        .iter()
        .enumerate()
        .filter(|(_, p)| more || !p.additional)
        .collect();
    let rows = shown
        .iter()
        .map(|(i, p)| {
            let extra: usize = q
                .str(&format!("n.{}", p.keyword))
                .parse::<usize>()
                .map_or(0, |n| n.saturating_sub(1));
            let mut kw = esc(&p.keyword);
            if p.required {
                kw.push_str(" <span class=\"req\">*</span>");
            }
            vec![
                Cell::mono_html(kw),
                Cell::text(&p.prompt),
                Cell::html(control(p, &values[*i], &string, extra)),
                Cell {
                    class: Some("small grey"),
                    html: esc(&p.choices),
                },
            ]
        })
        .collect();
    let mut body = format!(
        "<p class=\"small\">Command: <span class=\"mono\">{}</span> &nbsp; <a href=\"{}\" target=\"_blank\">Command help</a></p><form method=\"post\">{}",
        esc(&given),
        esc(&PRODUCT.help("prompt")),
        rq.ctx.csrf_field()
    );
    body.push_str(&list_table(&List {
        cols: ["Keyword", "Parameter", "Value", "Choices"]
            .iter()
            .map(|c| Col::plain(c))
            .collect(),
        rows,
        empty: "The command has no parameters.".into(),
        keep: Vec::new(),
    }));
    if !more && def.params.iter().any(|p| p.additional) {
        let href = format!(
            "{}{}",
            PRODUCT.url("/cmd/prompt"),
            query(&[("cmd", &string), ("more", "1")])
        );
        let _ = write!(
            body,
            "<p class=\"small\"><a href=\"{}\">Additional parameters</a></p>",
            esc(&href)
        );
    }
    body.push_str("<div class=\"buttons\">");
    if rq.allowed(policy::CMD_RUN) {
        body.push_str("<input type=\"submit\" name=\"run\" value=\"Run\">");
    }
    body.push_str("<input type=\"submit\" name=\"show\" value=\"Show command string\">");
    if rq.allowed(policy::CMD_SAVE) {
        body.push_str("<input type=\"submit\" name=\"save\" value=\"Save to My commands\">");
    }
    let _ = write!(
        body,
        "<input type=\"submit\" value=\"Cancel\" formaction=\"{}\"></div></form><p class=\"small\">Command string: <span class=\"mono\">{}</span></p>",
        esc(&PRODUCT.url("/cmd")),
        esc(&string)
    );
    let title = format!("Prompt: {} ({})", def.text, name_of(&def.name));
    rq.page(Page {
        title: title.clone(),
        crumb: CRUMB.iter().map(ToString::to_string).collect(),
        h1: title,
        body,
        help: "prompt".into(),
        nav: None,
        ..Page::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> CmdDef {
        let p = |k: &str, max: u16, dft: Option<&str>| Param {
            keyword: k.into(),
            max,
            default: dft.map(Into::into),
            size: 10,
            ..Param::default()
        };
        CmdDef {
            name: "WRKX".into(),
            params: vec![p("A", 1, Some("*ALL")), p("L", 5, None), p("T", 1, None)],
            ..CmdDef::default()
        }
    }

    #[test]
    fn strings() {
        let d = def();
        let v = from_string(&d, "WRKX X L(P Q) T('a b')");
        assert_eq!(
            v,
            [
                vec!["X".to_string()],
                vec!["P".into(), "Q".into()],
                vec!["'a b'".into()]
            ]
        );
        assert_eq!(string_of(&d, &v), "WRKX A(X) L(P Q) T('a b')");
        assert_eq!(string_of(&d, &from_string(&d, "WRKX")), "WRKX A(*ALL)");
        assert_eq!(keyword("T", "it's"), "T('it''s')");
    }
}
