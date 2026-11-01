// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help windows: one topic per key, Quartz-written.

use std::fmt::Write as _;

use qsweb::html::esc;

/// A help topic: key, title, body HTML.
pub struct Topic {
    /// The key in `/help/<key>`.
    pub key: &'static str,
    /// The title.
    pub title: &'static str,
    /// The body, HTML.
    pub body: &'static str,
}

/// Every topic.
pub const TOPICS: &[Topic] = &[
    Topic {
        key: "home",
        title: "System status",
        body: "<p>An overview of the system: its identity, storage, activity and what needs attention. Every value links to the page that shows it in detail.</p>\n\
<h2>Elapsed interval</h2>\n<p>CPU use is measured from the previous time you displayed this page to now, as on the 5250 system status display. <b>Refresh</b> starts a new interval.</p>\n\
<h2>Auto-refresh</h2>\n<p>Choose a number of seconds and press <b>Apply</b> to reload the page by itself; choose <b>Off</b> to stop.</p>",
    },
    Topic {
        key: "notauth",
        title: "Not authorized",
        body: "<p>The function you chose needs an authority your user profile does not have. The page names the function, the object and the authority required. The attempt is written to the audit journal.</p>\n<p>Ask your security officer to grant the authority, or sign on with a profile that has it.</p>",
    },
    Topic {
        key: "error",
        title: "Messages",
        body: "<p>A message identifier such as <span class=\"mono\">CPF1321</span> links to its full text: the message, its cause and what to do about it.</p>",
    },
];

/// The title and body of `key`; an unknown key gives the contents.
pub fn topic(key: &str) -> (String, String) {
    if let Some(t) = TOPICS.iter().find(|t| t.key == key) {
        return (t.title.to_string(), t.body.to_string());
    }
    let mut body = String::from("<p>Help is available for these pages:</p>\n<ul>\n");
    for t in TOPICS {
        let _ = writeln!(body, "<li><a href=\"{}\">{}</a></li>", esc(&crate::PRODUCT.help(t.key)), esc(t.title));
    }
    body.push_str("</ul>");
    ("Contents".into(), body)
}
