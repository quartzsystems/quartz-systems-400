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
        key: "instances",
        title: "Server instances",
        body: "<p>This page lists every HTTP server instance defined on the system. A running instance is a job in subsystem <span class=\"mono\">QHTTPSVR</span> named after the instance.</p>\n\
<h2>Actions</h2>\n<p><b>Start</b> starts the instance (STRTCPSVR SERVER(*HTTP) HTTPSVR(name)). <b>Stop</b> and <b>Restart</b> open a confirmation page where you choose a controlled or immediate end (ENDTCPSVR). <b>Manage</b> opens the instance's properties. <b>Delete</b> removes a stopped instance and its server root.</p>\n\
<h2>Columns</h2>\n<table class=\"props\" cellspacing=\"0\">\n<tr><th>Ports</th><td>The ports named by the instance's Listen directives.</td></tr>\n<tr><th>Job</th><td>The qualified job name <span class=\"mono\">number/user/name</span> of the running instance.</td></tr>\n<tr><th>Started</th><td>When the running instance's job started.</td></tr>\n</table>",
    },
    Topic {
        key: "confirm-instance",
        title: "Start, stop or restart an instance",
        body: "<p>This page confirms an action on an HTTP server instance before it is taken. Nothing happens until you press <b>OK</b>; <b>Cancel</b> returns to the Server instances page.</p>
<h2>How to end</h2>
<p><b>Controlled</b> stops accepting new connections and gives requests in progress up to the number of seconds you give, from 0 to 3600, to finish (ENDTCPSVR SERVER(*HTTP) with a controlled end). <b>Immediate</b> ends the job at once and closes every connection.</p>
<p>A restart ends the instance the way you choose and starts it again with the configuration file as it is now. The administration instance itself cannot be stopped or restarted from its own pages; use a 5250 session.</p>
<p>Starting and stopping instances needs the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
    },
    Topic {
        key: "confirm-delete",
        title: "Delete an instance",
        body: "<p>Deleting an instance removes its server root, with the configuration file, the content under it and every log file, and its entry in the instance registry. It cannot be undone.</p>
<p>A running instance is not deleted: stop it first. Deleting needs the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
    },
    Topic {
        key: "signon",
        title: "Sign on",
        body: "<p>Sign on with a QS/400 user profile and its password. The same rules as a 5250 sign-on apply: a disabled profile cannot sign on, an expired password must be changed from a 5250 session first, and too many wrong passwords disable the profile (system value <span class=\"mono\">QMAXSIGN</span>).</p>\n<p>HTTP Server Administration needs the <span class=\"mono\">*IOSYSCFG</span> special authority, or membership in the administrators group named in Global settings.</p>",
    },
    Topic {
        key: "error",
        title: "Messages",
        body: "<p>Every message has an identifier, such as <span class=\"mono\">HTP8001</span>, and a second-level text: the message, its cause and what to do about it. Select a message identifier anywhere in this product to see its second-level text.</p>",
    },
    Topic {
        key: "notauth",
        title: "Not authorized",
        body: "<p>Each action checks the same authority as the command behind it. When the check fails the action is not taken, the page names the authority that was missing, and the attempt is recorded in the audit journal as an <span class=\"mono\">AF</span> entry.</p>",
    },
];

/// The title and body of `key`; the contents for an unknown key.
pub fn topic(key: &str) -> (String, String) {
    if let Some(t) = TOPICS.iter().find(|t| t.key == key) {
        return (t.title.to_string(), t.body.to_string());
    }
    let mut body = String::from("<p>Help is available for these topics:</p>\n<ul>\n");
    for t in TOPICS {
        let _ = writeln!(
            body,
            "<li><a href=\"{}\">{}</a></li>",
            esc(&crate::PRODUCT.help(t.key)),
            esc(t.title)
        );
    }
    body.push_str("</ul>");
    ("Contents".into(), body)
}
