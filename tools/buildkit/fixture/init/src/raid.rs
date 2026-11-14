// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A protected set with no object store: two image files mirrored by
//! `protect`, served as a kernel block device by `ublk`, an xfs made by
//! `xfsfmt` on it.
//!
//! The set is served by a process of its own (`/init serve ...`, this
//! program again), as an appliance's storage server would be; PID 1 is
//! its caller and keeps the record of the members. So the proof can
//! take the server away: the file system stays mounted, its requests
//! wait in the kernel, and the next server recovers the device. The set
//! keeps a write-intent map in a file, so a server killed under write
//! load is followed by a copy of the regions being written only.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use blockdev::{BlockDev, LinuxBlockDev};
use protect::UnitState;
use protect::intent::IntentStore;
use protect::mirror::{RESUME_CHUNK, ResumeState};
use protect::set::{Intent, MirrorMember, MirrorSet, ResumeJournal, ResyncProgress};
use rustix::mount::{MountFlags, UnmountFlags, mount, unmount};
use ublk::{BlockTarget, Device, Options};
use uuid::uuid;

/// The set's identity, QSDEMO's own.
const SET: [u8; 16] = *b"QSDEMO-MIRROR-01";
/// Magic of QSDEMO's resume map.
const RESUME_MAGIC: &[u8; 8] = b"QSDEMORS";
/// Each member: 96 MiB.
const MEMBER_BYTES: u64 = 96 << 20;
const BLOCK: u64 = 4096;
/// Where the data partition and the set's file system are mounted.
const DATA: &str = "/data";
const MOUNT: &str = "/set";
const RESUME_MAP: &str = "/data/resume.map";
/// QSDEMO's write-intent map: its magic and its file.
const INTENT_MAGIC: [u8; 8] = *b"QSDEMOWI";
const INTENT_MAP: &str = "/data/set.intent";

fn text(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The resume map in a file: QSDEMO's journal.
struct FileJournal;

impl ResumeJournal for FileJournal {
    fn load(&mut self, unit_uuid: [u8; 16]) -> Option<ResumeState> {
        let bytes = fs::read(RESUME_MAP).ok()?;
        ResumeState::decode(RESUME_MAGIC, &bytes).filter(|s| s.unit_uuid == unit_uuid)
    }

    fn save(&mut self, state: &ResumeState) -> std::io::Result<()> {
        let tmp = format!("{RESUME_MAP}.new");
        let mut f = File::create(&tmp)?;
        f.write_all(&state.encode(RESUME_MAGIC))?;
        f.sync_all()?;
        fs::rename(&tmp, RESUME_MAP)
    }

    fn clear(&mut self) -> std::io::Result<()> {
        match fs::remove_file(RESUME_MAP) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// The write-intent map in a file: QSDEMO's store for it.
struct FileIntent;

impl IntentStore for FileIntent {
    fn read_all(&mut self) -> std::io::Result<Vec<u8>> {
        match fs::read(INTENT_MAP) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            r => r,
        }
    }

    fn write(&mut self, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
        let f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(INTENT_MAP)?;
        f.write_all_at(bytes, offset)?;
        f.sync_data()
    }
}

/// The set's map: a megabyte a region, so the fixture's small set has
/// regions enough to tell a few from all; cleared at each flush.
fn intent(fresh: bool) -> Intent {
    let mut i = Intent::new(INTENT_MAGIC, Box::new(FileIntent));
    i.region_blocks = RESUME_CHUNK;
    i.clear_delay = Duration::ZERO;
    i.fresh = fresh;
    i
}

/// Run the copy between active members to its end, beside the I/O.
fn resync(set: &MirrorSet) -> Option<JoinHandle<Result<ResyncProgress, String>>> {
    let p = set.resync_progress()?;
    if p.complete {
        return None;
    }
    let set = set.clone();
    Some(std::thread::spawn(move || {
        loop {
            match set.resync_step(4).map_err(text)? {
                Some(p) if p.complete => return Ok(p),
                Some(_) => {}
                None => return Err("no copy".into()),
            }
        }
    }))
}

fn open_member(path: &str) -> Result<Box<dyn BlockDev>, String> {
    Ok(Box::new(
        LinuxBlockDev::open(Path::new(path)).map_err(|e| format!("{path}: {e}"))?,
    ))
}

/// The server: `serve start|recover ID BLOCKS PATH STATE PATH STATE`,
/// a path of `-` for a member that is not there. It answers each line
/// on its standard input with `OK ...` or `ERR ...`.
pub fn serve(args: &[String]) -> ! {
    if let Err(e) = serve_set(args) {
        println!("ERR {e}");
        std::process::exit(1);
    }
    std::process::exit(0);
}

fn serve_set(args: &[String]) -> Result<(), String> {
    let [mode, id, blocks, members @ ..] = args else {
        return Err("serve MODE ID BLOCKS (PATH STATE)...".into());
    };
    let blocks: u64 = blocks.parse().map_err(text)?;
    let mut list = Vec::new();
    for (i, m) in members.chunks(2).enumerate() {
        let [path, state] = m else {
            return Err("a member is a path and a state".into());
        };
        list.push(MirrorMember {
            uuid: [u8::try_from(i).map_err(text)? + 1; 16],
            state: match state.as_str() {
                "active" => UnitState::Active,
                "missing" => UnitState::Missing,
                other => return Err(format!("state {other}")),
            },
            dev: if path == "-" {
                None
            } else {
                Some(open_member(path)?)
            },
        });
    }
    let set =
        MirrorSet::open_with(SET, blocks, list, Some(intent(mode == "start"))).map_err(text)?;
    let mut copy = resync(&set);
    let target = Arc::new(BlockTarget(set.clone()));
    let mut device = Some(
        match mode.as_str() {
            "start" => Device::start_with(target, Options { recovery: true }),
            "recover" => Device::recover(id.parse().map_err(text)?, target),
            other => return Err(format!("mode {other}")),
        }
        .map_err(text)?,
    );
    println!("OK {}", device.as_ref().map_or(0, Device::id));

    for line in std::io::stdin().lines() {
        let line = line.map_err(text)?;
        let words: Vec<&str> = line.split_whitespace().collect();
        let answer = match words.as_slice() {
            ["members"] => Ok(set
                .members()
                .iter()
                .map(|m| m.state.to_string())
                .collect::<Vec<_>>()
                .join(",")),
            ["detach", n] => n
                .parse()
                .map_err(text)
                .and_then(|n| set.detach(n).map_err(text))
                .map(|_| String::new()),
            ["replace", n, path] => replace(&set, n, path),
            ["resync"] => match copy.take() {
                None => Ok("none".into()),
                Some(h) => h
                    .join()
                    .map_err(|_| "the copy panicked".to_string())
                    .and_then(|r| r)
                    .map(|p| format!("{} {} {}", p.chunks_copied, p.chunks_dirty, p.chunks)),
            },
            ["stop"] => {
                let stopped = set
                    .close()
                    .map_err(text)
                    .and_then(|()| device.take().map_or(Ok(()), Device::stop).map_err(text));
                println!(
                    "{}",
                    stopped.map_or_else(|e| format!("ERR {e}"), |()| "OK".into())
                );
                return Ok(());
            }
            _ => Err(format!("unknown request {line:?}")),
        };
        match answer {
            Ok(a) => println!("OK {a}"),
            Err(e) => println!("ERR {e}"),
        }
    }
    // The caller is gone without a stop: the device stays for the next
    // server.
    Ok(())
}

/// A new disk in a member's place, brought in step.
fn replace(set: &MirrorSet, n: &str, path: &str) -> Result<String, String> {
    let n: usize = n.parse().map_err(text)?;
    set.replace(
        n,
        open_member(path)?,
        [0xA0 + u8::try_from(n).map_err(text)?; 16],
    )
    .map_err(text)?;
    let mut journal = FileJournal;
    let mut resume = set.resume(n, &mut journal).map_err(text)?;
    loop {
        let p = set
            .resume_step(&mut resume, &mut journal, 8)
            .map_err(text)?;
        if p.complete {
            return Ok(format!("{} {}", p.chunks_done, p.chunks));
        }
    }
}

/// The server as its caller holds it.
struct Server {
    child: Child,
    to: ChildStdin,
    from: BufReader<ChildStdout>,
}

impl Server {
    /// Start `/init serve ARGS` and take its first answer.
    fn spawn(args: &[&str]) -> Result<(Self, String), String> {
        let mut child = Command::new("/proc/self/exe")
            .arg("serve")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("server: {e}"))?;
        let to = child.stdin.take().ok_or("no pipe to the server")?;
        let from = BufReader::new(child.stdout.take().ok_or("no pipe from the server")?);
        let mut s = Self { child, to, from };
        let first = s.answer()?;
        Ok((s, first))
    }

    fn answer(&mut self) -> Result<String, String> {
        let mut line = String::new();
        self.from.read_line(&mut line).map_err(text)?;
        match line.trim().split_once(' ').unwrap_or((line.trim(), "")) {
            ("OK", rest) => Ok(rest.to_string()),
            _ => Err(format!("the server answered {:?}", line.trim())),
        }
    }

    fn ask(&mut self, request: &str) -> Result<String, String> {
        writeln!(self.to, "{request}").map_err(text)?;
        self.answer()
    }

    /// The server dies as a crash would take it: no stop, no goodbye.
    fn kill(mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    fn stop(mut self) -> Result<(), String> {
        self.ask("stop")?;
        self.child.wait().map_err(text)?;
        Ok(())
    }
}

/// The device node of the partition named `name`.
fn partition(name: &str) -> Result<String, String> {
    let want = format!("PARTNAME={name}");
    for e in fs::read_dir("/sys/class/block").map_err(text)?.flatten() {
        let uevent = fs::read_to_string(e.path().join("uevent")).unwrap_or_default();
        if uevent.lines().any(|l| l == want) {
            return Ok(format!("/dev/{}", e.file_name().to_string_lossy()));
        }
    }
    Err(format!("no partition named {name}"))
}

fn mount_xfs(dev: &str, at: &str) -> Result<(), String> {
    fs::create_dir_all(at).map_err(text)?;
    mount(dev, at, "xfs", MountFlags::empty(), None::<&std::ffi::CStr>)
        .map_err(|e| format!("mount {dev} at {at}: {e}"))
}

/// A megabyte that says which megabyte it is.
fn pattern(seed: u8) -> Vec<u8> {
    (0..1usize << 20)
        .map(|i| seed.wrapping_add((i % 251) as u8))
        .collect()
}

fn write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    let mut f = File::create(path).map_err(|e| format!("{path}: {e}"))?;
    f.write_all(bytes).map_err(text)?;
    f.sync_all().map_err(text)
}

/// Read a file from the device, not from memory.
fn read_file(path: &str) -> Result<Vec<u8>, String> {
    rustix::fs::sync();
    fs::write("/proc/sys/vm/drop_caches", "3").map_err(|e| format!("drop caches: {e}"))?;
    fs::read(path).map_err(|e| format!("{path}: {e}"))
}

fn image(path: &str) -> Result<(), String> {
    File::create(path)
        .and_then(|f| f.set_len(MEMBER_BYTES))
        .map_err(|e| format!("{path}: {e}"))
}

/// Wait for a device node to appear.
fn wait_for(node: &str) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !Path::new(node).exists() {
        if Instant::now() > deadline {
            return Err(format!("{node} did not appear"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

/// Files written over and over, each synced, until told to stop: what
/// each holds last.
struct Load {
    stop: Arc<AtomicBool>,
    thread: JoinHandle<Result<BTreeMap<String, u8>, String>>,
}

impl Load {
    fn start() -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            let mut last = BTreeMap::new();
            for i in 0u32.. {
                if flag.load(Ordering::Relaxed) {
                    break;
                }
                let name = format!("/set/load-{}", i % 8);
                let seed = u8::try_from(i % 200).map_err(text)? + 3;
                write_file(&name, &pattern(seed))?;
                last.insert(name, seed);
            }
            Ok(last)
        });
        Self { stop, thread }
    }

    fn finish(self) -> Result<BTreeMap<String, u8>, String> {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.join().map_err(|_| "the load panicked")?
    }
}

/// Whether two members hold the same bytes.
fn same(a: &str, b: &str) -> Result<bool, String> {
    let open = |p: &str| File::open(p).map_err(|e| format!("{p}: {e}"));
    let (mut a, mut b) = (open(a)?, open(b)?);
    let (mut x, mut y) = (vec![0u8; 1 << 20], vec![0u8; 1 << 20]);
    for _ in 0..MEMBER_BYTES >> 20 {
        a.read_exact(&mut x).map_err(text)?;
        b.read_exact(&mut y).map_err(text)?;
        if x != y {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Make the set, use it, break it, mend it. One line for each thing
/// that held.
pub fn prove() -> Result<Vec<String>, String> {
    let mut held = Vec::new();
    mount_xfs(&partition("QSDEMO-DATA")?, DATA)?;
    let (m0, m1, m2) = ("/data/m0.img", "/data/m1.img", "/data/m2.img");
    image(m0)?;
    image(m1)?;
    let blocks = (MEMBER_BYTES / BLOCK).to_string();

    // Two new, equal members: both active.
    let (mut server, id) = Server::spawn(&["start", "-", &blocks, m0, "active", m1, "active"])?;
    let node = format!("/dev/ublkb{id}");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !Path::new(&node).exists() {
        if Instant::now() > deadline {
            return Err(format!("{node} did not appear"));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let mut dev = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&node)
        .map_err(|e| format!("{node}: {e}"))?;
    let mut opts = xfsfmt::Options::new(
        uuid!("5d0f6f0e-3a57-4c1b-8a55-515344454d04"),
        xfsfmt::Timestamp { secs: 0, nsecs: 0 },
    );
    opts.label = "QSDEMOSET".into();
    opts.sector_size = 4096;
    xfsfmt::format(&mut dev, MEMBER_BYTES, &opts).map_err(text)?;
    dev.sync_all().map_err(text)?;
    drop(dev);
    mount_xfs(&node, MOUNT)?;
    let one = pattern(1);
    write_file("/set/one", &one)?;
    held.push(format!(
        "mirrored set of 2 image files, {blocks} blocks, served as {node}; xfs made, mounted and written"
    ));

    // A member is removed: the file is gone, the data is not.
    server.ask("detach 1")?;
    fs::remove_file(m1).map_err(text)?;
    if read_file("/set/one")? != one {
        return Err("the file read back differs with a member removed".into());
    }
    let members = server.ask("members")?;
    held.push(format!(
        "member 1 removed ({members}); {} bytes read back from the member left",
        one.len()
    ));

    // The server is killed. A read begun now waits in the kernel for
    // the next server, which recovers the device under the mount.
    server.kill();
    let waiting = std::thread::spawn(|| read_file("/set/one"));
    std::thread::sleep(Duration::from_millis(500));
    let (mut server, again) =
        Server::spawn(&["recover", &id, &blocks, m0, "active", "-", "missing"])?;
    if again != id {
        return Err(format!("recovered device {again}, not {id}"));
    }
    if waiting.join().map_err(|_| "the waiting read panicked")?? != one {
        return Err("the read that waited for the new server differs".into());
    }
    let two = pattern(2);
    write_file("/set/two", &two)?;
    if read_file("/set/two")? != two {
        return Err("the file written after the restart differs".into());
    }
    held.push(format!(
        "server killed and restarted: {node} recovered under the mounted xfs, a waiting read answered, written again"
    ));

    // A new disk in the removed member's place, rebuilt.
    image(m2)?;
    let rebuilt = server.ask(&format!("replace 1 {m2}"))?;
    let members = server.ask("members")?;
    if members != "Active,Active" {
        return Err(format!("after the rebuild the members are {members}"));
    }
    unmount(MOUNT, UnmountFlags::empty()).map_err(|e| format!("unmount {MOUNT}: {e}"))?;
    server.stop()?;
    if !same(m0, m2)? {
        return Err("the rebuilt member differs from the other".into());
    }
    let (done, of) = rebuilt.split_once(' ').unwrap_or((&rebuilt, "?"));
    held.push(format!(
        "member 1 replaced and rebuilt, {done} of {of} chunks; the members are identical"
    ));

    held.push(under_load(m0, m2, &blocks, &two)?);
    let _ = unmount(DATA, UnmountFlags::empty());
    Ok(held)
}

/// The set served again, both members active, files written and synced
/// without a pause, and the server killed in the middle. The next
/// server copies the regions that were being written, not the set.
fn under_load(m0: &str, m2: &str, blocks: &str, two: &[u8]) -> Result<String, String> {
    let (server, id) = Server::spawn(&["start", "-", blocks, m0, "active", m2, "active"])?;
    let node = format!("/dev/ublkb{id}");
    wait_for(&node)?;
    mount_xfs(&node, MOUNT)?;
    let load = Load::start();
    std::thread::sleep(Duration::from_millis(1500));
    server.kill();
    std::thread::sleep(Duration::from_millis(300));
    let (mut server, again) = Server::spawn(&["recover", &id, blocks, m0, "active", m2, "active"])?;
    if again != id {
        return Err(format!("recovered device {again}, not {id}"));
    }
    std::thread::sleep(Duration::from_millis(1500));
    let last = load.finish()?;
    let copied = server.ask("resync")?;
    if copied == "none" {
        return Err("the load left no region dirty".into());
    }
    let [copied, dirty, chunks] = copied
        .split(' ')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("the copy answered {copied:?}"))?[..]
    else {
        return Err(format!("the copy answered {copied:?}"));
    };
    if copied >= chunks {
        return Err(format!(
            "the copy took {copied} of {chunks} chunks: the whole set"
        ));
    }
    for (name, seed) in &last {
        if read_file(name)? != pattern(*seed) {
            return Err(format!("{name} differs after the copy"));
        }
    }
    if read_file("/set/two")? != two {
        return Err("a file written before the load differs after the copy".into());
    }
    unmount(MOUNT, UnmountFlags::empty()).map_err(|e| format!("unmount {MOUNT}: {e}"))?;
    server.stop()?;
    if !same(m0, m2)? {
        return Err("the members differ after the copy".into());
    }
    Ok(format!(
        "server killed under write load: {copied} of {chunks} chunks copied ({dirty} dirty), {} files intact; the members are identical",
        last.len()
    ))
}
