// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The engine over files in memory: static files, indexes, listings,
//! conditional and ranged requests, aliases, redirects, access by
//! address, Basic authentication, error documents, logs, keep-alive.

use std::io;
use std::net::SocketAddr;
use std::sync::Arc;

use super::auth::{MemAuth, SYSTEM};
use super::*;
use crate::conf::parse;
use crate::files::MemFiles;

const NOW: u64 = 1_793_232_000;

const CONF: &str = "ServerRoot /www/shop\nListen *:8080\nServerName shop.example\nDocumentRoot htdocs\n\
LogFormat \"%h %u \\\"%r\\\" %>s %b\" short\nCustomLog logs/access_log short\nErrorLog logs/error_log\nLogLevel info\n\
DirectoryIndex index.html index.htm\nOptions -Indexes\nAddType application/x-shop .shp\n\
Alias /pics/ /www/shop/images/\nScriptAlias /cgi-bin/ /QSYS.LIB/SHOP.LIB/\n\
Redirect permanent /old http://shop.example/new\nRedirect gone /dead\nErrorDocument 404 /missing.html\nErrorDocument 410 \"Long gone\"\n\
KeepAlive On\nMaxKeepAliveRequests 3\n\
<Directory />\n   Order Deny,Allow\n   Deny From all\n</Directory>\n\
<Directory /www/shop/htdocs>\n   Order Allow,Deny\n   Allow From all\n</Directory>\n\
<Directory /www/shop/htdocs/list>\n   Options +Indexes\n</Directory>\n\
<Directory /www/shop/htdocs/lan>\n   Order Allow,Deny\n   Allow From 10.0.0.0/8\n</Directory>\n\
<Directory /www/shop/htdocs/priv>\n   AuthType Basic\n   AuthName \"Shop staff\"\n   PasswdFile %%SYSTEM%%\n   Require valid-user\n</Directory>\n\
<Directory /www/shop/htdocs/boss>\n   AuthType Basic\n   AuthName Boss\n   Require user BOSS\n</Directory>\n\
<Directory /www/shop/images>\n   Order Allow,Deny\n   Allow From all\n</Directory>\n\
<Directory /QSYS.LIB/SHOP.LIB>\n   Order Allow,Deny\n   Allow From all\n</Directory>\n";

fn server() -> (Server, Arc<MemFiles>) {
    let files = Arc::new(MemFiles::at(NOW - 3600));
    files.put("/www/shop/htdocs/index.html", b"<h1>shop</h1>");
    files.put("/www/shop/htdocs/a.txt", b"0123456789");
    files.put("/www/shop/htdocs/t.shp", b"x");
    files.put("/www/shop/htdocs/missing.html", b"no such page");
    files.put("/www/shop/htdocs/list/b.css", b"b{}");
    files.mkdir_all("/www/shop/htdocs/list/sub").unwrap();
    files.mkdir_all("/www/shop/htdocs/empty").unwrap();
    files.put("/www/shop/htdocs/lan/x.html", b"lan");
    files.put("/www/shop/htdocs/priv/p.html", b"private");
    files.put("/www/shop/htdocs/boss/b.html", b"boss");
    files.put("/www/shop/images/logo.png", b"PNG");
    files.put("/www/shop/conf/secret", b"secret");
    let plan = Plan::new("SHOP", "/www/shop", &parse(CONF).unwrap());
    let deps = Deps {
        files: files.clone(),
        auth: Arc::new(MemAuth::new().with(SYSTEM, "CLERK", "pw1").with(SYSTEM, "BOSS", "pw2")),
        clock: Arc::new(|| NOW),
    };
    (Server::new(plan, deps), files)
}

fn ctx(peer: &str) -> Ctx {
    Ctx {
        local: "127.0.0.1:8080".parse().unwrap(),
        peer: format!("{peer}:40000").parse::<SocketAddr>().unwrap(),
        secure: false,
    }
}

fn get(s: &Server, path: &str, headers: &[(&str, &str)]) -> Response {
    let mut r = Request::new("GET", path);
    for (k, v) in headers {
        r.headers.push(((*k).to_string(), (*v).to_string()));
    }
    s.respond(&r, &ctx("192.168.1.5"))
}

#[test]
fn files_indexes_and_listings() {
    let (s, _) = server();
    let r = get(&s, "/", &[]);
    assert_eq!((r.status, r.text().as_str()), (200, "<h1>shop</h1>"));
    assert_eq!(r.header("content-type"), Some("text/html; charset=utf-8"));
    assert_eq!(r.header("last-modified"), Some(time::http_date(NOW - 3600).as_str()));
    assert_eq!(get(&s, "/t.shp", &[]).header("content-type"), Some("application/x-shop"));
    let listing = get(&s, "/list/", &[]).text();
    assert!(listing.contains("<a href=\"sub/\">sub/</a>") && listing.contains("b.css"), "{listing}");
    assert_eq!(get(&s, "/empty/", &[]).status, 403);
    let dir = get(&s, "/list?x=1", &[]);
    assert_eq!((dir.status, dir.header("location")), (301, Some("/list/?x=1")));
    // ErrorDocument as a local page keeps the status.
    let missing = get(&s, "/nope.html", &[]);
    assert_eq!((missing.status, missing.text().as_str()), (404, "no such page"));
    assert_eq!(get(&s, "/../conf/secret", &[]).status, 400);
    assert_eq!(s.respond(&Request::new("HEAD", "/a.txt"), &ctx("1.2.3.4")).status, 200);
    assert_eq!(s.respond(&Request::new("POST", "/a.txt"), &ctx("1.2.3.4")).status, 405);
    assert_eq!(s.respond(&Request::new("DELETE", "/a.txt"), &ctx("1.2.3.4")).status, 501);
}

#[test]
fn conditional_and_ranges() {
    let (s, _) = server();
    let since = time::http_date(NOW);
    assert_eq!(get(&s, "/a.txt", &[("if-modified-since", &since)]).status, 304);
    let old = time::http_date(NOW - 7200);
    assert_eq!(get(&s, "/a.txt", &[("if-modified-since", &old)]).status, 200);
    let r = get(&s, "/a.txt", &[("range", "bytes=2-4")]);
    assert_eq!((r.status, r.text().as_str(), r.header("content-range")), (206, "234", Some("bytes 2-4/10")));
    assert_eq!(get(&s, "/a.txt", &[("range", "bytes=50-")]).status, 416);
}

#[test]
fn aliases_redirects_and_scripts() {
    let (s, files) = server();
    let p = get(&s, "/pics/logo.png", &[]);
    assert_eq!((p.status, p.text().as_str()), (200, "PNG"));
    let r = get(&s, "/old/x.html", &[]);
    assert_eq!((r.status, r.header("location")), (301, Some("http://shop.example/new/x.html")));
    let g = get(&s, "/dead", &[]);
    assert_eq!((g.status, g.text().as_str()), (410, "Long gone"));
    assert_eq!(get(&s, "/cgi-bin/PGM.PGM", &[]).status, 501);
    let log = String::from_utf8(files.read("/www/shop/logs/error_log").unwrap()).unwrap();
    assert!(log.contains("CGI is not supported yet"), "{log}");
}

#[test]
fn access_by_address() {
    let (s, files) = server();
    let lan = Request::new("GET", "/lan/x.html");
    assert_eq!(s.respond(&lan, &ctx("10.1.2.3")).status, 200);
    assert_eq!(s.respond(&lan, &ctx("192.168.1.5")).status, 403);
    let log = String::from_utf8(files.read("/www/shop/logs/error_log").unwrap()).unwrap();
    assert!(log.contains("[error] [client 192.168.1.5] Client denied by the configuration: /www/shop/htdocs/lan/x.html"), "{log}");
}

#[test]
fn basic_authentication() {
    let (s, files) = server();
    let r = get(&s, "/priv/p.html", &[]);
    assert_eq!((r.status, r.header("www-authenticate")), (401, Some("Basic realm=\"Shop staff\"")));
    // CLERK:pw1
    let ok = get(&s, "/priv/p.html", &[("authorization", "Basic Q0xFUks6cHcx")]);
    assert_eq!((ok.status, ok.text().as_str()), (200, "private"));
    assert_eq!(get(&s, "/priv/p.html", &[("authorization", "Basic Q0xFUks6bm8=")]).status, 401);
    // Only BOSS (BOSS:pw2) may see /boss.
    assert_eq!(get(&s, "/boss/b.html", &[("authorization", "Basic Q0xFUks6cHcx")]).status, 401);
    assert_eq!(get(&s, "/boss/b.html", &[("authorization", "Basic Qk9TUzpwdzI=")]).status, 200);
    let log = String::from_utf8(files.read("/www/shop/logs/access_log").unwrap()).unwrap();
    assert!(log.contains("192.168.1.5 CLERK \"GET /priv/p.html HTTP/1.1\" 200 7\n"), "{log}");
    assert!(log.contains("192.168.1.5 - \"GET /priv/p.html HTTP/1.1\" 401 "), "{log}");
}

/// A connection in memory.
struct Duplex {
    input: io::Cursor<Vec<u8>>,
    output: Vec<u8>,
}

impl io::Read for Duplex {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.input.read(b)
    }
}

impl io::Write for Duplex {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        self.output.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Transport for Duplex {
    fn set_timeouts(&mut self, _: Option<Duration>, _: Option<Duration>) {}
}

fn run(s: &Server, wire: &str) -> String {
    let mut d = Duplex { input: io::Cursor::new(wire.as_bytes().to_vec()), output: Vec::new() };
    s.serve_conn(&mut d, ctx("10.0.0.7"));
    String::from_utf8(d.output).unwrap()
}

#[test]
fn keep_alive_limits() {
    let (s, _) = server();
    let four = "GET /a.txt HTTP/1.1\r\nHost: x\r\n\r\n".repeat(4);
    let out = run(&s, &four);
    // MaxKeepAliveRequests 3: the third answer closes.
    assert_eq!(out.matches("HTTP/1.1 200 OK").count(), 3);
    assert_eq!(out.matches("Connection: close").count(), 1);
    assert_eq!(s.connections(), 0);
    assert_eq!(s.requests(), 3);
    let bad = run(&s, "BROKEN\r\n\r\n");
    assert!(bad.starts_with("HTTP/1.1 400 "), "{bad}");
    s.stop();
    // Stopping: the request in hand is answered and the connection closes.
    let one = run(&s, &"GET /a.txt HTTP/1.1\r\n\r\n".repeat(2));
    assert_eq!(one.matches("HTTP/1.1 200 OK").count(), 1);
    assert!(one.contains("Connection: close"));
    assert!(s.drain(Duration::from_millis(10)));
}

#[test]
fn virtual_hosts_and_off_logs() {
    let conf = "ServerRoot /www/v\nListen 80\nDocumentRoot /www/v/main\nErrorLog off\n\
        <VirtualHost *:80>\n   ServerName a.example\n   DocumentRoot /www/v/a\n</VirtualHost>\n\
        <VirtualHost *:80>\n   ServerName b.example\n   ServerAlias www.b.example\n   DocumentRoot /www/v/b\n</VirtualHost>\n";
    let files = Arc::new(MemFiles::new());
    files.put("/www/v/a/index.html", b"A");
    files.put("/www/v/b/index.html", b"B");
    let s = Server::new(
        Plan::new("V", "/www/v", &parse(conf).unwrap()),
        Deps { files: files.clone(), auth: Arc::new(auth::NoAuth), clock: Arc::new(|| NOW) },
    );
    let at = |host: &str| {
        let mut r = Request::new("GET", "/");
        r.headers.push(("host".into(), host.into()));
        let c = Ctx { local: "10.0.0.1:80".parse().unwrap(), ..ctx("10.0.0.2") };
        s.respond(&r, &c).text()
    };
    assert_eq!(at("a.example"), "A");
    assert_eq!(at("www.b.example:80"), "B");
    assert_eq!(at("other"), "A");
    assert!(!files.exists("/www/v/logs/error_log"));
}
