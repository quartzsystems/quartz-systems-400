// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The update service's directory mirrored from where the PTF workflow
//! publishes it (the `ptf-catalog` release, flat): `catalog.toml`, every
//! `QSnnnnn.savf` and the group manifests. A PTF is fetched when it is
//! missing or does not match the catalog's size and SHA-256; the catalog
//! is written last, so a client never sees one whose files are not there.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use ptf_update::catalog::Catalog;

/// Where to mirror from and what to trust.
pub struct Source {
    /// The base URL the files are under, ending in `/`.
    pub base: String,
    /// Public keys the catalog must be signed with; empty takes it unchecked.
    pub trusted: Vec<[u8; 32]>,
}

/// What a pass did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Pass {
    /// The catalog's sequence now served.
    pub sequence: u64,
    /// PTFs fetched.
    pub fetched: usize,
    /// Whether the catalog changed.
    pub changed: bool,
}

fn get(url: &str) -> Result<Vec<u8>> {
    let r = httpc::send(&httpc::Request {
        method: "GET".into(),
        url: url.into(),
        headers: vec![("User-Agent".into(), "qs400-updated".into())],
        timeout: 120,
        redirects: 5,
        ..httpc::Request::default()
    })
    .with_context(|| url.to_string())?;
    if r.status != 200 {
        bail!("{url}: HTTP {}", r.status);
    }
    Ok(r.body)
}

/// Write `bytes` to `path` through a temporary file beside it.
fn put(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("part");
    fs::write(&tmp, bytes).with_context(|| tmp.display().to_string())?;
    fs::rename(&tmp, path).with_context(|| path.display().to_string())
}

fn parse(text: &str, trusted: &[[u8; 32]]) -> Result<Catalog> {
    if trusted.is_empty() {
        Ok(Catalog::parse_unverified(text)?)
    } else {
        Ok(Catalog::parse(text, trusted)?)
    }
}

/// One pass over `dir`: the catalog, then what it lists.
pub fn once(src: &Source, dir: &Path) -> Result<Pass> {
    fs::create_dir_all(dir.join("ptf"))?;
    fs::create_dir_all(dir.join("group"))?;
    let text = String::from_utf8(get(&format!("{}catalog.toml", src.base))?)
        .context("catalog.toml is not UTF-8")?;
    let cat = parse(&text, &src.trusted).context("catalog.toml")?;
    let local = dir.join("catalog.toml");
    let have = fs::read_to_string(&local).ok();
    if let Some(old) = have
        .as_deref()
        .and_then(|t| Catalog::parse_unverified(t).ok())
        && old.sequence > cat.sequence
    {
        bail!(
            "the published catalog's sequence {} is older than the {} served",
            cat.sequence,
            old.sequence
        );
    }
    let mut pass = Pass {
        sequence: cat.sequence,
        ..Pass::default()
    };
    for p in &cat.ptfs {
        let path = dir.join("ptf").join(format!("{}.savf", p.id));
        let good = fs::read(&path).is_ok_and(|b| ptf_update::client::check(p, &b).is_ok());
        if good {
            continue;
        }
        let bytes = get(&format!("{}{}.savf", src.base, p.id))?;
        ptf_update::client::check(p, &bytes)?;
        put(&path, &bytes)?;
        tracing::info!(ptf = %p.id, bytes = bytes.len(), "fetched");
        pass.fetched += 1;
    }
    for g in &cat.groups {
        let bytes = get(&format!("{}{}.toml", src.base, g.name))?;
        put(&dir.join("group").join(format!("{}.toml", g.name)), &bytes)?;
    }
    if have.as_deref() != Some(text.as_str()) {
        put(&local, text.as_bytes())?;
        pass.changed = true;
    }
    Ok(pass)
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Arc;

    use blockdev::{BlockDev, MemBlockDev};
    use ed25519_dalek::SigningKey;
    use ptf::package::{self, Build};
    use ptf::{PtfHeader, PtfKind};
    use ptf_update::catalog::GroupKind;
    use ptf_update::publish::{self, GroupDef, Publication};
    use qsname::Name;
    use store::obj::dtaara::{self, DataAreaType, DataAreaValue};
    use store::obj::{CreateParams, ops, types};
    use store::{FormatParams, Store, StoreConfig};

    use super::*;

    const NOW: u64 = 1_767_225_600;

    fn key() -> SigningKey {
        SigningKey::from_bytes(&[9u8; 32])
    }

    /// A PTF replacing one data area.
    fn make(id: &str) -> Vec<u8> {
        let dev = Arc::new(MemBlockDev::with_seed(16384, 3));
        let fp = FormatParams {
            asp_number: 1,
            asp_uuid: [4; 16],
            unit_uuids: vec![[5; 16]],
            unit_serials: Vec::new(),
            wal_pages: 256,
            now: NOW,
        };
        let mut st = Store::format(
            vec![Box::new(dev) as Box<dyn BlockDev>],
            &fp,
            StoreConfig::default(),
        )
        .unwrap();
        let mut txn = st.begin().unwrap();
        let qsys = ops::bootstrap(&mut txn, NOW, "QS400").unwrap();
        let params = |lib, n: &str, ty| CreateParams {
            library: lib,
            name: Name::parse(n).unwrap(),
            type_code: ty,
            attribute: String::new(),
            text: String::new(),
            owner: "QSYS".into(),
            system: "QS400".into(),
            now: NOW,
            by_system: true,
        };
        let lib = ops::create(&mut txn, &params(qsys, "QSVCLIB", types::LIB)).unwrap();
        dtaara::create(
            &mut txn,
            &params(lib, "LEVEL", types::DTAARA),
            DataAreaType::Char { len: 8 },
            &DataAreaValue::Char(id.into()),
        )
        .unwrap();
        txn.commit().unwrap();
        let header = PtfHeader {
            ptf: id.into(),
            product: "QS00SS1".into(),
            option: "0000".into(),
            release: "V1R0M0".into(),
            load: "5050".into(),
            target_release: "V1R0M0".into(),
            kind: PtfKind::Immediate,
            lic: false,
            migration: false,
            cover_only: false,
            created: NOW,
            prereqs: Vec::new(),
            coreqs: Vec::new(),
            supersedes: Vec::new(),
            exits: Vec::new(),
            object_count: 0,
            store_format: 0,
        };
        let k = key();
        let b = Build {
            header,
            cover: format!("{id}: sets the level."),
            objects: vec![("QSVCLIB".into(), "LEVEL".into(), types::DTAARA)],
            lic: None,
            lic_b: None,
            key: &k,
            system: "QSBUILD".into(),
        };
        package::build(&mut st, qsys, &b, NOW).unwrap()
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("qs400-updated-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    /// The release as the PTF workflow leaves it: flat.
    fn release() -> PathBuf {
        let pubdir = tmp("pub");
        let ptfs = vec![make("QS00001"), make("QS00002")];
        let groups = vec![GroupDef {
            name: "C26312110".into(),
            kind: GroupKind::Cumulative,
            level: 1,
            release: "V1R0M0".into(),
            text: "cumulative".into(),
            ptfs: vec!["QS00001".into(), "QS00002".into()],
        }];
        let p = Publication {
            ptfs: &ptfs,
            hiper: &[],
            groups: &groups,
            sequence: 2,
            now: NOW,
        };
        publish::publish(&pubdir, &p, &key()).unwrap();
        let rel = tmp("rel");
        fs::create_dir_all(&rel).unwrap();
        fs::copy(pubdir.join("catalog.toml"), rel.join("catalog.toml")).unwrap();
        for sub in ["ptf", "group"] {
            for e in fs::read_dir(pubdir.join(sub)).unwrap() {
                let e = e.unwrap();
                fs::copy(e.path(), rel.join(e.file_name())).unwrap();
            }
        }
        rel
    }

    /// A static server over `dir` on loopback; its base URL.
    fn host(dir: PathBuf) -> String {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { continue };
                let mut line = String::new();
                let mut r = BufReader::new(c.try_clone().unwrap());
                if r.read_line(&mut line).is_err() {
                    continue;
                }
                loop {
                    let mut h = String::new();
                    if r.read_line(&mut h).is_err() || h.trim().is_empty() {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/");
                let (status, body) = match fs::read(dir.join(path.trim_start_matches('/'))) {
                    Ok(b) => ("200 OK", b),
                    Err(_) => ("404 Not Found", Vec::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = c.write_all(head.as_bytes());
                let _ = c.write_all(&body);
            }
        });
        format!("http://{addr}/")
    }

    #[test]
    fn mirrors_the_release_and_repairs_it() {
        let src = Source {
            base: host(release()),
            trusted: vec![key().verifying_key().to_bytes()],
        };
        let dir = tmp("dir");
        let p = once(&src, &dir).unwrap();
        assert_eq!((p.sequence, p.fetched, p.changed), (2, 2, true));
        for f in [
            "catalog.toml",
            "ptf/QS00001.savf",
            "ptf/QS00002.savf",
            "group/C26312110.toml",
        ] {
            assert!(dir.join(f).is_file(), "{f}");
        }
        // Nothing new: nothing fetched.
        assert_eq!(
            once(&src, &dir).unwrap(),
            Pass {
                sequence: 2,
                fetched: 0,
                changed: false
            }
        );
        // A damaged PTF is fetched again.
        fs::write(dir.join("ptf/QS00002.savf"), b"damaged").unwrap();
        assert_eq!(once(&src, &dir).unwrap().fetched, 1);
        // A catalog another key signed is refused.
        let other = Source {
            base: src.base.clone(),
            trusted: vec![
                SigningKey::from_bytes(&[1u8; 32])
                    .verifying_key()
                    .to_bytes(),
            ],
        };
        assert!(once(&other, &dir).is_err());
    }
}
