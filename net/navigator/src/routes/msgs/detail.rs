// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `/msgq/<lib>/<name>/<key>`: one message as `DSPMSG`'s detail shows
//! it (sender, second-level text) and, for an inquiry waiting for its
//! reply, the Reply form (`SNDRPYMSG`).

use std::fmt::Write as _;

use qsweb::html::esc;
use qsweb::http::Response;
use qsweb::msg::Kind;
use qsweb::partials::{Button, Prop, PropGroup, buttons, form, msgid, props_table};

use super::queue::{kind, text};
use super::{back, job_link, run};
use crate::env::msgs::QueuedMessage;
use crate::routes::Rq;
use crate::routes::common::{mono, quote};

/// The page, or the reply's POST.
pub fn page(rq: &Rq<'_>, lib: &str, name: &str, key: &str) -> Response {
    let (lib, name) = (lib.to_ascii_uppercase(), name.to_ascii_uppercase());
    let back = back(rq, &lib, &name);
    let Ok(ms) = rq.env().msgs.messages(rq.user(), &lib, &name) else {
        return rq.not_found(&format!("message queue {lib}/{name}"));
    };
    let Some(m) = ms.into_iter().find(|m| m.key.eq_ignore_ascii_case(key)) else {
        return rq.done(&back, rq.message(Kind::Error, "CPF2410", &[key]));
    };
    if rq.post() {
        let f = rq.form();
        if f.has("cancel") {
            return rq.redirect(&back);
        }
        let r = f.str("reply").trim();
        let rpy = if r.is_empty() { "*DFT".to_string() } else { quote(r) };
        return run(rq, &format!("SNDRPYMSG MSGKEY({}) MSGQ({lib}/{name}) RPY({rpy}) RMV(*NO)", m.key), &back);
    }
    let (d, t) = rq.stamp(m.sent);
    let id = if m.id.is_empty() { String::new() } else { msgid(rq.ctx.product, &m.id) };
    let mut rows = vec![
        Prop::new("Message ID", id),
        Prop::new("Severity", format!("{:02}", m.severity)),
        Prop::new("Type", kind(&m).into()),
        Prop::new("Date/time sent", format!("{d} {t}")),
        Prop::new("From job", job_link(rq, &m.job)),
        Prop::new("From program", mono(&m.program)),
        Prop::new("To queue", mono(&format!("{lib}/{name}"))),
    ];
    if let Some(r) = &m.reply {
        rows.push(Prop::new("Reply", mono(r)));
    }
    let groups = [PropGroup {
        title: Some("Message".into()),
        rows,
    }];
    let mut body = props_table(rq.ctx.product, &groups, true);
    let _ = write!(body, "<h2>Message</h2><pre class=\"out\">{}</pre>", esc(&text(&m)));
    if !m.cause.is_empty() {
        let _ = write!(body, "<h2>Cause</h2><p>{}</p>", commands_mono(&m.cause));
    }
    if !m.recovery.is_empty() {
        let _ = write!(body, "<h2>Recovery</h2><p>{}</p>", commands_mono(&m.recovery));
    }
    if m.waiting() {
        body.push_str(&reply_form(rq, &m));
    }
    let label = if m.id.is_empty() { format!("from {}", m.user) } else { m.id.clone() };
    let title = format!("Message {label}");
    let crumb = if back == "/mymsgs" { "My messages".to_string() } else { name.clone() };
    let desc = if m.waiting() {
        "Second-level text and reply for an inquiry message."
    } else {
        "Second-level text of a message."
    };
    let shown = if m.id.is_empty() { "Message".to_string() } else { m.id.clone() };
    super::page(rq, &[&crumb, &shown], &title, &title, Some(desc), body, "message", None)
}

/// The Reply form: an empty reply sends the default.
fn reply_form(rq: &Rq<'_>, m: &QueuedMessage) -> String {
    let valid = if m.replies.is_empty() { "*ANY" } else { m.replies.as_str() };
    let inner = format!(
        "<h2>Reply</h2><table class=\"props\"><tr><th>Reply</th><td><input type=\"text\" name=\"reply\" value=\"\" size=\"10\" class=\"cmd\"> &nbsp; Valid replies: {} &nbsp; Default: {}</td></tr></table>{}",
        mono(valid),
        mono(&m.default),
        buttons(&[Button::new("send", "Reply"), Button::new("cancel", "Cancel")])
    );
    form(&rq.ctx, "", &inner)
}

/// Whether `w` is a CL command name: 3 to 10 capitals and digits,
/// starting with a capital.
fn command_name(w: &str) -> bool {
    (3..=10).contains(&w.len())
        && w.starts_with(|c: char| c.is_ascii_uppercase())
        && w.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

/// Whether `w` is a `KEYWORD(value)` parameter, trailing punctuation
/// aside: the length of the parameter part.
fn parameter(w: &str) -> Option<usize> {
    let open = w.find('(')?;
    let kw = &w[..open];
    if kw.is_empty() || !kw.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
        return None;
    }
    let close = w.rfind(')')?;
    w[close + 1..].chars().all(|c| ".,;:".contains(c)).then_some(close + 1)
}

/// Text with the command strings in it (`DSPRCDLCK FILE(...) ...`) in
/// Courier, the rest escaped.
pub fn commands_mono(text: &str) -> String {
    let words: Vec<&str> = text.split(' ').collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        let mut j = i + 1;
        let mut end = None;
        if command_name(w) {
            while j < words.len() {
                match parameter(words[j]) {
                    Some(n) => {
                        end = Some((j, n));
                        j += 1;
                        if n < words[j - 1].len() {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
        if let Some((last, n)) = end {
            let mut cmd = words[i..last].join(" ");
            cmd.push(' ');
            cmd.push_str(&words[last][..n]);
            out.push(format!("{}{}", mono(&cmd), esc(&words[last][n..])));
            i = last + 1;
        } else {
            out.push(esc(w));
            i += 1;
        }
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::commands_mono;

    #[test]
    fn commands_in_text() {
        assert_eq!(
            commands_mono("the command DSPRCDLCK FILE(A/B) RCDNBR(4). Then"),
            "the command <span class=\"mono\">DSPRCDLCK FILE(A/B) RCDNBR(4)</span>. Then"
        );
        assert_eq!(commands_mono("Reply C (or R)."), "Reply C (or R).");
    }
}
