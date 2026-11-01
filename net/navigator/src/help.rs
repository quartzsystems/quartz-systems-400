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
    // Database.
    Topic {
        key: "sql",
        title: "Run SQL",
        body: "<p>Type one SQL statement and press <b>Run</b>. It runs with your user profile's authority, with the schema chosen as the default for unqualified names, the naming chosen (<span class=\"mono\">*SQL</span>: <span class=\"mono\">schema.table</span>; <span class=\"mono\">*SYS</span>: <span class=\"mono\">library/file</span>) and the isolation level chosen.</p>\n<h2>Results</h2>\n<p>A query shows at most 500 rows; <b>Download as CSV</b> runs it again and gives every row. Column headings sort the rows shown. <b>Explain</b> shows how the statement would run without running it.</p>\n<h2>Previous statements</h2>\n<p>Your last 50 statements, newest first. Choose one to put it back in the statement box. They are kept while the server runs.</p>",
    },
    Topic {
        key: "db",
        title: "Schemas",
        body: "<p>The schemas (libraries) you may see, and in the one chosen its tables, views, indexes and procedures from the system catalog. Choose a table to see its columns, constraints, indexes, triggers and attributes, or its data.</p>",
    },
    Topic {
        key: "db-table",
        title: "Table",
        body: "<p>A table as the system catalog describes it, tab by tab. <b>Generate SQL (DDL)</b> shows the statements that create the table. <b>Run SQL</b> starts a query of the table.</p>",
    },
    Topic {
        key: "db-data",
        title: "Viewing table data",
        body: "<p>The table's rows, 25 a page, read with the statement shown and your authority. Type a condition in <b>Where</b> (for example <span class=\"mono\">STATUS = 'SPARE'</span>) and press <b>Apply</b> to narrow them. <b>Edit in Run SQL</b> opens the statement there.</p>",
    },
    // Messages.
    Topic {
        key: "msgq",
        title: "Message queue",
        body: "<p>The messages on a queue, newest first. Inquiry messages still waiting for a reply are bold and offer <b>Reply</b>. Choose another queue, a type or the first date to show and press <b>Go</b>.</p>\n<h2>Actions</h2>\n<p><b>Remove</b> removes one message; <b>Clear queue...</b> removes them all, keeping the inquiries waiting unless you choose otherwise. Both show the command first.</p>",
    },
    Topic {
        key: "mymsgs",
        title: "My messages",
        body: "<p>The messages on your user profile's message queue. <b>Send message</b> sends a message to a user or a message queue; an inquiry's reply comes back to your queue.</p>",
    },
    Topic {
        key: "message",
        title: "Message details",
        body: "<p>One message: who sent it, from which job and program, and its full text. For an inquiry waiting for its reply, type one of the valid replies and press <b>Reply</b>; an empty reply sends the default. A message answered already cannot be answered again.</p>",
    },
    Topic {
        key: "qhst",
        title: "History log",
        body: "<p>The system's history log, newest first, as the 5250 display prints it. Narrow it by period, message identifier (a name ending in <span class=\"mono\">*</span> is generic), lowest severity or job, and press <b>Go</b>.</p>",
    },
    // Users and groups.
    Topic {
        key: "users",
        title: "Users",
        body: "<p>The user profiles on the system. Filter by name (generic with <span class=\"mono\">*</span>), status, class or group, primary or supplemental. <b>Delete</b> asks what to do with the objects the profile owns.</p>",
    },
    Topic {
        key: "user",
        title: "User properties",
        body: "<p>One user profile, tab by tab. <b>Apply</b> changes what you changed with one command; creating or changing profiles needs special authority <span class=\"mono\">*SECADM</span>, and you can give only the special authorities you have.</p>\n<h2>Reset password</h2>\n<p>Type the new password and press <b>Reset password...</b>; choose <b>Set to expired</b> so that the user must change it at the next sign-on.</p>",
    },
    Topic {
        key: "createuser",
        title: "Create user",
        body: "<p>Five steps to a new user profile. <b>Back</b> and <b>Next</b> keep what you typed; <b>Cancel</b> forgets it. The last step shows the command that <b>Create</b> runs and can also create the home directory and the message queue.</p>",
    },
    Topic {
        key: "groups",
        title: "Groups",
        body: "<p>Group profiles: profiles other users name as their group. <b>Members</b> lists the users whose primary or supplemental group it is. A group with members cannot be deleted.</p>",
    },
    Topic {
        key: "autl",
        title: "Authorization lists",
        body: "<p>Authorization lists secure objects with one list of users and their authorities. <b>Open</b> shows and changes the entries, <b>Objects</b> the objects the list secures.</p>",
    },
    // System.
    Topic {
        key: "sysval",
        title: "System values",
        body: "<p>The system values by category. <b>Change</b> shows a value with the values it accepts in grey; <b>OK</b> runs <span class=\"mono\">CHGSYSVAL</span> as you, which writes an <span class=\"mono\">SV</span> entry to the audit journal. A value marked <span class=\"mono\">(IPL)</span> takes effect at the next IPL. Security values need special authority <span class=\"mono\">*SECADM</span>.</p>",
    },
    Topic {
        key: "disk",
        title: "Disk status",
        body: "<p>The auxiliary storage pools and the disk units in them, with how full each is. A bar turns red at 70 %. <b>Reset</b> restarts your I/O counters, which Disk activity measures from.</p>",
    },
    Topic {
        key: "hdwrsc",
        title: "Hardware resources",
        body: "<p>The hardware as a tree: the system unit, its I/O processors, their adapters and the devices on them. <b>Vary on</b> and <b>Vary off</b> run <span class=\"mono\">VRYCFG</span> for the lines, controllers and devices over a resource, after a confirmation.</p>",
    },
    Topic {
        key: "time",
        title: "Time management",
        body: "<p>The time zone, the date and time formats and how the clock is kept right. <b>Apply</b> runs <span class=\"mono\">CHGSYSVAL</span> for each value you changed and <span class=\"mono\">CHGNTPA</span> for the time servers (special authority <span class=\"mono\">*IOSYSCFG</span>).</p>",
    },
    // Network.
    Topic {
        key: "tcp",
        title: "TCP/IP configuration",
        body: "<p>The host and domain names, name servers and datagram forwarding. <b>Apply</b> runs <span class=\"mono\">CHGTCPDMN</span> and <span class=\"mono\">CHGTCPA</span> for what you changed; changing TCP/IP needs special authority <span class=\"mono\">*IOSYSCFG</span>.</p>",
    },
    Topic {
        key: "tcpifc",
        title: "TCP/IP interfaces",
        body: "<p>The system's internet addresses. <b>Start</b> and <b>End</b> run <span class=\"mono\">STRTCPIFC</span> and <span class=\"mono\">ENDTCPIFC</span>; <b>Add...</b>, <b>Change</b> and <b>Remove</b> change the configuration. Addresses the system makes itself cannot be removed.</p>",
    },
    Topic {
        key: "tcprte",
        title: "TCP/IP routes",
        body: "<p>Where traffic for other networks goes. <span class=\"mono\">*DFTROUTE</span> carries everything no other route matches. A route is inactive while its interface is.</p>",
    },
    Topic {
        key: "servers",
        title: "Servers",
        body: "<p>The TCP servers and the ports they listen on. <b>Start</b>, <b>End</b> and <b>Restart</b> run <span class=\"mono\">STRTCPSVR</span> and <span class=\"mono\">ENDTCPSVR</span>; <b>Properties</b> sets whether a server starts with TCP/IP. The HTTP server's instances are managed in HTTP Server Administration.</p>",
    },
    // Performance.
    Topic {
        key: "perfsys",
        title: "System activity",
        body: "<p>The processor's use by job and the storage pools' paging, measured from your previous display of the page. A job's bar spans 0 to 20 % CPU. Choose an interval and press <b>Apply</b> to reload the page by itself.</p>",
    },
    Topic {
        key: "perfdisk",
        title: "Disk activity",
        body: "<p>Each disk unit's requests and transfer rates, measured from your previous display of the page. A unit busy 70 % or more is red and named under the table.</p>",
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
