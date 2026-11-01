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
    // Work management.
    Topic {
        key: "jobs",
        title: "Active jobs",
        body: "<p>The jobs running now, as the 5250 active jobs display shows them. Choose a subsystem, user, job name (a name ending in <span class=\"mono\">*</span> matches every name starting so) or status and press <b>Go</b>; column headings sort.</p>\n<h2>Elapsed interval</h2>\n<p>CPU % is measured over the elapsed interval. <b>Refresh</b> starts a new one.</p>\n<h2>Actions</h2>\n<p>Hold, Release and End run the job commands as you, after a page that shows the command. Ending or holding another user's job needs special authority <span class=\"mono\">*JOBCTL</span>.</p>",
    },
    Topic {
        key: "endjob",
        title: "Ending a job",
        body: "<p><b>Controlled</b> lets the job's programs see that it is ending and finish their work; after the delay time it ends immediately. <b>Immediate</b> ends it at once, without cleanup. Spooled files are kept unless you choose to delete them; the maximum log entries limit what the job log keeps.</p>",
    },
    Topic {
        key: "confirm",
        title: "Confirmations",
        body: "<p>A confirmation names what the command acts on and shows the command itself. <b>OK</b> runs it with your authority; <b>Cancel</b> returns without running it.</p>",
    },
    Topic {
        key: "job",
        title: "Job properties",
        body: "<p>One job, tab by tab: its attributes, performance, library list, job log, spooled files, locks and call stack. On <b>General</b>, <b>Apply</b> changes the run priority, time slice, logging and inquiry reply of the job.</p>\n<h2>Job log</h2>\n<p>The job log as printed, message identifiers linked to their text. <b>Show</b> narrows it by severity or type, <b>Last</b> keeps the newest messages, <b>Display second level</b> adds each message's cause and recovery.</p>",
    },
    Topic {
        key: "jobq",
        title: "Job queues",
        body: "<p>Job queues and the jobs waiting on them. A held queue starts no jobs until released. <b>Clear</b> removes every waiting job and asks first.</p>",
    },
    Topic {
        key: "sbs",
        title: "Subsystems",
        body: "<p>Every subsystem description and whether it runs. <b>End</b> ends a subsystem and every job in it, after a confirmation; <b>Start</b> starts one. Both need special authority <span class=\"mono\">*JOBCTL</span>.</p>",
    },
    Topic {
        key: "sbs-props",
        title: "Subsystem properties",
        body: "<p>A subsystem description's attributes, pools, routing entries, autostart, workstation and prestart job entries. Changes run the commands that change the description; most take effect when the subsystem next starts.</p>",
    },
    Topic {
        key: "outq",
        title: "Output queues",
        body: "<p>Output queues and how many spooled files each holds. <b>Open</b> lists the files; a held queue sends nothing to its writer. <b>Clear</b> deletes every file on the queue and asks first.</p>",
    },
    Topic {
        key: "splf",
        title: "Spooled files",
        body: "<p>The spooled files on a queue or of a job. <b>View</b> shows a file as printed; <b>Move</b> sends it to another output queue; <b>Delete</b> asks first. Other users' files need special authority <span class=\"mono\">*SPLCTL</span> or <span class=\"mono\">*JOBCTL</span>.</p>",
    },
    Topic {
        key: "splf-view",
        title: "Viewing a spooled file",
        body: "<p>One page at a time, 132 columns exactly as spooled. Type a page number and press <b>Go</b>, or use Previous and Next. <b>Find</b> searches from the page shown to the end, then from the start. The file is also available as PDF and as text.</p>",
    },
    Topic {
        key: "writers",
        title: "Printers",
        body: "<p>Printers and their writers. A writer prints the files of its output queue on its printer; <b>End writer</b> stops it and <b>Vary off</b> makes the printer unavailable, both after a confirmation.</p>",
    },
    Topic {
        key: "jobscde",
        title: "Scheduled jobs",
        body: "<p>Job schedule entries: a command submitted as a job at the times given. A held entry submits nothing until released. <b>Submit now</b> submits the job once without changing its schedule.</p>",
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
