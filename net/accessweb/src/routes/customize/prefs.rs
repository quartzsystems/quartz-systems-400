// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Preferences (`/preferences?tab=`): General shows every common
//! group, each other tab its own. A tab's post changes only its fields;
//! policy "Change own preferences" gates the changes.

use std::fmt::Write as _;

use qsweb::frame::Page;
use qsweb::html::esc;
use qsweb::http::{Form, Response};
use qsweb::msg::Kind;
use qsweb::partials::{Button, Prop, PropGroup, props, tabs};

use super::GROUP;
use crate::prefs::{self as p, Prefs};
use crate::routes::Rq;
use crate::{PRODUCT, msgs, policy};

const CRUMB: &[&str] = &[GROUP, "Preferences"];

/// The groups of fields.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Grp {
    General,
    Database,
    S5250,
    Files,
    Print,
    Home,
}

impl Grp {
    fn title(self) -> &'static str {
        match self {
            Grp::General => "General",
            Grp::Database => "Database",
            Grp::S5250 => "5250",
            Grp::Files => "Files",
            Grp::Print => "Print",
            Grp::Home => "Home page",
        }
    }

    /// The fields the group's form posts.
    fn keys(self) -> &'static [&'static str] {
        match self {
            Grp::General => &[
                p::ROWS,
                p::DATE,
                p::TIME,
                p::ZONE,
                p::LANG,
                p::HELP,
                p::CONFIRM_DELETE,
                p::CONFIRM_END,
                p::CONFIRM_HOLD,
            ],
            Grp::Database => &[
                p::SCHEMA,
                p::NAMING,
                p::OUTPUT,
                p::MAX_ROWS,
                p::SYSTEM_NAMES,
            ],
            Grp::S5250 => &[p::SCREEN, p::COLOURS, p::KEYPAD, p::FONT, p::CURSOR],
            Grp::Files => &[p::EXISTING, p::TRANSFER, p::ENDINGS],
            Grp::Print => &[p::PRINT_OPEN],
            Grp::Home => &[p::LAYOUT, p::COUNTS],
        }
    }
}

/// The tabs: label, `?tab=` value, the groups shown.
const TABS: &[(&str, &str, &[Grp])] = &[
    (
        "General",
        "",
        &[Grp::General, Grp::Database, Grp::S5250, Grp::Home],
    ),
    ("Database", "database", &[Grp::Database]),
    ("5250", "5250", &[Grp::S5250]),
    ("Files", "files", &[Grp::Files]),
    ("Print", "print", &[Grp::Print]),
    ("Home page", "home", &[Grp::Home]),
];

/// The checkboxes: their keys.
const BOXES: &[&str] = &[
    p::CONFIRM_DELETE,
    p::CONFIRM_END,
    p::CONFIRM_HOLD,
    p::SYSTEM_NAMES,
    p::KEYPAD,
    p::COUNTS,
];

/// A select; the first option is the default and is not marked.
fn pick(name: &str, options: &[String], chosen: &str) -> String {
    let mut s = format!("<select name=\"{}\">", esc(name));
    for (i, o) in options.iter().enumerate() {
        let sel = if i > 0 && o == chosen {
            " selected"
        } else {
            ""
        };
        let _ = write!(s, "<option{sel}>{}</option>", esc(o));
    }
    s.push_str("</select>");
    s
}

fn picks(name: &str, options: &[&str], chosen: &str) -> String {
    pick(
        name,
        &options.iter().map(ToString::to_string).collect::<Vec<_>>(),
        chosen,
    )
}

/// Radio buttons, each followed by its label.
fn radios(name: &str, choices: &[(&str, &str)], chosen: &str) -> String {
    choices
        .iter()
        .map(|(v, l)| {
            format!(
                "<input type=\"radio\" name=\"{name}\" value=\"{v}\"{}> {}",
                if *v == chosen { " checked" } else { "" },
                esc(l)
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn check(name: &str, on: bool) -> String {
    qsweb::partials::checkbox(name, on)
}

/// The time zone choices: the system's, UTC.
fn zones(rq: &Rq<'_>) -> Vec<String> {
    vec![
        format!("{} ({})", p::ZONE_SYSTEM, rq.env().system.zone_label()),
        p::ZONE_UTC.into(),
    ]
}

#[allow(clippy::too_many_lines)]
fn group(rq: &Rq<'_>, g: Grp, v: &Prefs) -> PropGroup {
    let rows = match g {
        Grp::General => {
            let z = zones(rq);
            let zone = if v.get(p::ZONE) == p::ZONE_UTC {
                z[1].clone()
            } else {
                z[0].clone()
            };
            vec![
                Prop::new(
                    "Rows per page",
                    picks(p::ROWS, p::ROW_CHOICES, &v.get(p::ROWS)),
                ),
                Prop::new(
                    "Date format",
                    picks(p::DATE, p::DATE_FORMATS, &v.get(p::DATE)),
                ),
                Prop::new(
                    "Time format",
                    picks(p::TIME, p::TIME_FORMATS, &v.get(p::TIME)),
                ),
                Prop::new("Time zone", pick(p::ZONE, &z, &zone)),
                Prop::new("Language", picks(p::LANG, p::LANGUAGES, &v.get(p::LANG))),
                Prop::new(
                    "Open help in",
                    radios(
                        p::HELP,
                        &[("win", "New window"), ("same", "Same window")],
                        &v.get(p::HELP),
                    ),
                ),
                Prop::new(
                    "Confirm before",
                    format!(
                        "{} Deleting {} Ending jobs {} Holding / releasing",
                        check(p::CONFIRM_DELETE, v.on(p::CONFIRM_DELETE)),
                        check(p::CONFIRM_END, v.on(p::CONFIRM_END)),
                        check(p::CONFIRM_HOLD, v.on(p::CONFIRM_HOLD))
                    ),
                ),
            ]
        }
        Grp::Database => vec![
            Prop::new(
                "Default schema",
                pick(p::SCHEMA, &v.schemas, &v.get(p::SCHEMA)),
            ),
            Prop::new(
                "Naming convention",
                radios(
                    p::NAMING,
                    &[
                        ("sql", "*SQL (schema.table)"),
                        ("sys", "*SYS (library/file)"),
                    ],
                    &v.get(p::NAMING),
                ),
            ),
            Prop::new(
                "Default output",
                picks(p::OUTPUT, p::OUTPUTS, &v.get(p::OUTPUT)),
            ),
            Prop::new(
                "Maximum rows returned",
                qsweb::partials::input(p::MAX_ROWS, &v.get(p::MAX_ROWS), 6),
            ),
            Prop::new(
                "Show system column names",
                check(p::SYSTEM_NAMES, v.on(p::SYSTEM_NAMES)),
            ),
        ],
        Grp::S5250 => vec![
            Prop::new(
                "Default screen size",
                radios(
                    p::SCREEN,
                    &[("24", "24 x 80"), ("27", "27 x 132")],
                    &v.get(p::SCREEN),
                ),
            ),
            Prop::new(
                "Colours",
                radios(
                    p::COLOURS,
                    &[("g", "Green on black"), ("w", "Black on white")],
                    &v.get(p::COLOURS),
                ),
            ),
            Prop::new("Show keypad", check(p::KEYPAD, v.on(p::KEYPAD))),
            Prop::new("Font size", picks(p::FONT, p::FONTS, &v.get(p::FONT))),
            Prop::new("Cursor", picks(p::CURSOR, p::CURSORS, &v.get(p::CURSOR))),
        ],
        Grp::Files => vec![
            Prop::new(
                "When a file exists",
                picks(p::EXISTING, p::EXISTING_CHOICES, &v.get(p::EXISTING)),
            ),
            Prop::new(
                "Transfer files as",
                picks(p::TRANSFER, p::TRANSFERS, &v.get(p::TRANSFER)),
            ),
            Prop::new(
                "Text line endings",
                picks(p::ENDINGS, p::ENDING_CHOICES, &v.get(p::ENDINGS)),
            ),
        ],
        Grp::Print => vec![Prop::new(
            "Open printer output as",
            picks(p::PRINT_OPEN, p::PRINT_CHOICES, &v.get(p::PRINT_OPEN)),
        )],
        Grp::Home => vec![
            Prop::new(
                "Layout",
                radios(
                    p::LAYOUT,
                    &[
                        ("list", "List with descriptions"),
                        ("compact", "Compact links only"),
                    ],
                    &v.get(p::LAYOUT),
                ),
            ),
            Prop::new(
                "Show at sign-on",
                format!(
                    "{} Message and spooled file counts",
                    check(p::COUNTS, v.on(p::COUNTS))
                ),
            ),
        ],
    };
    PropGroup {
        title: Some(g.title().into()),
        rows,
    }
}

/// The tab of `?tab=`.
fn tab_of(rq: &Rq<'_>) -> usize {
    let t = rq.query().get("tab").unwrap_or("").to_string();
    TABS.iter().position(|x| x.1 == t).unwrap_or(0)
}

fn url(tab: usize) -> String {
    match TABS[tab].1 {
        "" => "/preferences".into(),
        t => format!("/preferences?tab={t}"),
    }
}

fn show(rq: &Rq<'_>, tab: usize, v: &Prefs, messages: Vec<qsweb::msg::Message>) -> Response {
    let tablist: Vec<(&str, String)> = TABS
        .iter()
        .enumerate()
        .map(|(i, t)| (t.0, PRODUCT.url(&url(i))))
        .collect();
    let groups: Vec<PropGroup> = TABS[tab].2.iter().map(|g| group(rq, *g, v)).collect();
    let mut bs = vec![
        Button::new("apply", "Apply"),
        Button::new("ok", "OK"),
        Button::new("cancel", "Cancel"),
        Button::new("restore_defaults", "Restore defaults"),
    ];
    if !rq.allowed(policy::PREFS_CHANGE) {
        for b in &mut bs {
            if b.name.as_deref() != Some("cancel") {
                b.disabled = true;
            }
        }
    }
    let body = format!("{}{}", tabs(&tablist, tab), props(&rq.ctx, &groups, &bs));
    rq.page(Page {
        title: "Preferences".into(),
        crumb: CRUMB.iter().map(ToString::to_string).collect(),
        h1: "Preferences".into(),
        desc: Some(format!("Preferences for user {}.", rq.user())),
        messages,
        body,
        help: "preferences".into(),
        nav: Some((GROUP, Some("/preferences"))),
        ..Page::default()
    })
}

/// Take `g`'s posted fields into `v`; a refused value's message.
fn take(rq: &Rq<'_>, g: Grp, f: &Form, v: &mut Prefs) -> Result<(), qsweb::msg::Message> {
    for key in g.keys() {
        let posted = f.str(key).trim().to_string();
        let value = if BOXES.contains(key) {
            if f.has(key) {
                p::ON.to_string()
            } else {
                String::new()
            }
        } else if *key == p::ZONE {
            if posted == p::ZONE_UTC {
                p::ZONE_UTC.into()
            } else {
                p::ZONE_SYSTEM.into()
            }
        } else {
            posted
        };
        let ok = match *key {
            p::ROWS => p::ROW_CHOICES.contains(&value.as_str()),
            p::DATE => p::DATE_FORMATS.contains(&value.as_str()),
            p::TIME => p::TIME_FORMATS.contains(&value.as_str()),
            p::LANG => p::LANGUAGES.contains(&value.as_str()),
            p::HELP => ["win", "same"].contains(&value.as_str()),
            p::SCHEMA => v.schemas.contains(&value) || (v.schemas.is_empty() && value.is_empty()),
            p::NAMING => ["sql", "sys"].contains(&value.as_str()),
            p::OUTPUT => p::OUTPUTS.contains(&value.as_str()),
            p::MAX_ROWS => value
                .parse::<u32>()
                .is_ok_and(|n| (1..=100_000).contains(&n)),
            p::SCREEN => ["24", "27"].contains(&value.as_str()),
            p::COLOURS => ["g", "w"].contains(&value.as_str()),
            p::FONT => p::FONTS.contains(&value.as_str()),
            p::CURSOR => p::CURSORS.contains(&value.as_str()),
            p::EXISTING => p::EXISTING_CHOICES.contains(&value.as_str()),
            p::TRANSFER => p::TRANSFERS.contains(&value.as_str()),
            p::ENDINGS => p::ENDING_CHOICES.contains(&value.as_str()),
            p::PRINT_OPEN => p::PRINT_CHOICES.contains(&value.as_str()),
            p::LAYOUT => ["list", "compact"].contains(&value.as_str()),
            _ => true,
        };
        if !ok {
            let label = group(rq, g, v)
                .rows
                .iter()
                .find(|r| r.value.contains(&format!("name=\"{key}\"")))
                .map_or_else(|| (*key).to_string(), |r| r.label.clone());
            return Err(rq.message(Kind::Error, msgs::BAD_VALUE, &[&value, &label]));
        }
        v.stored.insert((*key).to_string(), value);
    }
    Ok(())
}

/// `GET /preferences` shows a tab; `POST` changes its fields.
pub fn page(rq: &Rq<'_>) -> Response {
    let tab = tab_of(rq);
    let mut v = rq.prefs();
    if !rq.post() {
        return show(rq, tab, &v, Vec::new());
    }
    let f = rq.form();
    if f.has("cancel") {
        return rq.redirect("/home");
    }
    if let Err(r) = rq.policy(policy::PREFS_CHANGE, CRUMB) {
        return r;
    }
    let m = if f.has("restore_defaults") {
        v.stored.retain(|k, _| k.starts_with('_'));
        rq.message(Kind::Done, msgs::PREFS_RESET, &[rq.user()])
    } else {
        for g in TABS[tab].2 {
            if let Err(m) = take(rq, *g, &f, &mut v) {
                return show(rq, tab, &v, vec![m]);
            }
        }
        rq.message(Kind::Done, msgs::PREFS_CHANGED, &[rq.user()])
    };
    if let Err(e) = rq.env().prefs.put(rq.user(), &v.to_store()) {
        return show(rq, tab, &v, vec![e]);
    }
    if f.has("ok") {
        return rq.done("/home", m);
    }
    rq.done(&url(tab), m)
}
