// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A web product of QSDEMO's own on the shared web layer: its family
//! (wordmark, release, paths, cookie), a console product with its own
//! navigation tree, a sign-on, one form with CSRF and a message row
//! with a `QMC`-style identifier. A certificate made here (P-256,
//! self-signed) serves it over TLS 1.3 on a socket pair; a client
//! pinned to that certificate signs on, posts the form and fetches the
//! page, which must match the fixture's golden file. Then the terminal
//! page ([`crate::term`]): its picture must be the golden picture, its
//! script the look crate's, and a posted key the console's key.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use qsweb::brand::Brand;
use qsweb::frame::{self, Ctx, NavGroup, Page, Product};
use qsweb::golden::{self, Normalize};
use qsweb::http::{Handler, Peer, Request, Response};
use qsweb::msg::{Kind, Message};
use qsweb::partials::{Button, Prop, PropGroup, input, props};
use qsweb::session::{self, Sessions};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{CryptoProvider, verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection,
};
use rustls::{SignatureScheme, StreamOwned};

/// The page the client ends on, as it must be.
const GOLDEN: &str = include_str!("../golden/settings.html");

/// The terminal page's picture, as it must be.
const GOLDEN_PNG: &[u8] = include_bytes!("../golden/terminal.png");

/// QSDEMO's family of web products.
static BRAND: Brand = Brand {
    name: "QSDEMO",
    vendor: "Example Systems",
    release: &crate::RELEASE,
    server: "QSDEMO",
    css_path: "/demo/shared/site.css",
    nav_path: "/demo/shared/nav.js",
    cookie: "QSDEMOSID",
    signon_notice: "Authorized administrators only.",
    notice_in_form: false,
    user_max: 10,
    notauth_note: "Ask the console's administrator for this authority.",
    job_log: false,
};

/// The console: one product of the family.
static PRODUCT: Product = Product {
    brand: &BRAND,
    wordmark: "QSDEMO Management Console",
    crumb_root: "Management Console",
    prefix: "/demo",
    nav: &[
        NavGroup {
            label: "Systems",
            items: &[("Managed systems", "/systems")],
        },
        NavGroup {
            label: "Console",
            items: &[("Settings", "/settings"), ("Certificate", "/certificate")],
        },
    ],
    system_label: "Console",
    nav_script: false,
};

const USER: &str = "ADMIN";
const PASSWORD: &str = "demo-pass";

struct Console {
    sessions: Sessions,
    name: std::sync::Mutex<String>,
}

impl Console {
    fn page(&self, req: &Request, sid: &str, user: &str) -> Response {
        let s = self.sessions.get(sid).expect("a live session");
        let mut messages = Vec::new();
        if let Some(m) = self.sessions.take_flash(sid) {
            messages.push(m);
        }
        let name = self.name.lock().expect("name").clone();
        let ctx = Ctx {
            product: &PRODUCT,
            system: name.clone(),
            user: user.into(),
            csrf: s.csrf,
            open: None,
            here: frame::here(req),
        };
        let body = props(
            &ctx,
            &[PropGroup {
                title: Some("Identification".into()),
                rows: vec![
                    Prop::new("Console name", input("name", &name, 20)),
                    Prop::new("Release", BRAND.release()),
                ],
            }],
            &[Button::new("ok", "OK"), Button::new("cancel", "Cancel")],
        );
        let page = Page {
            title: "Settings".into(),
            crumb: vec!["Console".into(), "Settings".into()],
            h1: "Settings".into(),
            desc: Some("The console's own name, shown beside the user on every page.".into()),
            messages,
            body,
            help: "settings".into(),
            nav: Some(("Console", Some("/settings"))),
            ..Page::default()
        };
        Response::html(200, frame::render(&ctx, &page))
    }

    /// `/terminal`, its picture, its script and its keypad's posts.
    fn terminal(&self, req: &Request, s: &session::Session) -> Response {
        let here = req.path.strip_prefix(PRODUCT.prefix).unwrap_or("");
        match (req.method.as_str(), here) {
            ("GET", "/terminal/screen.png") => match crate::term::png() {
                Ok(png) => Response::new(200, "image/png", png),
                Err(e) => qsweb::http::plain(500, &e),
            },
            ("GET", "/terminal/script.js") => Response::new(
                200,
                "text/javascript; charset=utf-8",
                crate::term::script().into_bytes(),
            ),
            ("POST", "/terminal/key") => {
                let f = req.form();
                if !session::csrf_ok(s, f.get(session::CSRF_FIELD).unwrap_or("")) {
                    return qsweb::http::plain(403, "The form has expired.");
                }
                let label = f.get("k").unwrap_or("");
                let m = match crate::term::posted(label) {
                    Some(k) => Message::new(
                        Kind::Done,
                        "QMC2002",
                        &format!("Key {label} sent to the console as {k:?}."),
                    ),
                    None => Message::new(
                        Kind::Error,
                        "QMC0002",
                        &format!("Key {label} is not a console key."),
                    ),
                };
                self.sessions.flash(&s.id, m);
                Response::redirect(&PRODUCT.url("/terminal"))
            }
            ("GET", "/terminal") => {
                let mut messages = Vec::new();
                if let Some(m) = self.sessions.take_flash(&s.id) {
                    messages.push(m);
                }
                let ctx = Ctx {
                    product: &PRODUCT,
                    system: self.name.lock().expect("name").clone(),
                    user: s.user.clone(),
                    csrf: s.csrf.clone(),
                    open: None,
                    here: frame::here(req),
                };
                let page = Page {
                    title: "Terminal".into(),
                    crumb: vec!["Systems".into(), "Terminal".into()],
                    h1: "Terminal".into(),
                    messages,
                    body: crate::term::body(&|p| PRODUCT.url(p), session::CSRF_FIELD, &s.csrf),
                    help: "terminal".into(),
                    nav: Some(("Systems", None)),
                    ..Page::default()
                };
                Response::html(200, frame::render(&ctx, &page))
            }
            _ => qsweb::http::plain(404, "There is no such page."),
        }
    }
}

impl Handler for Console {
    fn server(&self) -> String {
        BRAND.server_header()
    }

    fn handle(&self, req: &Request, peer: &Peer) -> Response {
        if req.path == BRAND.css_path {
            return qsweb::css_response();
        }
        if req.path == PRODUCT.url("/signon") {
            if req.method == "POST" {
                let f = req.form();
                if f.get("user") == Some(USER) && f.get("password") == Some(PASSWORD) {
                    let s = self.sessions.create(USER);
                    return Response::redirect(&PRODUCT.url("/settings"))
                        .with("Set-Cookie", &PRODUCT.set_cookie(&s.id, peer.secure));
                }
                let m = Message::new(
                    Kind::Error,
                    "QMC0001",
                    "The user or password is not correct.",
                );
                return Response::html(
                    200,
                    frame::signon(&PRODUCT, &self.name.lock().expect("name"), "", &[m], None),
                );
            }
            return Response::html(
                200,
                frame::signon(&PRODUCT, &self.name.lock().expect("name"), "", &[], None),
            );
        }
        let Some(s) = PRODUCT.session_id(req).and_then(|id| self.sessions.get(id)) else {
            return Response::redirect(&PRODUCT.url("/signon"));
        };
        if req.path.starts_with(&PRODUCT.url("/terminal")) {
            return self.terminal(req, &s);
        }
        if req.path != PRODUCT.url("/settings") {
            return qsweb::http::plain(404, "There is no such page.");
        }
        if req.method == "POST" {
            let f = req.form();
            if !session::csrf_ok(&s, f.get(session::CSRF_FIELD).unwrap_or("")) {
                return qsweb::http::plain(403, "The form has expired.");
            }
            if let Some(n) = f.get("name").filter(|n| !n.is_empty()) {
                *self.name.lock().expect("name") = n.to_string();
                self.sessions.flash(
                    &s.id,
                    Message::new(
                        Kind::Done,
                        "QMC2001",
                        &format!("Console name changed to {n}."),
                    ),
                );
            }
            return Response::redirect(&PRODUCT.url("/settings"));
        }
        self.page(req, &s.id, &s.user)
    }
}

fn provider() -> Arc<CryptoProvider> {
    Arc::new(rustls_graviola::default_provider())
}

/// Accepts the one certificate whose SHA-256 it was given.
#[derive(Debug)]
struct Pinned {
    sha256: [u8; 32],
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if x509kit::crypto::sha256(end_entity) == self.sha256 {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("not the pinned certificate".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(unix)]
type Sock = std::os::unix::net::UnixStream;
#[cfg(not(unix))]
type Sock = std::net::TcpStream;

/// Two connected sockets.
fn pair() -> io::Result<(Sock, Sock)> {
    #[cfg(unix)]
    let (a, b) = Sock::pair()?;
    #[cfg(not(unix))]
    let (a, b) = {
        let l = std::net::TcpListener::bind("127.0.0.1:0")?;
        let a = Sock::connect(l.local_addr()?)?;
        (a, l.accept()?.0)
    };
    for s in [&a, &b] {
        s.set_read_timeout(Some(Duration::from_secs(20)))?;
    }
    Ok((a, b))
}

/// One answer as the client read it.
struct Got {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
    raw: Vec<u8>,
}

impl Got {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

/// The client's end: requests on one kept-alive TLS connection.
struct Client<S: Read + Write> {
    conn: BufReader<S>,
    cookie: Option<String>,
}

impl<S: Read + Write> Client<S> {
    fn send(&mut self, method: &str, path: &str, form: &[(&str, &str)]) -> Result<Got, String> {
        let body: Vec<String> = form
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    qsweb::http::form::encode(k),
                    qsweb::http::form::encode(v)
                )
            })
            .collect();
        let body = body.join("&");
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: qsdemo.local\r\n");
        if let Some(c) = &self.cookie {
            req.push_str(&format!("Cookie: {}={c}\r\n", BRAND.cookie));
        }
        if method == "POST" {
            req.push_str(&format!(
                "Content-Type: application/x-www-form-urlencoded\r\nContent-Length: {}\r\n",
                body.len()
            ));
        }
        req.push_str("\r\n");
        req.push_str(&body);
        let w = self.conn.get_mut();
        w.write_all(req.as_bytes())
            .and_then(|()| w.flush())
            .map_err(|e| format!("sending {method} {path}: {e}"))?;
        let mut line = String::new();
        self.conn
            .read_line(&mut line)
            .map_err(|e| format!("reading {path}: {e}"))?;
        let status = line
            .split(' ')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("{path}: status line {line:?}"))?;
        let mut headers = Vec::new();
        loop {
            let mut h = String::new();
            self.conn.read_line(&mut h).map_err(|e| e.to_string())?;
            let h = h.trim_end();
            if h.is_empty() {
                break;
            }
            if let Some((k, v)) = h.split_once(':') {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        let mut got = Got {
            status,
            headers,
            body: String::new(),
            raw: Vec::new(),
        };
        let len: usize = got
            .header("Content-Length")
            .and_then(|l| l.parse().ok())
            .ok_or_else(|| format!("{path}: no Content-Length"))?;
        let mut b = vec![0u8; len];
        self.conn.read_exact(&mut b).map_err(|e| e.to_string())?;
        got.body = String::from_utf8_lossy(&b).into_owned();
        got.raw = b;
        if let Some(c) = got.header("Set-Cookie") {
            let v = c
                .split(';')
                .next()
                .and_then(|kv| kv.split_once('='))
                .map(|(_, v)| v.to_string());
            self.cookie = v.filter(|v| !v.is_empty());
        }
        Ok(got)
    }
}

fn expect(got: &Got, status: u16, what: &str) -> Result<(), String> {
    if got.status == status {
        Ok(())
    } else {
        Err(format!(
            "{what}: {} instead of {status}: {}",
            got.status, got.body
        ))
    }
}

/// The CSRF token a page's form carries.
fn csrf_of(html: &str) -> Option<String> {
    golden::tokens(html).into_iter().find_map(|t| match t {
        golden::Tok::Open(n, a)
            if n == "input" && a.get("name").map(String::as_str) == Some(session::CSRF_FIELD) =>
        {
            a.get("value").cloned()
        }
        _ => None,
    })
}

/// Run the product and its client: what held, the page the client
/// ended on, and how it differs from the golden file.
#[allow(clippy::type_complexity)]
pub fn run() -> Result<(Vec<String>, String, Option<String>), String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let id =
        x509kit::server::self_signed("qsdemo.local", &["qsdemo.local", "127.0.0.1"], now - 60, 30)
            .map_err(|e| format!("certificate: {e}"))?;
    let pkcs8 = id.key.to_pkcs8_der().map_err(|e| e.to_string())?.to_vec();
    let server = ServerConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .with_no_client_auth()
        .with_single_cert(
            vec![CertificateDer::from(id.cert.clone())],
            PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(pkcs8)),
        )
        .map_err(|e| format!("server TLS: {e}"))?;
    let mut sha256 = [0u8; 32];
    sha256.copy_from_slice(&x509kit::crypto::sha256(&id.cert));
    let client = ClientConfig::builder_with_provider(provider())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned {
            sha256,
            provider: provider(),
        }))
        .with_no_client_auth();

    let (a, b) = pair().map_err(|e| e.to_string())?;
    let console = Console {
        sessions: Sessions::new(Duration::from_secs(600)),
        name: std::sync::Mutex::new("QSDEMOMC".into()),
    };
    let served = thread::spawn(move || -> Result<(), String> {
        let conn = ServerConnection::new(Arc::new(server)).map_err(|e| e.to_string())?;
        let mut tls = StreamOwned::new(conn, b);
        let peer = Peer {
            addr: "127.0.0.1:2400".parse().expect("an address"),
            secure: true,
        };
        qsweb::http::serve(&mut tls, &peer, &console);
        Ok(())
    });
    let name = ServerName::try_from("qsdemo.local").map_err(|e| e.to_string())?;
    let conn = ClientConnection::new(Arc::new(client), name).map_err(|e| e.to_string())?;
    let mut c = Client {
        conn: BufReader::new(StreamOwned::new(conn, a)),
        cookie: None,
    };

    let mut said = Vec::new();
    let r = c.send("GET", "/demo/settings", &[])?;
    expect(&r, 303, "signed off")?;
    let r = c.send("GET", "/demo/signon", &[])?;
    expect(&r, 200, "sign-on page")?;
    if r.header("Server") != Some(&BRAND.server_header()) {
        return Err(format!("Server header {:?}", r.header("Server")));
    }
    let r = c.send(
        "POST",
        "/demo/signon",
        &[("user", USER), ("password", "wrong")],
    )?;
    if !r.body.contains("QMC0001") {
        return Err("a wrong password was not refused".into());
    }
    let r = c.send(
        "POST",
        "/demo/signon",
        &[("user", USER), ("password", PASSWORD)],
    )?;
    expect(&r, 303, "sign on")?;
    let suite = c.conn.get_ref().conn.negotiated_cipher_suite();
    let version = c.conn.get_ref().conn.protocol_version();
    let r = c.send("GET", "/demo/settings", &[])?;
    expect(&r, 200, "settings")?;
    let token = csrf_of(&r.body).ok_or("no CSRF field on the form")?;
    let r = c.send(
        "POST",
        "/demo/settings",
        &[("csrf", "forged"), ("name", "EVIL")],
    )?;
    expect(&r, 403, "a forged form")?;
    let r = c.send(
        "POST",
        "/demo/settings",
        &[("csrf", &token), ("name", "QSDEMOMC1")],
    )?;
    expect(&r, 303, "the form")?;
    let r = c.send("GET", "/demo/settings", &[])?;
    expect(&r, 200, "settings after the form")?;
    let page = r.body;

    // The terminal page on the look crate.
    let r = c.send("GET", "/demo/terminal", &[])?;
    expect(&r, 200, "terminal page")?;
    for want in [
        "id=\"term\"",
        "src=\"/demo/terminal/screen.png\"",
        "data-socket=\"/demo/terminal/ws\"",
        "name=\"k\" value=\"Field+\"",
    ] {
        if !r.body.contains(want) {
            return Err(format!("the terminal page has no {want}"));
        }
    }
    let token = csrf_of(&r.body).ok_or("no CSRF field on the keypad")?;
    let png = c.send("GET", "/demo/terminal/screen.png", &[])?;
    expect(&png, 200, "the terminal's picture")?;
    if png.raw != GOLDEN_PNG {
        return Err(format!(
            "the terminal's picture ({} bytes) is not the golden picture ({} bytes)",
            png.raw.len(),
            GOLDEN_PNG.len()
        ));
    }
    let js = c.send("GET", "/demo/terminal/script.js", &[])?;
    expect(&js, 200, "the terminal's script")?;
    if !js.body.starts_with("var QS_FONT = {") || !js.body.contains("data-socket") {
        return Err("the terminal's script is not the look crate's".into());
    }
    let r = c.send(
        "POST",
        "/demo/terminal/key",
        &[("csrf", &token), ("k", "F3")],
    )?;
    expect(&r, 303, "a keypad post")?;
    let r = c.send("GET", "/demo/terminal", &[])?;
    if !r.body.contains("QMC2002") || !r.body.contains("Function(3)") {
        return Err("the posted F3 was not decoded as the console's F3".into());
    }
    let keys = crate::term::from_socket("{\"t\":\"key\",\"k\":\"Enter\"}");
    if keys != [qsipart::console::Key::Enter] {
        return Err(format!("a socket's Enter decoded as {keys:?}"));
    }
    drop(c);
    served
        .join()
        .map_err(|_| "the server's thread failed".to_string())??;

    let map = |_: &str| None;
    let n = Normalize {
        url: &map,
        script: Some(BRAND.nav_path),
    };
    // The footer names the release, which is the fixture's own.
    let golden = GOLDEN.replace("@RELEASE@", &BRAND.release());
    let differs = golden::diff(&page, &golden, &n);
    said.push(format!(
        "certificate CN=qsdemo.local P-256 self-signed, SHA-256 {}..",
        &id.info.sha256[..16]
    ));
    let (Some(version), Some(suite)) = (version, suite) else {
        return Err("no TLS version or cipher suite was negotiated".into());
    };
    said.push(format!(
        "served over {version:?} {:?} to a client pinned to it",
        suite.suite()
    ));
    said.push("signed on, CSRF refused a forged form, message QMC2001 shown".into());
    said.push("the settings page matches its golden file".into());
    said.push(
        "terminal: qsipart frames to a grid, the picture matches its golden PNG, F3 posted and decoded"
            .into(),
    );
    Ok((said, page, differs))
}

/// The proof's lines.
pub fn prove() -> Result<Vec<String>, String> {
    match run()? {
        (said, _, None) => Ok(said),
        (_, _, Some(d)) => Err(format!("the page is not its golden file:\n{d}")),
    }
}
