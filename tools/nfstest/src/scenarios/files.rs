// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Files: the three create modes, writes and commits, reads, a large file,
//! appends, truncation, links, special files, removal, and a core over UDP.

use nfs_v3::wire::{
    COMMIT3args, COMMIT3res, LINK3args, LINK3res, MKNOD3args, MKNOD3res, NFS_PROGRAM as P,
    Nfs3Option, READLINK3args, READLINK3res, SYMLINK3args, SYMLINK3res, createhow3, createverf3,
    devicedata3, ftype3, mknoddata3, nfspath3, nfsstat3, sattr3, specdata3, stable_how,
    symlinkdata3,
};
use onc_xdr::Opaque;

use super::{Family, Lines, Scenario, hex, line};
use crate::ctx::{Ctx, prog};
use crate::error::{check, fail};
use crate::rpc::{Cred, Proto};
use crate::v3::{Mount, Nfs3, Session, dirop, expect_err, fh, handle_of, mode, ok, status};

/// This file's scenarios.
pub const LIST: [Scenario; 14] = [
    s("create-modes", create_modes),
    s("create-exclusive", create_exclusive),
    s("write-unstable-commit", write_unstable),
    s("write-file-sync", write_sync),
    s("read-offsets-eof", read_offsets),
    s("large-file", large_file),
    s("append", append),
    s("truncate", truncate),
    s("symlink-readlink", symlink),
    s("mknod-fifo", mknod_fifo),
    s("mknod-device", mknod_device),
    s("hard-link", hard_link),
    s("remove", remove),
    s("udp-core", udp_core),
];

const fn s(name: &'static str, run: fn(&Ctx) -> Lines) -> Scenario {
    Scenario {
        name,
        family: Family::V3,
        run,
    }
}

/// A test pattern: byte `i` of the file at `off`.
pub fn pattern(off: u64, len: usize) -> Vec<u8> {
    (0..len as u64)
        .map(|i| {
            let x = off + i;
            (x ^ (x >> 8) ^ (x >> 16)).to_le_bytes()[0]
        })
        .collect()
}

/// FNV-1a, 64 bits.
pub fn fnv(h: u64, data: &[u8]) -> u64 {
    data.iter()
        .fold(h, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x100_0000_01b3))
}

/// FNV-1a's start.
pub const FNV0: u64 = 0xcbf2_9ce4_8422_2325;

fn create_modes(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "create-modes")?;
    let d = s.dir.clone();
    let r =
        s.n.create_res(&d, b"u", createhow3::UNCHECKED(mode(0o640)))?;
    let h = handle_of(&ok(r, "CREATE UNCHECKED")?.obj)?;
    s.n.write(&h, 0, b"keep")?;
    let r =
        s.n.create_res(&d, b"u", createhow3::UNCHECKED(mode(0o640)))?;
    ok(r, "CREATE UNCHECKED of an existing file")?;
    check(s.n.getattr(&h)?.size == 4, || {
        "UNCHECKED over a file without size truncated it".into()
    })?;
    let r = s.n.create_res(&d, b"u", createhow3::GUARDED(mode(0o640)))?;
    expect_err(
        &r,
        nfsstat3::NFS3ERR_EXIST,
        "CREATE GUARDED of an existing file",
    )?;
    let g = s.n.create(&d, b"g", 0o600)?;
    let a = s.n.getattr(&g)?;
    check(a.mode & 0o707 == 0o600, || {
        format!("GUARDED mode {:o}", a.mode)
    })?;
    s.close()?;
    Ok(vec![
        line("UNCHECKED twice", "NFS3_OK, data kept"),
        line("GUARDED over a file", "NFS3ERR_EXIST"),
    ])
}

fn create_exclusive(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "create-exclusive")?;
    let d = s.dir.clone();
    let v = createverf3(*b"nfstestA");
    let r = s.n.create_res(&d, b"x", createhow3::EXCLUSIVE(v))?;
    let first = ok(r, "CREATE EXCLUSIVE")?;
    let h1 = match handle_of(&first.obj) {
        Ok(h) => h,
        Err(_) => s.n.lookup(&d, b"x")?,
    };
    // A retransmission (a new XID, so not the duplicate request cache): same verifier.
    let r = s.n.create_res(&d, b"x", createhow3::EXCLUSIVE(v))?;
    let again = ok(r, "CREATE EXCLUSIVE retransmitted with the same verifier")?;
    if let Ok(h2) = handle_of(&again.obj) {
        check(s.n.getattr(&h2)?.fileid == s.n.getattr(&h1)?.fileid, || {
            "the retransmission made another file".into()
        })?;
    }
    let r =
        s.n.create_res(&d, b"x", createhow3::EXCLUSIVE(createverf3(*b"nfstestB")))?;
    expect_err(
        &r,
        nfsstat3::NFS3ERR_EXIST,
        "CREATE EXCLUSIVE with another verifier",
    )?;
    // The client sets the attributes after an exclusive create (RFC 1813 3.3.8).
    ok(
        s.n.setattr(&h1, mode(0o640), Nfs3Option::None)?,
        "SETATTR after EXCLUSIVE",
    )?;
    let a = s.n.getattr(&h1)?;
    check(a.mode & 0o707 == 0o600, || format!("mode {:o}", a.mode))?;
    s.close()?;
    Ok(vec![
        line("same verifier again", "NFS3_OK, the same file"),
        line("another verifier", "NFS3ERR_EXIST"),
    ])
}

fn commit(n: &mut Nfs3, f: &[u8]) -> Result<[u8; 8], crate::Error> {
    let r: COMMIT3res = n.call(
        P::NFSPROC3_COMMIT,
        &COMMIT3args {
            file: fh(f),
            offset: 0,
            count: 0,
        },
    )?;
    Ok(ok(r, "COMMIT")?.verf.0)
}

fn write_unstable(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "write-unstable")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"u", 0o644)?;
    let mut verfs = Vec::new();
    for i in 0..4u64 {
        let data = pattern(i * 4096, 4096);
        let r = ok(
            s.n.write_res(&f, i * 4096, &data, stable_how::UNSTABLE)?,
            "WRITE UNSTABLE",
        )?;
        check(r.count == 4096, || format!("count {}", r.count))?;
        verfs.push((r.verf.0, r.committed));
    }
    check(verfs.iter().all(|v| v.0 == verfs[0].0), || {
        "the write verifier changed between writes".into()
    })?;
    let cv = commit(&mut s.n, &f)?;
    check(cv == verfs[0].0, || {
        "COMMIT's verifier is not the writes'".into()
    })?;
    let cv2 = commit(&mut s.n, &f)?;
    check(cv2 == cv, || "the verifier changed between commits".into())?;
    let (data, _) = s.n.read(&f, 0, 16384)?;
    check(data == pattern(0, 16384), || {
        "data read back differs".into()
    })?;
    s.close()?;
    Ok(vec![
        line("committed as", verfs[0].1),
        line("verifier", hex(&cv)),
    ])
}

fn write_sync(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "write-file-sync")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"s", 0o644)?;
    let data = pattern(0, 10_000);
    let r = ok(
        s.n.write_res(&f, 0, &data, stable_how::FILE_SYNC)?,
        "WRITE FILE_SYNC",
    )?;
    check(r.committed == stable_how::FILE_SYNC, || {
        format!("FILE_SYNC answered as {}", r.committed)
    })?;
    check(r.count == 10_000, || format!("count {}", r.count))?;
    let (back, eof) = s.n.read(&f, 0, 10_000)?;
    check(back == data, || "data read back differs".into())?;
    s.close()?;
    Ok(vec![line("eof at the end", eof)])
}

fn read_offsets(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "read-offsets")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"r", 0o644)?;
    s.n.write(&f, 0, &pattern(0, 10_000))?;
    let mut out = Vec::new();
    for (off, count, want, want_eof) in [
        (0u64, 100u32, 100usize, false),
        (5000, 1000, 1000, false),
        (9990, 100, 10, true),
        (10_000, 100, 0, true),
        (20_000, 100, 0, true),
    ] {
        let (b, eof) = s.n.read(&f, off, count)?;
        check(b.len() == want, || {
            format!("READ {off}+{count}: {} bytes", b.len())
        })?;
        check(b == pattern(off, want), || {
            format!("READ {off}+{count}: wrong data")
        })?;
        check(eof == want_eof, || format!("READ {off}+{count}: eof {eof}"))?;
        out.push(format!("READ {off}+{count}: {} bytes eof {eof}", b.len()));
    }
    s.close()?;
    Ok(out)
}

/// The server's preferred read and write sizes (at most 1 MiB; 32 KiB over UDP).
pub fn io_sizes(c: &Ctx, n: &mut Nfs3, root: &[u8]) -> Result<(u32, u32), crate::Error> {
    let i: nfs_v3::wire::FSINFO3res = n.call(
        P::NFSPROC3_FSINFO,
        &nfs_v3::wire::FSINFO3args { fsroot: fh(root) },
    )?;
    let i = ok(i, "FSINFO")?;
    let cap = if c.t.proto == Proto::Udp {
        32 * 1024
    } else {
        1 << 20
    };
    let r = i.rtpref.max(4096).min(i.rtmax).min(cap);
    let w = i.wtpref.max(4096).min(i.wtmax).min(cap);
    Ok((r, w))
}

fn large_file(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "large-file")?;
    let d = s.dir.clone();
    let root = s.root.clone();
    let (rsize, wsize) = io_sizes(c, &mut s.n, &root)?;
    let f = s.n.create(&d, b"large", 0o644)?;
    let total = c.t.large_mib << 20;
    let mut sum = FNV0;
    let mut off = 0u64;
    while off < total {
        let n = (total - off).min(u64::from(wsize)) as usize;
        let data = pattern(off, n);
        sum = fnv(sum, &data);
        let r = ok(
            s.n.write_res(&f, off, &data, stable_how::UNSTABLE)?,
            "WRITE",
        )?;
        check(r.count as usize == n, || {
            format!("WRITE at {off}: {} of {n}", r.count)
        })?;
        off += n as u64;
    }
    commit(&mut s.n, &f)?;
    check(s.n.getattr(&f)?.size == total, || {
        "size after the writes".into()
    })?;
    let mut back = FNV0;
    let mut off = 0u64;
    loop {
        let (b, eof) = s.n.read(&f, off, rsize)?;
        back = fnv(back, &b);
        off += b.len() as u64;
        if eof || b.is_empty() {
            break;
        }
    }
    check(off == total, || format!("read {off} of {total}"))?;
    check(back == sum, || "checksum differs".into())?;
    s.close()?;
    Ok(vec![
        line("bytes", total),
        line("wsize", wsize),
        line("rsize", rsize),
        line("fnv64", format!("{sum:016x}")),
    ])
}

fn append(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "append")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"log", 0o644)?;
    for i in 0..10u8 {
        let size = s.n.getattr(&f)?.size;
        s.n.write(&f, size, &[b'a' + i; 1000])?;
    }
    let (b, _) = s.n.read(&f, 0, 20_000)?;
    check(b.len() == 10_000, || format!("{} bytes", b.len()))?;
    check(
        b.chunks(1000)
            .enumerate()
            .all(|(i, ch)| ch.iter().all(|x| *x == b'a' + i as u8)),
        || "appended data out of place".into(),
    )?;
    s.close()?;
    Ok(vec![line("size", 10_000)])
}

fn truncate(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "truncate")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"t", 0o644)?;
    s.n.write(&f, 0, &pattern(0, 10_000))?;
    let size = |n: u64| sattr3 {
        size: Nfs3Option::Some(n),
        ..sattr3::default()
    };
    ok(
        s.n.setattr(&f, size(100), Nfs3Option::None)?,
        "SETATTR size 100",
    )?;
    let (b, eof) = s.n.read(&f, 0, 1000)?;
    check(b == pattern(0, 100) && eof, || {
        format!("after truncating down: {} bytes", b.len())
    })?;
    ok(
        s.n.setattr(&f, size(5000), Nfs3Option::None)?,
        "SETATTR size 5000",
    )?;
    let (b, _) = s.n.read(&f, 0, 6000)?;
    check(b.len() == 5000, || {
        format!("after extending: {} bytes", b.len())
    })?;
    check(
        b[..100] == pattern(0, 100)[..] && b[100..].iter().all(|x| *x == 0),
        || "after extending: the new bytes are not zero".into(),
    )?;
    s.close()?;
    Ok(vec![line("down to 100 then up to 5000", "zeros after 100")])
}

fn symlink(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "symlink")?;
    let d = s.dir.clone();
    let target = b"some/where/../target";
    let r: SYMLINK3res = s.n.call(
        P::NFSPROC3_SYMLINK,
        &SYMLINK3args {
            where_: dirop(&d, b"ln"),
            symlink: symlinkdata3 {
                symlink_attributes: mode(0o777),
                symlink_data: nfspath3(Opaque::borrowed(target)),
            },
        },
    )?;
    ok(r, "SYMLINK")?;
    let h = s.n.lookup(&d, b"ln")?;
    check(s.n.getattr(&h)?.type_ == ftype3::NF3LNK, || {
        "not a link".into()
    })?;
    let r: READLINK3res =
        s.n.call(P::NFSPROC3_READLINK, &READLINK3args { symlink: fh(&h) })?;
    let back = ok(r, "READLINK")?.data.0.to_vec();
    check(back == target, || {
        format!("READLINK: {}", String::from_utf8_lossy(&back))
    })?;
    // READLINK of a file that is not a link.
    let f = s.n.create(&d, b"plain", 0o644)?;
    let r: READLINK3res =
        s.n.call(P::NFSPROC3_READLINK, &READLINK3args { symlink: fh(&f) })?;
    let st = status(&r);
    check(st == nfsstat3::NFS3ERR_INVAL, || {
        format!("READLINK of a file: {st}")
    })?;
    s.close()?;
    Ok(vec![
        line("READLINK", String::from_utf8_lossy(target)),
        line("READLINK of a file", st),
    ])
}

fn mknod(n: &mut Nfs3, d: &[u8], name: &[u8], what: mknoddata3) -> Result<MKNOD3res, crate::Error> {
    n.call(
        P::NFSPROC3_MKNOD,
        &MKNOD3args {
            where_: dirop(d, name),
            what,
        },
    )
}

fn mknod_fifo(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "mknod-fifo")?;
    let d = s.dir.clone();
    let r = mknod(&mut s.n, &d, b"fifo", mknoddata3::NF3FIFO(mode(0o644)))?;
    ok(r, "MKNOD FIFO")?;
    let h = s.n.lookup(&d, b"fifo")?;
    let t = s.n.getattr(&h)?.type_;
    check(t == ftype3::NF3FIFO, || format!("type {t}"))?;
    s.close()?;
    Ok(vec![line("type", t)])
}

fn mknod_device(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "mknod-device")?;
    let d = s.dir.clone();
    let mut out = Vec::new();
    for (what, data) in [
        (
            "character device",
            mknoddata3::NF3CHR(devicedata3 {
                dev_attributes: mode(0o600),
                spec: specdata3 {
                    specdata1: 1,
                    specdata2: 3,
                },
            }),
        ),
        ("socket", mknoddata3::NF3SOCK(mode(0o600))),
    ] {
        let st = status(&mknod(&mut s.n, &d, what.as_bytes(), data)?);
        check(st != nfsstat3::NFS3_OK || c.t.expect_root, || {
            format!("MKNOD {what} was made")
        })?;
        out.push(line(&format!("MKNOD {what}"), st));
    }
    // A regular file is not MKNOD's (RFC 1813 3.3.11: BADTYPE).
    let mut b = crate::rpc::pack(&nfs_v3::wire::MKNOD3args {
        where_: dirop(&d, b"reg"),
        what: mknoddata3::NF3FIFO(sattr3::default()),
    })?;
    // The FIFO discriminant (7) made NF3REG (1), whose arm is void.
    let at = b.len() - sattr3_packed() - 4;
    b[at..at + 4].copy_from_slice(&1u32.to_be_bytes());
    b.truncate(at + 4);
    match s.n.call_bytes(P::NFSPROC3_MKNOD as u32, &b)? {
        Ok(res) => {
            let st = u32::from_be_bytes(res[..4].try_into().expect("status"));
            // NFS3ERR_BADTYPE (10007).
            check(st == 10007, || format!("MKNOD NF3REG: status {st}"))?;
            out.push(line("MKNOD NF3REG", "NFS3ERR_BADTYPE"));
        }
        Err(r) => return fail(format!("MKNOD NF3REG: {r}")),
    }
    s.close()?;
    Ok(out)
}

fn sattr3_packed() -> usize {
    use onc_xdr::Pack;
    sattr3::default().packed_size()
}

fn hard_link(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "hard-link")?;
    let d = s.dir.clone();
    let f = s.n.create(&d, b"one", 0o644)?;
    let r: LINK3res = s.n.call(
        P::NFSPROC3_LINK,
        &LINK3args {
            file: fh(&f),
            link: dirop(&d, b"two"),
        },
    )?;
    ok(r, "LINK")?;
    let g = s.n.lookup(&d, b"two")?;
    let (a, b) = (s.n.getattr(&f)?, s.n.getattr(&g)?);
    check(a.fileid == b.fileid, || "the link is another file".into())?;
    check(a.nlink == 2, || format!("nlink {}", a.nlink))?;
    s.n.write(&f, 0, b"shared")?;
    check(s.n.read(&g, 0, 100)?.0 == b"shared", || {
        "data not shared".into()
    })?;
    ok(s.n.remove_res(&d, b"one")?, "REMOVE one")?;
    check(s.n.read(&g, 0, 100)?.0 == b"shared", || {
        "the other name lost the data".into()
    })?;
    check(s.n.getattr(&g)?.nlink == 1, || "nlink after REMOVE".into())?;
    // A link to a directory is refused.
    let r: LINK3res = s.n.call(
        P::NFSPROC3_LINK,
        &LINK3args {
            file: fh(&d),
            link: dirop(&d, b"dirlink"),
        },
    )?;
    let st = status(&r);
    check(st != nfsstat3::NFS3_OK, || {
        "LINK of a directory was made".into()
    })?;
    s.close()?;
    Ok(vec![line("nlink", 2), line("LINK of a directory", st)])
}

fn remove(c: &Ctx) -> Lines {
    let mut s = Session::open(c, "remove")?;
    let d = s.dir.clone();
    s.n.create(&d, b"f", 0o644)?;
    ok(s.n.remove_res(&d, b"f")?, "REMOVE")?;
    expect_err(
        &s.n.lookup_res(&d, b"f")?,
        nfsstat3::NFS3ERR_NOENT,
        "LOOKUP after REMOVE",
    )?;
    expect_err(
        &s.n.remove_res(&d, b"f")?,
        nfsstat3::NFS3ERR_NOENT,
        "REMOVE again",
    )?;
    s.n.mkdir(&d, b"sub")?;
    let st = status(&s.n.remove_res(&d, b"sub")?);
    check(st != nfsstat3::NFS3_OK, || {
        "REMOVE of a directory was done".into()
    })?;
    s.close()?;
    Ok(vec![line("REMOVE of a directory", st)])
}

/// A program's UDP address: the explicit port, else the binder's answer.
fn udp_addr(
    c: &Ctx,
    explicit: Option<u16>,
    p: u32,
    v: u32,
) -> Result<std::net::SocketAddr, crate::Error> {
    let port = if let Some(p) = explicit {
        p
    } else {
        let g = crate::ctx::getport(c, p, v, Proto::Udp)?;
        let g = u16::try_from(g)
            .ok()
            .filter(|g| *g != 0)
            .ok_or_else(|| crate::Error::Skip(format!("program {p} is not registered over UDP")))?;
        c.t.map_port(g)
    };
    Ok(std::net::SocketAddr::new(c.ip, port))
}

fn udp_core(c: &Ctx) -> Lines {
    let cred = Cred::sys(c.t.uid, c.t.gid);
    let maddr = udp_addr(c, c.t.mount_port, prog::MOUNT, 3)?;
    let naddr = udp_addr(c, c.t.port, prog::NFS, 3)?;
    let mut m = Mount {
        c: c.client_as(maddr, Proto::Udp, cred.clone()),
    };
    let (root, _) = m.mount(c.t.export.as_bytes())?;
    let mut n = Nfs3 {
        c: c.client_as(naddr, Proto::Udp, cred),
    };
    n.c.call_raw(prog::NFS, 3, 0, &[])?;
    let scratch = crate::v3::walk_or_make(&mut n, &root, &c.t.scratch)?;
    let name = format!("udp-core-{}", std::process::id()).into_bytes();
    n.remove_tree(&scratch, &name)?;
    let d = n.mkdir(&scratch, &name)?;
    let f = n.create(&d, b"f", 0o644)?;
    let data = pattern(0, 8000);
    n.write(&f, 0, &data)?;
    let (back, _) = n.read(&f, 0, 8000)?;
    check(back == data, || "data read back over UDP differs".into())?;
    let list = n.list(&d)?;
    check(list.len() == 1, || format!("{} entries", list.len()))?;
    n.remove_tree(&scratch, &name)?;
    if status(&n.lookup_res(&scratch, &name)?) != nfsstat3::NFS3ERR_NOENT {
        return fail("the directory is still there");
    }
    Ok(vec![line(
        "over UDP",
        "MNT LOOKUP MKDIR CREATE WRITE READ READDIRPLUS REMOVE RMDIR",
    )])
}
