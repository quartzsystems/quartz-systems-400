// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Help windows: one topic per key, Quartz-written. Each area adds its
//! pages' topics to [`TOPICS`].

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
        title: "My home page",
        body: "<p>Where Access for Web starts: when you last signed on and from where, how many messages, spooled files and active jobs you have, and a link to each part of the product you may use. Preferences choose a compact layout or leave the counts out.</p>",
    },
    Topic {
        key: "signon",
        title: "Signing on",
        body: "<p>Sign on with your user profile and password on this system. The system's sign-on rules apply: a profile that is disabled, or whose password is wrong too many times in a row, cannot sign on. A password that has expired must be changed before anything else. An administrator can deny signing on to Access for Web with a policy.</p>",
    },
    Topic {
        key: "password",
        title: "Change password",
        body: "<p>Type your current password, then the new one twice. The system checks the new password against its password rules and answers with its own message when it refuses it; nothing changes then. The new password is the one for every way you sign on to this system.</p>",
    },
    Topic {
        key: "policies",
        title: "Policies",
        body: "<p>Policies allow or deny each function of Access for Web to a user profile, a group profile or <span class=\"mono\">*PUBLIC</span>. A function is checked at the user profile, then at its groups, then at <span class=\"mono\">*PUBLIC</span>: a Deny at any level denies it, and a function nobody has set is allowed. <b>Set by</b> says which level decided.</p>\n<p>Policies come on top of object authority: a function a policy allows still needs the authority of the command behind it. Changing policies needs special authority <span class=\"mono\">*SECADM</span>.</p>\n<h2>Changing</h2>\n<p>Choose the profile and press <b>Go</b>, set the functions and press <b>Apply</b> or <b>OK</b>. A setting that is the same as the levels below it is not kept for the profile. <b>Copy policies from...</b> gives the profile another profile's settings.</p>",
    },
    Topic {
        key: "preferences",
        title: "Preferences",
        body: "<p>How Access for Web shows things to you: rows per page, date and time formats, the time zone, the database defaults, the 5250 defaults, file transfers, printer output and the home page. <b>Restore defaults</b> sets every preference back to the value the product ships with.</p>",
    },
    Topic {
        key: "notauth",
        title: "Not authorized",
        body: "<p>The function you chose needs an authority your user profile does not have, or an Access for Web policy denies it. The page names the function, the object, the authority required and the policy. The attempt is written to the audit journal.</p>",
    },
    Topic {
        key: "error",
        title: "Messages",
        body: "<p>A message identifier such as <span class=\"mono\">CPF1321</span> links to its full text: the message, its cause and what to do about it.</p>",
    },
    Topic {
        key: "confirm",
        title: "Confirmations",
        body: "<p>A confirmation names what the command acts on and shows the command itself. <b>OK</b> runs it with your authority; <b>Cancel</b> returns without running it.</p>",
    },
    Topic {
        key: "notavail",
        title: "Not available",
        body: "<p>This part of Access for Web is not available on this system yet. Use a 5250 display or QS/400 Access for Desktop for it.</p>",
    },
];

/// The topic of `key` (title, body HTML); the contents for any other.
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
