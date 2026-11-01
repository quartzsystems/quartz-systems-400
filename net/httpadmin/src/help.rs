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
        key: "create-1",
        title: "Create HTTP server: name",
        body: "<p>Name the new instance. The name is also the name of the instance's job in subsystem <span class=\"mono\">QHTTPSVR</span> and, in lower case, of its directory under the default server root of Global settings.</p>
<p><b>Based on</b> either writes a new configuration file from your answers or copies the file of an existing instance, with the server root, document root and port you give in the next steps.</p>",
    },
    Topic {
        key: "create-name",
        title: "Instance name",
        body: "<p>1 to 10 characters: letters and digits, starting with a letter. The name is shown in upper case and must not already be used by another instance. Global settings limit how many instances the system may have.</p>",
    },
    Topic {
        key: "create-2",
        title: "Create HTTP server: server root",
        body: "<p>The server root holds the instance: <span class=\"mono\">conf/httpd.conf</span>, the <span class=\"mono\">logs</span> directory and, by default, the <span class=\"mono\">htdocs</span> content directory. Relative paths in the configuration file are taken from it.</p>",
    },
    Topic {
        key: "create-root",
        title: "Server root",
        body: "<p>An absolute path name. The directory is created if it does not exist; it must not be the server root of another instance.</p>",
    },
    Topic {
        key: "create-3",
        title: "Create HTTP server: document root",
        body: "<p>The document root is the directory the server serves files from: a request for <span class=\"mono\">/</span> maps to it. A sample <span class=\"mono\">index.html</span> can be written there so the new instance has a page to show; an existing file is never replaced.</p>",
    },
    Topic {
        key: "create-docroot",
        title: "Document root",
        body: "<p>An absolute path name. It is created if it does not exist.</p>",
    },
    Topic {
        key: "create-4",
        title: "Create HTTP server: address and port",
        body: "<p>The instance listens on the port you give, on every interface (<span class=\"mono\">*ALL</span>) or on one address of this system. More ports, and TLS ports, are added later on the Ports &amp; addresses page.</p>",
    },
    Topic {
        key: "create-addr",
        title: "IP address",
        body: "<p><span class=\"mono\">*ALL</span> listens on every TCP/IP interface. A specific address listens only on that interface. A port below 1024 needs the <span class=\"mono\">*IOSYSCFG</span> special authority on the server user profile named in Global settings.</p>",
    },
    Topic {
        key: "create-5",
        title: "Create HTTP server: access log",
        body: "<p>The access log records one line per request in the format you choose: <b>Common</b> (client, user, time, request, status, bytes), <b>Combined</b> (Common with the referring page and the browser) or <b>Extended</b> (Combined with the time taken). Rotation starts a new file daily or when the file passes 10 MB and keeps 30 rotated files.</p>",
    },
    Topic {
        key: "create-log",
        title: "Access log",
        body: "<p>An absolute path name, usually under the instance's <span class=\"mono\">logs</span> directory. Choose No to keep no access log; errors are always written to the error log.</p>",
    },
    Topic {
        key: "create-6",
        title: "Create HTTP server: create",
        body: "<p>Check the summary and press <b>Create</b>. The server root, its directories, the configuration file and the instance's registry entry are created, and message <span class=\"mono\">HTP8020</span> confirms it. The instance is created stopped. Creating an instance needs the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
    },
    Topic {
        key: "instance-general",
        title: "Instance: general",
        body: "<p>The general properties of an instance. The description, automatic start and hot backup partner are kept in the instance registry; the document root, server user profile and number of threads are directives of the configuration file (<span class=\"mono\">DocumentRoot</span>, <span class=\"mono\">ServerUserID</span>, <span class=\"mono\">ThreadsPerChild</span>).</p>
<p><b>Apply</b> saves and stays on the page, <b>OK</b> saves and returns to the list. A running instance uses changed directives after it is restarted. <b>Start</b>, <b>Stop</b> and <b>Restart</b> open the confirmation page. Changes need the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>
<p>The number of server threads must lie between 10 and the most Global settings allow.</p>",
    },
    Topic {
        key: "instance-usrprf",
        title: "Server user profile",
        body: "<p>The user profile the instance's job runs requests under. It must exist. If the instance listens on a port below 1024, the profile needs the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
    },
    Topic {
        key: "instance-hotbackup",
        title: "Hot backup instance",
        body: "<p>An instance that takes over the work of this one when it fails. Select the box to switch the partnership on; the partner chosen is remembered while it is switched off.</p>",
    },
    Topic {
        key: "instance-ports",
        title: "Instance: ports and addresses",
        body: "<p>Each row is a <span class=\"mono\">Listen</span> directive: the address the instance listens on (<span class=\"mono\">*ALL</span> for every interface) and the port. A port is TLS when a <span class=\"mono\">&lt;VirtualHost&gt;</span> for it turns <span class=\"mono\">SSLEngine On</span>; the certificate comes from the instance's certificate application (TLS page).</p>
<p><b>Add</b> writes a new Listen entry and, for a TLS port, its virtual host. <b>Remove</b> deletes the selected entries and any virtual host left without a port; an instance keeps at least one. A port below 1024 needs the <span class=\"mono\">*IOSYSCFG</span> special authority on the server user profile. A running instance uses the change after a restart.</p>",
    },
    Topic {
        key: "instance-directories",
        title: "Instance: directories and aliases",
        body: "<p>The directories the instance serves and how. A <span class=\"mono\">&lt;Directory&gt;</span> row is a directory container: under the document root it is reached at the URL below it, elsewhere through an <span class=\"mono\">Alias</span> shown on the same row. An <b>Alias</b> maps a URL path onto a directory outside the document root; a <b>ScriptAlias</b> does the same for programs the server runs. An alias's own options and access rules are kept in a <span class=\"mono\">&lt;Location&gt;</span> for its URL.</p>
<p>The server's guard for the root of the file system, which refuses everything not granted elsewhere, is not listed. <b>Remove</b> asks for confirmation before deleting the entry's directives. Changes need the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
    },
    Topic {
        key: "instance-directory",
        title: "Directory properties",
        body: "<p><b>Options</b> are the server features allowed in the directory: listing a directory without an index file (Indexes), following symbolic links, running programs (ExecCGI) and server-side includes. Only the options that differ from what the directory inherits are written.</p>
<p><b>Order</b>, <b>Allow from</b> and <b>Deny from</b> decide which clients may connect, by address, network (<span class=\"mono\">10.0.0.0/8</span>) or <span class=\"mono\">all</span>. With Deny,Allow the deny list is checked first and the allow list can override it.</p>
<p><b>Require authentication</b> asks for a user and password checked against a validation list (<span class=\"mono\">LIBRARY/NAME</span>); the realm is the name the browser shows.</p>",
    },
    Topic {
        key: "instance-tls",
        title: "Instance: TLS",
        body: "<p>The TLS settings of the instance's TLS ports, the ports whose virtual host turns <span class=\"mono\">SSLEngine</span> on (Ports &amp; addresses page). The certificate comes from the <span class=\"mono\">*SYSTEM</span> certificate store through the instance's certificate application, <span class=\"mono\">QIBM_HTTP_SERVER_</span> and the instance name; the list shows the certificate assigned to it first. Assign another on the Certificate applications page.</p>
<p><b>Protocol versions</b> are written as <span class=\"mono\">SSLProtocolDisable</span> for the versions not chosen; TLS 1.0 is never offered. <b>*DFT</b> uses the cipher list of system value <span class=\"mono\">QSSLCSL</span>; a custom list is written on the Configuration file page. <b>Client authentication</b> asks browsers for a certificate (Optional) or refuses those without one (Required). Changes need the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
    },
    Topic {
        key: "instance-tls-enable",
        title: "Enable TLS",
        body: "<p>Turns TLS on or off for every TLS port of the instance at once. Off, the ports stay in the configuration file and answer without TLS until it is turned on again.</p>",
    },
    Topic {
        key: "instance-logging",
        title: "Instance: logging",
        body: "<p>The <b>access log</b> records each request (<span class=\"mono\">CustomLog</span>) in the format chosen; the <b>error log</b> records problems at the log level chosen and above (<span class=\"mono\">ErrorLog</span>, <span class=\"mono\">LogLevel</span>). Paths under the server root are written relative to it.</p>
<p><b>Rotate</b> starts a new file daily at the time given or when a file passes the size given, and keeps the number of rotated files given (<span class=\"mono\">LogMaint</span>). <b>Log directory CCSID</b> is the coded character set the logs are written in; 1208 is UTF-8. Changes need the <span class=\"mono\">*IOSYSCFG</span> special authority and are used by a running instance after it is restarted.</p>",
    },
    Topic {
        key: "instance-config",
        title: "Instance: configuration file",
        body: "<p>The instance's configuration file as it is on disk. The other pages of the instance read and write the same file; changes made here show on them at once.</p>
<p><b>Validate</b> checks every line without writing anything and reports the first error with its line and column (message <span class=\"mono\">HTP8100</span>). <b>Apply</b> checks the text the same way and, only if it is correct, replaces the file: the new text is written to a temporary file and renamed over the old one, so an interruption leaves the previous file in place, and the previous file is kept as <span class=\"mono\">httpd.conf.bak</span>. A running instance uses the new file after it is restarted. <b>Reset</b> discards your edits and shows the file again.</p>
<p>Applying needs the <span class=\"mono\">*IOSYSCFG</span> special authority.</p>",
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
