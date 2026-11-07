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
        key: "db-tables",
        title: "Tables",
        body: "<p>The tables, views, aliases and files of a schema, as the catalog describes them. Type a schema pattern (<span class=\"mono\">%</span> and <span class=\"mono\">_</span> as in SQL, or a name ending in <span class=\"mono\">*</span>), choose the schema and the kind of object, and press <b>Go</b>. From each one you can view its rows, see its columns, insert a record, copy its rows to a file or a file into it, and show its qualified name to copy. The filter stays while you are signed on.</p>",
    },
    Topic {
        key: "db-view",
        title: "Viewing a table",
        body: "<p>The rows of a table, 25 a page, at most the maximum rows your preferences set. <b>Find</b> shows the rows whose column holds the text, in any case; <b>Sort by</b> or a column heading orders them. <b>Edit</b> and <b>Delete</b> change one row, as your user profile, when the table has a key. <b>Output</b> other than HTML downloads every row the page selects as a file.</p>",
    },
    Topic {
        key: "db-record",
        title: "Inserting and editing records",
        body: "<p>One field per column, its type in grey. A red <span class=\"req\">*</span> marks a column that must have a value. Tick <b>NULL</b> to leave a value out; a long text left empty is NULL when the column allows it. Dates are typed in the date format of your preferences. Values the system makes, such as a change timestamp, are left to it. The system checks every value and answers with its own message when it refuses one.</p>",
    },
    Topic {
        key: "db-sql",
        title: "Run SQL",
        body: "<p>A script of SQL statements separated by <span class=\"mono\">;</span>, run in a connection of the page's own as your user profile. <b>Run statement</b> runs the one at the cursor, <b>Run all</b> every one, stopping at the first error when <b>Stop at first error</b> is ticked. Under a commit level other than None, changes stay uncommitted until <b>Commit</b> or <b>Rollback</b>, and the options cannot change meanwhile.</p>
<p>Each result has a tab; a query shows the first rows up to your maximum, and <b>Fetch more</b> or <b>Fetch all</b> reads the rest. <b>Download as</b> writes every row of the result shown. <b>Open .sql</b> and <b>Save as .sql</b> read and write script files; the history keeps your last 100 statements.</p>",
    },
    Topic {
        key: "db-requests",
        title: "My requests",
        body: "<p>Saved SQL scripts and copy requests. <b>Run</b> runs a script that only reads at once, and opens any other for you to run; a copy request opens its copy page. <b>Download</b> gives the <span class=\"mono\">.sql</span> or <span class=\"mono\">.qdx</span> file, which QS/400 Access for Desktop opens too, and a file of either kind can be imported. <b>Share</b> lists a request for a group or everyone; only its owner can change it.</p>",
    },
    Topic {
        key: "db-copy",
        title: "Copying data",
        body: "<p><b>Copy data to table</b> reads a CSV, TSV, XLSX, ODS or fixed-width file and writes its rows to a table: added to it, replacing every row, or updating the rows whose keys match. Map each column of the file to a column of the table, or skip it; <b>Preview 10 rows</b> shows the values as the table takes them. <b>Copy data from table</b> writes a table's or a query's rows to a file. Decimals are moved exactly. A copy runs as your user profile in a connection of its own, with a progress page and <b>Cancel</b>; a <span class=\"mono\">.qdx</span> request keeps it to run again here or in QS/400 Access for Desktop.</p>",
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
    // Files and Print (steps 13-15).
    Topic {
        key: "files",
        title: "Browse files",
        body: "<p>The integrated file system as your user profile sees it. The tree on the left opens the folders on the way to the one shown; <b>Path</b> and <b>Pattern</b> (<span class=\"mono\">*</span> any run of characters, <span class=\"mono\">?</span> one) choose what the list shows, 500 entries a page. A link shows its target; one whose target is gone is red.</p>\n<p><b>Cut</b> and <b>Copy</b> remember entries; <b>Paste</b> moves or copies them into the folder shown, on the system. <b>Delete</b> asks first and says how many objects go with a folder.</p>\n<h2>Transfers</h2>\n<p><b>Upload</b> takes one or several files from your workstation, as binary or as text converted to the CCSID given. If a file exists, <b>Ask</b> keeps the upload waiting in the transfer queue for Replace or Skip. <b>Download options</b> choose binary, or text converted from the file's CCSID to UTF-8. An interrupted download resumes where it stopped when the browser asks for the rest; the queue's Retry starts it again.</p>",
    },
    Topic {
        key: "file-props",
        title: "File properties",
        body: "<p>What the system knows of the entry: its type, size, CCSID, times and change count, its owner and primary group. The owner, or a user with <span class=\"mono\">*ALLOBJ</span>, can change the data authority (owner, group and public, as <span class=\"mono\">rwxr-x---</span>), <span class=\"mono\">*PUBLIC</span>'s authority and the authorization list.</p>",
    },
    Topic {
        key: "file-edit",
        title: "Edit file",
        body: "<p>A text file in its own CCSID and line endings. <b>Save</b> writes it back; if it changed on the system since you opened it, you are asked before your text replaces it. <b>Save as...</b> writes it under another name, <b>Reset</b> reads it again. Binary files and files over 8 MB open read-only; files over 64 MB do not open: download them instead.</p>",
    },
    Topic {
        key: "my-files",
        title: "My files",
        body: "<p>Your home directory and the shares you are authorized to, each with the space used under it. <b>Browse</b> opens it in Browse files. A policy can limit you to your home directory.</p>",
    },
    Topic {
        key: "printer-output",
        title: "Printer output",
        body: "<p>Your spooled files, or with the authority and the policy other users' too. The filter chooses the user, the output queue (or type one), the status, the form type and the user data; <b>Refresh</b> reads the list again and <b>Auto-refresh</b> does so every 15, 30 or 60 seconds. A column heading sorts the page shown.</p>\n<p><b>Open</b> shows a file a page at a time; <b>PDF</b> and <b>Text</b> give all of it. Select files to <b>Hold</b> them (now, or once the page being printed is done), <b>Release</b>, <b>Delete</b> or <b>Move</b> them to another output queue, at its end or its top, or to save one as PDF or text. When an action on several files is refused for some, each file's result is listed.</p>",
    },
    Topic {
        key: "splf-props",
        title: "Spooled file properties",
        body: "<p>Every attribute of the spooled file: its job and number, status, queue, pages and copies, form type and priority, when it was made, its device type and page layout, and the writer printing it.</p>",
    },
    Topic {
        key: "splf-view",
        title: "Spooled file",
        body: "<p>One page of the spooled file as it prints, column for column. Go to a page by its number, step with Previous and Next, or Find the next page holding some text. <b>View as PDF</b> and <b>Download text</b> give the whole file.</p>",
    },
    Topic {
        key: "printers",
        title: "Printers",
        body: "<p>The printers of the system, whether their writers are started, and their output queues. <b>Print to</b> opens Printer output ready to move files to the printer's queue; <b>Output</b> lists the files waiting on it.</p>",
    },
    Topic {
        key: "messages",
        title: "Messages",
        body: "<p>The messages on your own message queue, on <span class=\"mono\">QSYSOPR</span> or on another queue you may read, newest first. Inquiries waiting for a reply are in bold. <b>Reply</b> answers an inquiry; an empty reply sends its default. <b>Remove</b> takes a message off the queue; removing an inquiry nobody answered asks twice, because its sender then gets no reply. <b>Send</b> puts a message on a user's or a queue's message queue; an inquiry's reply comes to your own queue.</p>\n<h2>Watching</h2>\n<p>The queues chosen under Watch are looked at on every page: <b>Messages waiting</b> in the header counts what arrived since you last looked, and the next page you ask for opens Message Arrived for the first message the Arrived option names. <b>Later</b> leaves it on the queue.</p>",
    },
    Topic {
        key: "jobs",
        title: "Jobs",
        body: "<p><b>My jobs</b> lists the jobs of your user profile: running, waiting on a job queue, and ended with printer output still on the system. <b>Hold</b> and <b>Release</b> ask first, then run the command as your user profile. Job log, Properties and End open the job's pages in Navigator for QS/400.</p>\n<p><b>Server jobs</b> are the jobs that run Access for Web and serve your session. Ending one needs the End server jobs policy and special authority <span class=\"mono\">*JOBCTL</span>.</p>",
    },
    Topic {
        key: "cmd",
        title: "Run command",
        body: "<p>Type a CL command and press <b>Run</b>. It runs as your user profile, with your library list, and the page waits for it; in batch it is submitted to the job queue chosen. <b>Check</b> checks the parameters without running it. <b>Prompt</b> shows its parameters. The results list each command's messages as the system sent them; a message ID links to its full text. <b>Clear</b> empties the results; the recent commands stay.</p>",
    },
    Topic {
        key: "prompt",
        title: "Prompt",
        body: "<p>The command's parameters as its definition gives them: the keyword, what it is, the value and the values it accepts. A list parameter has a field per value; <b>+</b> adds one. <b>Show command string</b> builds the command from the fields; <b>Run</b> runs it; <b>Save to My commands</b> keeps it. The system checks the command when it runs.</p>",
    },
    Topic {
        key: "mycmds",
        title: "My commands",
        body: "<p>Commands you saved, with how they run. <b>Run</b> and <b>Prompt</b> put the command on Run command; <b>Edit</b> changes it; <b>Share</b> renames it or shares it with one of your groups or <span class=\"mono\">*PUBLIC</span>, who see it under Shared with you. A shared command still runs as the user who runs it.</p>",
    },
    Topic {
        key: "download",
        title: "Download",
        body: "<p>Files for the computers that use this system: printer drivers, the terminal font, certificates, documentation and connection details. They are read from the directory named on the page as your user profile.</p>",
    },
];

/// The topic of `key` (title, body HTML); the contents for any other.
pub fn topic(key: &str) -> (String, String) {
    if let Some(t) = TOPICS.iter().find(|t| t.key == key) {
        return (t.title.to_string(), t.body.to_string());
    }
    let mut body = String::from("<p>Help is available for these pages:</p>\n<ul>\n");
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
