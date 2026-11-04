// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The response file over the network at a D IPL (docs/INSTALL.md,
//! "Network install"): the first Ethernet link gets an address from
//! DHCP (unless `qs400.ip=` gives one), then the file a `qs400.rsp=`
//! URL names, or with `qs400.rsp=dhcp`, or after a network boot with no
//! response file on the media, `INSTALL.RSP` beside the boot file the
//! DHCP server named. `https` checks the server against the shipped
//! roots and the certificates in `QS400/TRUST.PEM` on the media.

use std::io;
use std::net::TcpStream;
use std::sync::Mutex;
use std::time::Duration;

use instmedia::fetch::{self, Url, http};
use instmedia::rsp::Source;

use crate::ipl::system::System;

/// The text fetched, kept: the IPL asks more than once.
static FETCHED: Mutex<Option<(String, String)>> = Mutex::new(None);

/// The trust file on the media.
pub const TRUST: &str = "QS400/TRUST.PEM";

/// Whether the firmware booted this IPL over the network: the
/// `BootCurrent` entry's device path holds a MAC, IPv4, IPv6 or URI node.
pub fn network_boot() -> bool {
    crate::platform::efivar::current_boot_path().is_some_and(|p| device_path_is_network(&p))
}

/// Whether an EFI device path (the nodes of a `Boot####` entry) names
/// a network device: messaging (type 3) MAC (11), IPv4 (12), IPv6 (13)
/// or URI (24) nodes.
pub fn device_path_is_network(mut p: &[u8]) -> bool {
    while p.len() >= 4 {
        let (ty, sub) = (p[0], p[1]);
        let len = usize::from(u16::from_le_bytes([p[2], p[3]]));
        if ty == 0x7f || len < 4 || len > p.len() {
            return false;
        }
        if ty == 3 && matches!(sub, 11 | 12 | 13 | 24) {
            return true;
        }
        p = &p[len..];
    }
    false
}

/// The response file's text for `src` (a URL, or beside the boot file).
pub fn text(system: &mut System, src: &Source) -> Result<String, String> {
    let key = format!("{src:?}");
    if let Some((k, t)) = FETCHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
        && k == key
    {
        return Ok(t);
    }
    super::enumerate_links(system);
    super::net_address(system);
    let url = match src {
        Source::Url(u) => u.clone(),
        Source::Dhcp => {
            let l = super::ipl_lease().ok_or("no DHCP lease: no network boot server to ask")?;
            let (next, file) = l.boot.ok_or("the DHCP server named no boot file")?;
            fetch::beside(next, &file)
        }
        Source::Media(_) => return Err("not a network source".into()),
    };
    let body = get(&url)?;
    let t = String::from_utf8(body).map_err(|_| format!("{url}: not text"))?;
    system.out.serial(&format!("slic: install: response file from {url}\n"));
    *FETCHED.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some((key, t.clone()));
    Ok(t)
}

/// The bytes at `url`.
fn get(url: &str) -> Result<Vec<u8>, String> {
    let u = Url::parse(url)?;
    if u.scheme == "tftp" {
        return fetch::tftp::get(&u.host, u.port, &u.path, Duration::from_secs(2));
    }
    let roots = trusted_roots();
    let connect = move |u: &Url| -> io::Result<Box<dyn http::Conn>> {
        let tcp = TcpStream::connect((u.host.as_str(), u.port))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        if u.scheme != "https" {
            return Ok(Box::new(tcp));
        }
        let cfg = tls::config::client_config(
            &tls::policy::TlsPolicy::opsys(),
            roots.clone(),
            None,
            tls::config::ServerAuthMode::Full,
        )
        .map_err(io::Error::other)?;
        let name = tls::config::server_name(&u.host).map_err(io::Error::other)?;
        let s = tls::stream::TlsStream::connect(cfg, name, tcp).map_err(io::Error::other)?;
        Ok(Box::new(s))
    };
    http::get(&u, &connect)
}

/// The shipped roots and the media's `TRUST.PEM`.
fn trusted_roots() -> tls::rustls::RootCertStore {
    let mut roots = tls::rustls::RootCertStore::empty();
    for r in certs::roots::roots() {
        let _ = roots.add(tls::rustls::pki_types::CertificateDer::from(r.der.clone()));
    }
    if let Ok(f) = super::media::locate()
        && let Ok(Some(pem)) = super::media::with_media(&f, |at| std::fs::read_to_string(at.join(TRUST)).ok())
        && let Ok(ders) = certs::x509::pem_certificates(&pem)
    {
        for d in ders {
            let _ = roots.add(tls::rustls::pki_types::CertificateDer::from(d));
        }
    }
    roots
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_device_paths() {
        // PciRoot(0)/Pci(2,0)/MAC(525400123456,1)/IPv4(...)/End
        let mut p = vec![2, 1, 12, 0, 0xd0, 0x41, 3, 0x0a, 0, 0, 0, 0];
        p.extend([1, 1, 6, 0, 0, 2]);
        let mut mac = vec![3, 11, 37, 0];
        mac.extend([0x52, 0x54, 0, 0x12, 0x34, 0x56]);
        mac.resize(36, 0);
        mac.push(1);
        p.extend(&mac);
        p.extend([0x7f, 0xff, 4, 0]);
        assert!(device_path_is_network(&p));
        // A disk: Pci/Sata/HD/File, no network node.
        let disk = [2, 1, 12, 0, 0xd0, 0x41, 3, 0x0a, 0, 0, 0, 0, 3, 18, 10, 0, 0, 0, 0, 0, 0, 0, 0x7f, 0xff, 4, 0];
        assert!(!device_path_is_network(&disk));
        assert!(!device_path_is_network(&[3, 11, 2, 0]));
    }
}
