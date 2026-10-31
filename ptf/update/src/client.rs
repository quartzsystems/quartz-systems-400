// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client `SNDPTFORD`, `QSCHKUPD`, `SNDSRVRQS` and `WRKPTFGRP` use:
//! one connection per request, TLS for `https://`, through a proxy when
//! one is set; every answer that matters verified.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tls::rustls::ClientConfig;
use tls::stream::TlsStream;

use crate::catalog::{Catalog, CatalogPtf};
use crate::http::{self, Request, Response};
use crate::order::{OrderRequest, OrderResult};
use crate::url::{Url, host_port};
use crate::{Error, Result};

/// Who is asking: the headers every request carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
    /// The system name.
    pub system: String,
    /// `VxRyMz`.
    pub release: String,
    /// The build ID.
    pub build: String,
}

impl Default for Ident {
    fn default() -> Self {
        let r = release::RELEASE;
        Self {
            system: "QS400".into(),
            release: format!("V{}R{}M{}", r.v, r.r, r.m),
            build: release::build_id().into(),
        }
    }
}

/// A connection to the service.
pub struct Client {
    /// The service URL.
    pub url: Url,
    /// `host:port` of a proxy.
    pub proxy: Option<(String, u16)>,
    /// The TLS configuration for `https://`.
    pub tls: Option<Arc<ClientConfig>>,
    /// Connect and read timeout.
    pub timeout: Duration,
    /// Who is asking.
    pub ident: Ident,
}

enum Conn {
    Plain(TcpStream),
    Tls(Box<TlsStream<TcpStream>>),
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(buf),
            Self::Tls(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(buf),
            Self::Tls(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls(s) => s.flush(),
        }
    }
}

impl Client {
    /// A client for `url`; `proxy` is `host:port` or empty.
    pub fn new(url: &str, proxy: &str, tls: Option<Arc<ClientConfig>>) -> Result<Self> {
        let proxy = if proxy.trim().is_empty() || proxy.trim().eq_ignore_ascii_case("*NONE") {
            None
        } else {
            Some(host_port(proxy, 8080).ok_or_else(|| Error::Url(proxy.into()))?)
        };
        Ok(Self {
            url: Url::parse(url)?,
            proxy,
            tls,
            timeout: Duration::from_secs(30),
            ident: Ident::default(),
        })
    }

    fn tcp(&self, host: &str, port: u16) -> Result<TcpStream> {
        let mut last = None;
        for addr in (host, port).to_socket_addrs()? {
            match TcpStream::connect_timeout(&addr, self.timeout) {
                Ok(s) => {
                    s.set_read_timeout(Some(self.timeout))?;
                    s.set_write_timeout(Some(self.timeout))?;
                    return Ok(s);
                }
                Err(e) => last = Some(e),
            }
        }
        Err(last.map_or_else(|| Error::Url(format!("{host}: no address")), Error::Io))
    }

    fn connect(&self) -> Result<Conn> {
        let tcp = match &self.proxy {
            None => self.tcp(&self.url.host, self.url.port)?,
            Some((host, port)) => {
                let mut s = self.tcp(host, *port)?;
                if self.url.tls {
                    let auth = self.url.authority();
                    write!(s, "CONNECT {auth} HTTP/1.1\r\nHost: {auth}\r\n\r\n")?;
                    let r = http::read_response(&mut s, true)?;
                    if r.status != 200 {
                        return Err(Error::Status {
                            status: r.status,
                            text: "the proxy refused CONNECT".into(),
                        });
                    }
                }
                s
            }
        };
        if !self.url.tls {
            return Ok(Conn::Plain(tcp));
        }
        let config = self
            .tls
            .clone()
            .ok_or_else(|| Error::Url(format!("{}: no TLS configuration", self.url)))?;
        let name = tls::config::server_name(&self.url.host)?;
        Ok(Conn::Tls(Box::new(TlsStream::connect(config, name, tcp)?)))
    }

    /// One request; the answer when it is 200.
    pub fn call(&self, method: &str, rel: &str, body: Vec<u8>) -> Result<Response> {
        let path = self.url.path_of(rel);
        let target = if self.proxy.is_some() && !self.url.tls {
            format!("http://{}{path}", self.url.authority())
        } else {
            path
        };
        let req = Request {
            method: method.into(),
            target,
            headers: vec![
                (
                    "User-Agent".into(),
                    format!("QS400/{} {}", self.ident.release, self.ident.build),
                ),
                ("X-QS400-System".into(), self.ident.system.clone()),
                ("X-QS400-Release".into(), self.ident.release.clone()),
            ],
            body,
        };
        let mut c = self.connect()?;
        http::write_request(&mut c, &req, &self.url.authority())?;
        let res = http::read_response(&mut c, false)?;
        if res.status != 200 {
            return Err(Error::Status {
                status: res.status,
                text: String::from_utf8_lossy(&res.body).trim().to_string(),
            });
        }
        Ok(res)
    }

    fn text(&self, method: &str, rel: &str, body: Vec<u8>) -> Result<String> {
        let r = self.call(method, rel, body)?;
        String::from_utf8(r.body).map_err(|_| Error::Protocol(format!("{rel}: not text")))
    }

    /// The catalog, verified against `trusted`; refused when older than
    /// `min_sequence`.
    pub fn catalog(&self, trusted: &[[u8; 32]], min_sequence: u64) -> Result<Catalog> {
        let c = Catalog::parse(&self.text("GET", "/v1/catalog", Vec::new())?, trusted)?;
        if c.sequence < min_sequence {
            return Err(Error::Verify(format!(
                "catalog {} is older than {min_sequence}",
                c.sequence
            )));
        }
        Ok(c)
    }

    /// A PTF's save stream, checked against its catalog entry.
    pub fn ptf(&self, entry: &CatalogPtf) -> Result<Vec<u8>> {
        let body = self.call("GET", &format!("/v1/ptf/{}", entry.id), Vec::new())?.body;
        check(entry, &body)?;
        Ok(body)
    }

    /// A group's signed manifest, verified.
    pub fn group(&self, name: &str, trusted: &[[u8; 32]]) -> Result<String> {
        let text = self.text("GET", &format!("/v1/group/{name}"), Vec::new())?;
        if !ptf::group::verify_signed(&text, trusted).1 {
            return Err(Error::Verify(format!("group {name}")));
        }
        Ok(text)
    }

    /// Place an order.
    pub fn order(&self, req: &OrderRequest) -> Result<OrderResult> {
        OrderResult::decode(&self.text("POST", "/v1/order", req.encode().into_bytes())?)
    }

    /// A service request; the service's one line.
    pub fn service(&self, action: &str, contact: &str) -> Result<String> {
        let body = format!(
            "QS400SERVICE 1\naction {action}\nsystem {}\ncontact {contact}\n",
            self.ident.system
        );
        Ok(self.text("POST", "/v1/service", body.into_bytes())?.trim().to_string())
    }
}

/// A stream against its catalog entry: size and SHA-256.
pub fn check(entry: &CatalogPtf, bytes: &[u8]) -> Result<()> {
    let digest = hex::encode(Sha256::digest(bytes));
    if bytes.len() as u64 != entry.size || !digest.eq_ignore_ascii_case(&entry.sha256) {
        return Err(Error::Verify(format!(
            "{}: {} bytes, SHA-256 {digest}",
            entry.id,
            bytes.len()
        )));
    }
    Ok(())
}
