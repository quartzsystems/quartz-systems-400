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
    Topic {
        key: "5250",
        title: "5250 session",
        body: "<p>Your saved 5250 sessions and Start 5250 display. <b>Start</b> opens a saved session's display; <b>Properties</b> changes its settings; <b>Download .qsd</b> saves it as a session file QS/400 Access for Desktop opens too; <b>Delete</b> removes it after asking.</p>\n<p>The form below starts a display with the settings you give: the system, the screen size, the CCSID, a device name (<span class=\"mono\">*DFT</span> lets the system name it), Telnet over TLS on port 992, the colour map, the keypad, hotspots and a keyboard map. <b>Sign on automatically</b> signs your profile on with a one-time ticket instead of a password; it needs TLS. <b>Save session</b> keeps the settings under the session name; <b>Open a session file</b> takes a <span class=\"mono\">.qsd</span> file from the desktop product.</p>",
    },
    Topic {
        key: "5250props",
        title: "Session properties",
        body: "<p>A saved session's settings. The system, screen size, CCSID, device name and TLS take effect when the session next connects; the colour map, hotspots and keypad at once.</p>",
    },
    Topic {
        key: "5250verify",
        title: "Verify connection",
        body: "<p>Checks what a 5250 session needs from the system, one at a time: the name lookup, the Telnet port, the Telnet TLS port with its certificate, the QAP port, its TLS handshake and certificate, and the QAP Hello. A test sign-on, when you ask for one, signs your profile on and off again; it counts toward the system's sign-on attempts and is audited.</p>",
    },
    Topic {
        key: "5250session",
        title: "5250 display",
        body: "<p>The system's 5250 display in the page, drawn with the QS/400 console's font and colours. Type on the screen as at a display station; the keyboard map says which key does what. Function-key labels such as <span class=\"mono\">F3=Exit</span>, and <span class=\"mono\">More...</span> and <span class=\"mono\">Bottom</span>, act when clicked. Drag to select a rectangle; <b>Copy</b> copies it, <b>Paste</b> types the clipboard at the cursor.</p>\n<p>Without script the page still works: the keypad and toolbar buttons send their keys and the picture shows the last screen; <b>Edit</b> &gt; <b>Paste</b> types text at the cursor and <b>Copy</b> shows the screen's text. <b>Print Screen</b> makes a one-page PDF of the screen.</p>",
    },
    Topic {
        key: "5250keymap",
        title: "Keyboard map",
        body: "<p>Which key of your keyboard does which 5250 function. Change a key's function in its row, or give another key one with <b>Add a key</b>. <b>Load keyboard map</b> takes a <span class=\"mono\">.qkm</span> file from QS/400 Access for Desktop; <b>Export</b> saves this map as one. <b>Restore defaults</b> sets the map back to the product's.</p>",
    },
    Topic {
        key: "5250macros",
        title: "Macros",
        body: "<p>A macro replays keys and typing and waits for the screens between them. <b>Record...</b> records in the session's display until you press <b>Stop</b> here; what is typed into a field that does not display is kept as a prompt, never as the text. Edit a macro's steps one a line: <span class=\"mono\">type \"TEXT\"</span>, <span class=\"mono\">key NAME</span>, <span class=\"mono\">wait screen \"TEXT\" at ROW,COL</span>, <span class=\"mono\">wait unlock</span>, <span class=\"mono\">pause MS</span>, <span class=\"mono\">prompt \"LABEL\"</span>; a <span class=\"mono\">;</span> starts a comment. Macro files (<span class=\"mono\">.qsm</span>) are QS/400 Access for Desktop's.</p>",
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
