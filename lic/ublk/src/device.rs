// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! One ublk device: added and started through `/dev/ublk-control`, its
//! one queue served on a thread of its own, resized, stopped and
//! deleted.

use std::fs::{File, OpenOptions};
use std::os::fd::OwnedFd;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use tracing::{info, warn};

use crate::Error;
use crate::abi::{
    self, ATTR_FUA, ATTR_VOLATILE_CACHE, CMD_ADD_DEV, CMD_DEL_DEV, CMD_SET_PARAMS, CMD_START_DEV,
    CMD_STOP_DEV, CMD_UPDATE_SIZE, DevInfo, F_CMD_IOCTL_ENCODE, F_UPDATE_SIZE,
    IO_COMMIT_AND_FETCH_REQ, IO_FETCH_REQ, IoDesc,
};
use crate::sys::{Descs, Ring};
use crate::target::{BLOCK, Served, Target, start};

/// Tags of the queue.
pub const DEPTH: u16 = 32;
/// Largest request: 512 KiB.
pub const MAX_IO: u32 = 512 << 10;
/// `-ENODEV`: the kernel ended a fetch (`UBLK_IO_RES_ABORT`).
const ABORT: i32 = -19;

/// A running device.
pub struct Device {
    id: u32,
    ctrl: File,
    ring: Mutex<Ring>,
    queue: Option<JoinHandle<()>>,
}

fn fail(what: &str, e: impl std::fmt::Display) -> Error {
    Error::Kernel(format!("{what}: {e}"))
}

/// One control command: submitted, waited for, its result.
fn control(
    ring: &mut Ring,
    ctrl: &File,
    op: u32,
    cmd: &[u8; abi::CTRL_CMD_LEN],
    what: &str,
) -> Result<i32, Error> {
    use std::os::fd::AsRawFd;
    let sqe = abi::uring_cmd(ctrl.as_raw_fd(), op, u64::from(op), cmd);
    if !ring.push(&sqe) {
        return Err(Error::Kernel(format!("{what}: control ring full")));
    }
    ring.submit_and_wait(1).map_err(|e| fail(what, e))?;
    let c = ring
        .pop()
        .ok_or_else(|| Error::Kernel(format!("{what}: no completion")))?;
    let (_, res) = abi::cqe(&c);
    if res < 0 {
        return Err(Error::Kernel(format!("{what}: errno {}", -res)));
    }
    Ok(res)
}

impl Device {
    /// Add a device over `target`, serve its queue, start it. The kernel
    /// then shows `/dev/ublkbN`.
    pub fn start(target: Arc<dyn Target>) -> Result<Self, Error> {
        let ctrl = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/ublk-control")
            .map_err(|e| fail("/dev/ublk-control", e))?;
        let mut ring = Ring::new(4).map_err(|e| fail("control ring", e))?;
        let pid = rustix::process::getpid().as_raw_nonzero().get();
        let mut info = DevInfo {
            queues: 1,
            depth: DEPTH,
            max_io: MAX_IO,
            dev_id: u32::MAX,
            pid,
            flags: F_CMD_IOCTL_ENCODE | F_UPDATE_SIZE,
        }
        .encode();
        let cmd = abi::ctrl_cmd(u32::MAX, info.as_mut_ptr() as u64, abi::DEV_INFO_LEN16, 0);
        control(&mut ring, &ctrl, CMD_ADD_DEV, &cmd, "add device")?;
        let id = DevInfo::dev_id_of(&info);
        let mut dev = Self {
            id,
            ctrl,
            ring: Mutex::new(ring),
            queue: None,
        };
        if let Err(e) = dev.bring_up(target, pid) {
            let _ = dev.delete();
            return Err(e);
        }
        info!("ublk: device {id} started");
        Ok(dev)
    }

    fn bring_up(&mut self, target: Arc<dyn Target>, pid: i32) -> Result<(), Error> {
        let params = abi::params_basic(
            ATTR_VOLATILE_CACHE | ATTR_FUA,
            MAX_IO >> 9,
            target.blocks() * (BLOCK as u64 / 512),
        );
        let cmd = abi::ctrl_cmd(self.id, params.as_ptr() as u64, abi::PARAMS_BASIC_LEN16, 0);
        self.control(CMD_SET_PARAMS, &cmd, "set parameters")?;
        let cdev = open_char(self.id)?;
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::Builder::new()
            .name(format!("ublk{}", self.id))
            .spawn(move || queue(&cdev, &target, &tx))
            .map_err(|e| fail("queue thread", e))?;
        self.queue = Some(handle);
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(e),
            Err(_) => return Err(Error::Kernel("queue thread did not start".into())),
        }
        let cmd = abi::ctrl_cmd(self.id, 0, 0, u64::try_from(pid).unwrap_or(1));
        self.control(CMD_START_DEV, &cmd, "start device")?;
        Ok(())
    }

    fn control(&self, op: u32, cmd: &[u8; abi::CTRL_CMD_LEN], what: &str) -> Result<i32, Error> {
        let mut ring = self.ring.lock().unwrap_or_else(PoisonError::into_inner);
        control(&mut ring, &self.ctrl, op, cmd, what)
    }

    /// The device number.
    #[must_use]
    pub fn id(&self) -> u32 {
        self.id
    }

    /// The block device the kernel shows.
    #[must_use]
    pub fn block_path(&self) -> PathBuf {
        PathBuf::from(format!("/dev/ublkb{}", self.id))
    }

    /// Tell the kernel the device's new size.
    pub fn resize(&self, blocks: u64) -> Result<(), Error> {
        let cmd = abi::ctrl_cmd(self.id, 0, 0, blocks * (BLOCK as u64 / 512));
        self.control(CMD_UPDATE_SIZE, &cmd, "update size")
            .map(|_| ())
    }

    /// Stop the device (its fetches end), let the queue thread go and
    /// delete it.
    pub fn stop(mut self) -> Result<(), Error> {
        let cmd = abi::ctrl_cmd(self.id, 0, 0, 0);
        let stopped = self.control(CMD_STOP_DEV, &cmd, "stop device");
        if let Some(q) = self.queue.take() {
            let _ = q.join();
        }
        stopped?;
        self.delete()?;
        info!("ublk: device {} deleted", self.id);
        Ok(())
    }

    fn delete(&mut self) -> Result<(), Error> {
        let cmd = abi::ctrl_cmd(self.id, 0, 0, 0);
        self.control(CMD_DEL_DEV, &cmd, "delete device").map(|_| ())
    }
}

/// `/dev/ublkcN`, which devtmpfs makes as the device is added.
fn open_char(id: u32) -> Result<File, Error> {
    let path = format!("/dev/ublkc{id}");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(f) => return Ok(f),
            Err(e) if Instant::now() > deadline => return Err(fail(&path, e)),
            Err(_) => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

/// `user_data` of the read on the flusher's event descriptor.
const FLUSHED: u64 = u64::MAX;

/// A request waiting for a flush: its tag and what it answers then.
struct Waiting {
    tag: u16,
    ok: i32,
}

/// Flushes run on a thread of their own: a flush can take long (a host
/// writing back a large cache), and reads and writes must not queue
/// behind it. Each batch of waiting requests is answered by one flush
/// begun after they all came, then the queue is woken through an event
/// descriptor.
struct Flusher {
    tx: Option<mpsc::Sender<Waiting>>,
    done: Arc<Mutex<Vec<(u16, i32)>>>,
    event: Arc<OwnedFd>,
    thread: Option<JoinHandle<()>>,
}

impl Flusher {
    fn start(target: Arc<dyn Target>) -> Result<Self, Error> {
        let event = Arc::new(
            rustix::event::eventfd(0, rustix::event::EventfdFlags::CLOEXEC)
                .map_err(|e| fail("flush event", e))?,
        );
        let done: Arc<Mutex<Vec<(u16, i32)>>> = Arc::default();
        let (tx, rx) = mpsc::channel::<Waiting>();
        let (d, ev) = (Arc::clone(&done), Arc::clone(&event));
        let thread = std::thread::Builder::new()
            .name("ublk-flush".into())
            .spawn(move || {
                // Its memory reclaim must not wait on the device either.
                let _ = rustix::process::configure_io_flusher_behavior(true);
                while let Ok(first) = rx.recv() {
                    let mut batch = vec![first];
                    batch.extend(rx.try_iter());
                    let r = target.flush();
                    d.lock().unwrap_or_else(PoisonError::into_inner).extend(
                        batch
                            .iter()
                            .map(|w| (w.tag, r.map_or_else(|e| -e, |()| w.ok))),
                    );
                    let _ = rustix::io::write(&*ev, &1u64.to_ne_bytes());
                }
            })
            .map_err(|e| fail("flush thread", e))?;
        Ok(Self {
            tx: Some(tx),
            done,
            event,
            thread: Some(thread),
        })
    }

    fn send(&self, w: Waiting) -> Result<(), Waiting> {
        match &self.tx {
            Some(tx) => tx.send(w).map_err(|e| e.0),
            None => Err(w),
        }
    }

    fn take_done(&self) -> Vec<(u16, i32)> {
        std::mem::take(&mut *self.done.lock().unwrap_or_else(PoisonError::into_inner))
    }

    /// Let the thread end once its batches are flushed.
    fn stop(&mut self) {
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// Commit tag `tag`'s answer and fetch its next request.
fn commit(ring: &mut Ring, fd: i32, buf: &mut blockdev::AlignedBuf, tag: u16, result: i32) {
    let addr = buf.as_mut_ptr() as u64;
    let sqe = abi::uring_cmd(
        fd,
        IO_COMMIT_AND_FETCH_REQ,
        u64::from(tag),
        &abi::io_cmd(0, tag, result, addr),
    );
    ring.push(&sqe);
}

/// The queue: every tag fetched, then each request served and committed
/// with the next fetch, until the kernel aborts the fetches. Flushes,
/// and the flush after a write with FUA, are the flusher's; their tags
/// are committed when it is done.
fn queue(cdev: &File, target: &Arc<dyn Target>, ready: &mpsc::Sender<Result<(), Error>>) {
    // Memory reclaim must not wait on the device this thread serves.
    if let Err(e) = rustix::process::configure_io_flusher_behavior(true) {
        warn!("ublk: not an I/O flusher: {e}");
    }
    let setup = || -> Result<(Ring, Descs, Flusher), Error> {
        // Room for every tag's commit and the event's read in one round.
        let ring = Ring::new(u32::from(DEPTH) * 2).map_err(|e| fail("queue ring", e))?;
        let descs = Descs::new(cdev, usize::from(DEPTH)).map_err(|e| fail("descriptors", e))?;
        Ok((ring, descs, Flusher::start(Arc::clone(target))?))
    };
    let (mut ring, descs, mut flusher) = match setup() {
        Ok(r) => r,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let blocks = MAX_IO as usize / BLOCK;
    let mut bufs: Vec<blockdev::AlignedBuf> = (0..DEPTH)
        .map(|_| blockdev::AlignedBuf::new(blocks))
        .collect();
    let (fd, efd) = {
        use std::os::fd::AsRawFd;
        (cdev.as_raw_fd(), flusher.event.as_raw_fd())
    };
    // The event's count lands here; it lives until the read completes.
    let mut count = Box::new([0u8; 8]);
    let arm = |ring: &mut Ring, count: &mut [u8; 8]| {
        ring.push(&abi::read_sqe(efd, count.as_mut_ptr() as u64, 8, FLUSHED))
    };
    for tag in 0..DEPTH {
        let addr = bufs[usize::from(tag)].as_mut_ptr() as u64;
        let sqe = abi::uring_cmd(
            fd,
            IO_FETCH_REQ,
            u64::from(tag),
            &abi::io_cmd(0, tag, 0, addr),
        );
        ring.push(&sqe);
    }
    let mut armed = arm(&mut ring, &mut count);
    if let Err(e) = ring.submit_and_wait(0) {
        let _ = ready.send(Err(fail("fetch", e)));
        return;
    }
    let _ = ready.send(Ok(()));
    let mut live = usize::from(DEPTH);
    while live > 0 {
        if let Err(e) = ring.submit_and_wait(1) {
            warn!("ublk: queue: {e}");
            break;
        }
        while let Some(c) = ring.pop() {
            let (user, res) = abi::cqe(&c);
            if user == FLUSHED {
                for (tag, result) in flusher.take_done() {
                    if result < 0 {
                        warn!("ublk: flush for tag {tag} failed: errno {}", -result);
                    }
                    commit(&mut ring, fd, &mut bufs[usize::from(tag)], tag, result);
                }
                armed = arm(&mut ring, &mut count);
                continue;
            }
            let tag = u16::try_from(user).unwrap_or(0);
            if res != 0 {
                if res != ABORT {
                    warn!("ublk: tag {tag}: fetch ended with {res}");
                }
                live -= 1;
                continue;
            }
            let d = IoDesc::decode(&descs.get(usize::from(tag)));
            let result = match start(target.as_ref(), &d, &mut bufs[usize::from(tag)]) {
                Served::Done(r) => r,
                Served::Flush(ok) => match flusher.send(Waiting { tag, ok }) {
                    Ok(()) => continue,
                    // No flusher: flushed here.
                    Err(w) => target.flush().map_or_else(|e| -e, |()| w.ok),
                },
            };
            if result < 0 {
                warn!("ublk: request {d:?} failed: errno {}", -result);
            }
            commit(&mut ring, fd, &mut bufs[usize::from(tag)], tag, result);
        }
    }
    flusher.stop();
    // The event's read completes before its count goes.
    let deadline = Instant::now() + Duration::from_secs(5);
    if armed {
        let _ = rustix::io::write(&*flusher.event, &1u64.to_ne_bytes());
    }
    while armed && Instant::now() < deadline {
        if ring.submit_and_wait(1).is_err() {
            break;
        }
        while let Some(c) = ring.pop() {
            if abi::cqe(&c).0 == FLUSHED {
                armed = false;
            }
        }
    }
    drop(count);
}
