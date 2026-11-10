// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! Files and directories: round trip, listing, rename, delete, change notify.

use std::time::Duration;

use smb::{FileRenameInformation, NotifyAction, NotifyFilter, ReadAt, WriteAt};

use super::scratch::{list, remove};
use super::{Lines, Scratch};
use crate::conn::{Share, base_config};
use crate::error::check;
use crate::{Error, Target};

/// `STATUS_OBJECT_NAME_NOT_FOUND`.
pub const NAME_NOT_FOUND: u32 = 0xC000_0034;

/// Writes `data` to a new file, reads it back through a new open, removes it.
pub async fn write_read_remove(s: &Share, path: &str, data: &[u8]) -> Result<(), Error> {
    put(s, path, data).await?;
    let got = get(s, path).await?;
    check(got == data, || format!("{path}: read {} bytes back, not the {} written", got.len(), data.len()))?;
    remove(s, path).await
}

/// Creates `path` holding `data`.
pub async fn put(s: &Share, path: &str, data: &[u8]) -> Result<(), Error> {
    let f = s.create_file(path).await?;
    let n = f.write_at(data, 0).await?;
    check(n == data.len(), || format!("wrote {n} of {} bytes", data.len()))?;
    f.close().await?;
    Ok(())
}

/// The whole of `path`.
pub async fn get(s: &Share, path: &str) -> Result<Vec<u8>, Error> {
    let f = s.open_file(path, false).await?;
    let len = usize::try_from(smb::GetLen::get_len(&f).await?).map_err(|_| Error::Check("file too large".into()))?;
    let mut buf = vec![0u8; len];
    let mut at = 0;
    while at < len {
        let n = f.read_at(&mut buf[at..], at as u64).await?;
        if n == 0 {
            break;
        }
        at += n;
    }
    buf.truncate(at);
    f.close().await?;
    Ok(buf)
}

/// Expects opening `path` to fail with `STATUS_OBJECT_NAME_NOT_FOUND`.
async fn absent(s: &Share, path: &str) -> Result<(), Error> {
    match s.open_file(path, false).await {
        Ok(f) => {
            f.close().await?;
            Err(Error::Check(format!("{path} still opens")))
        }
        Err(e) => check(e.status() == Some(NAME_NOT_FOUND), || format!("{path}: expected 0x{NAME_NOT_FOUND:08X}, got {e}")),
    }
}

/// Text and every byte value, written, read back through a new open.
pub async fn round_trip(t: &Target, sc: &Scratch) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    let mut data = b"QS/400 NetServer round trip\r\n".to_vec();
    data.extend(0..=255u8);
    let path = sc.file("round-trip.bin");
    put(&s, &path, &data).await?;
    let got = get(&s, &path).await?;
    check(got == data, || format!("read back {} bytes, differing from the {} written", got.len(), data.len()))?;
    remove(&s, &path).await?;
    s.close().await?;
    Ok(vec![format!("bytes: {}", data.len())])
}

/// Files and a directory made, then listed with `*`.
pub async fn dir_list(t: &Target, sc: &Scratch) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    let dir = sc.file("listing");
    s.create_dir(&dir).await?;
    let want = ["Alpha.txt", "beta.TXT", "Gamma Delta.dat"];
    for name in want {
        put(&s, &format!("{dir}\\{name}"), name.as_bytes()).await?;
    }
    s.create_dir(&format!("{dir}\\sub")).await?;
    let mut got = list(&s, &dir).await?;
    got.sort();
    let mut expect: Vec<String> = want.iter().map(|n| (*n).to_string()).collect();
    expect.push("sub".into());
    expect.sort();
    check(got == expect, || format!("listed {got:?}, expected {expect:?}"))?;
    for name in &got {
        remove(&s, &format!("{dir}\\{name}")).await?;
    }
    remove(&s, &dir).await?;
    s.close().await?;
    Ok(vec![format!("listed: {}", got.join(", "))])
}

/// A rename on an open handle; the old name is gone, the data is kept.
pub async fn rename(t: &Target, sc: &Scratch) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    let (from, to) = (sc.file("rename-from.txt"), sc.file("rename-to.txt"));
    put(&s, &from, b"renamed").await?;
    let f = s.open_file(&from, true).await?;
    f.set_info(FileRenameInformation {
        replace_if_exists: false.into(),
        root_directory: 0,
        file_name: to.as_str().into(),
    })
    .await?;
    f.close().await?;
    absent(&s, &from).await?;
    check(get(&s, &to).await? == b"renamed", || "renamed file lost its data".into())?;
    remove(&s, &to).await?;
    s.close().await?;
    Ok(vec![format!("{from} -> {to}")])
}

/// Delete on close; the name no longer opens.
pub async fn delete(t: &Target, sc: &Scratch) -> Lines {
    let s = Share::connect(t, base_config(t)).await?;
    let path = sc.file("delete-me.txt");
    put(&s, &path, b"gone").await?;
    remove(&s, &path).await?;
    absent(&s, &path).await?;
    s.close().await?;
    Ok(vec![format!("deleted: {path}")])
}

/// A watch on the run directory from one connection sees a file another
/// connection creates.
pub async fn change_notify(t: &Target, sc: &Scratch) -> Lines {
    notify_between(t, sc, "notify", Share::connect(t, base_config(t)).await?).await
}

/// As [`change_notify`], the writer being another machine (another
/// `ClientGuid`, so another `QZLSFILE` job on QS/400).
pub async fn change_notify_two_machines(t: &Target, sc: &Scratch) -> Lines {
    let writer = Share::connect_machine(t, base_config(t), crate::conn::other_machine()).await?;
    notify_between(t, sc, "notify2", writer).await
}

async fn notify_between(t: &Target, sc: &Scratch, name: &str, writer: Share) -> Lines {
    let dir_name = sc.file(name);
    let watcher = Share::connect(t, base_config(t)).await?;
    watcher.create_dir(&dir_name).await?;
    let dir = watcher.open_dir(&dir_name).await?;
    let filter = NotifyFilter::new().with_file_name(true).with_dir_name(true);
    let watch = dir.watch_timeout(filter, false, Duration::from_secs(20));
    let make = async {
        tokio::time::sleep(Duration::from_millis(500)).await;
        put(&writer, &format!("{dir_name}\\added.txt"), b"x").await
    };
    let (events, created) = tokio::join!(watch, make);
    created?;
    let seen = events?;
    let lines: Vec<String> = seen.iter().map(|n| format!("{:?} {}", n.action, n.file_name)).collect();
    check(
        seen.iter().any(|n| matches!(n.action, NotifyAction::Added) && n.file_name == "added.txt"),
        || format!("no Added added.txt in {lines:?}"),
    )?;
    dir.close().await?;
    remove(&writer, &format!("{dir_name}\\added.txt")).await?;
    remove(&writer, &dir_name).await?;
    writer.close().await?;
    watcher.close().await?;
    Ok(lines)
}
