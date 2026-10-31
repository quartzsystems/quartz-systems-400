// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The real ioctls: MTIOCTOP, MTIOCGET and MTIOCPOS on an `st` node and
//! `SG_IO` on an `sg` node. Each is one `ioctl` on a structure this module
//! owns; this is the crate's only `unsafe`.

#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::mem::size_of;
use std::os::fd::AsRawFd;
use std::path::Path;

use super::ioctl::{MtGet, MtOp, SgDir, SgIoctl, SgReply, StIoctl};

/// `struct mtop` (`linux/mtio.h`).
#[repr(C)]
struct RawMtop {
    mt_op: libc::c_short,
    mt_count: libc::c_int,
}

/// `struct mtget`, field names as in C.
#[allow(clippy::struct_field_names)]
#[repr(C)]
#[derive(Default)]
struct RawMtget {
    mt_type: libc::c_long,
    mt_resid: libc::c_long,
    mt_dsreg: libc::c_long,
    mt_gstat: libc::c_long,
    mt_erreg: libc::c_long,
    mt_fileno: libc::c_int,
    mt_blkno: libc::c_int,
}

/// `struct mtpos`.
#[repr(C)]
#[derive(Default)]
struct RawMtpos {
    mt_blkno: libc::c_long,
}

/// `struct sg_io_hdr` (`scsi/sg.h`).
#[repr(C)]
struct RawSgIoHdr {
    interface_id: libc::c_int,
    dxfer_direction: libc::c_int,
    cmd_len: u8,
    mx_sb_len: u8,
    iovec_count: u16,
    dxfer_len: u32,
    dxferp: *mut libc::c_void,
    cmdp: *const u8,
    sbp: *mut u8,
    timeout: u32,
    flags: u32,
    pack_id: libc::c_int,
    usr_ptr: *mut libc::c_void,
    status: u8,
    masked_status: u8,
    msg_status: u8,
    sb_len_wr: u8,
    host_status: u16,
    driver_status: u16,
    resid: libc::c_int,
    duration: u32,
    info: u32,
}

const _: () = assert!(size_of::<RawMtop>() == 8);
const _: () = assert!(size_of::<RawMtget>() == 48);
const _: () = assert!(size_of::<RawMtpos>() == 8);
const _: () = assert!(size_of::<RawSgIoHdr>() == 88);

/// `_IOC(dir, type, nr, size)`.
const fn ioc(dir: u32, ty: u8, nr: u8, size: usize) -> u32 {
    #[allow(clippy::cast_possible_truncation)]
    let size = size as u32;
    (dir << 30) | (size << 16) | ((ty as u32) << 8) | nr as u32
}

const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;
const MTIOCTOP: u32 = ioc(IOC_WRITE, b'm', 1, size_of::<RawMtop>());
const MTIOCGET: u32 = ioc(IOC_READ, b'm', 2, size_of::<RawMtget>());
const MTIOCPOS: u32 = ioc(IOC_READ, b'm', 3, size_of::<RawMtpos>());
const SG_IO: u32 = 0x2285;
const SG_DXFER_NONE: libc::c_int = -1;
const SG_DXFER_TO_DEV: libc::c_int = -2;
const SG_DXFER_FROM_DEV: libc::c_int = -3;

/// The request number in the C library's type (`int` on musl).
#[allow(
    clippy::cast_possible_wrap,
    clippy::cast_lossless,
    clippy::unnecessary_cast
)]
fn request(n: u32) -> libc::Ioctl {
    n as libc::Ioctl
}

fn check(r: libc::c_int) -> io::Result<()> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn open(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .or_else(|_| OpenOptions::new().read(true).open(path))
}

/// An open `st` node (use the no-rewind `/dev/nstN`).
#[derive(Debug)]
pub struct LinuxSt {
    file: File,
}

impl LinuxSt {
    /// Open a tape node, read-write when the volume allows it.
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self { file: open(path)? })
    }
}

impl StIoctl for LinuxSt {
    fn mtop(&mut self, op: MtOp, count: i32) -> io::Result<()> {
        let arg = RawMtop {
            mt_op: op.code(),
            mt_count: count,
        };
        // SAFETY: MTIOCTOP reads one `struct mtop`, which `arg` is
        // (layout checked above); the descriptor is open for the call.
        check(unsafe { libc::ioctl(self.file.as_raw_fd(), request(MTIOCTOP), &raw const arg) })
    }

    fn mtget(&mut self) -> io::Result<MtGet> {
        let mut g = RawMtget::default();
        // SAFETY: MTIOCGET writes one `struct mtget` into `g`, which
        // lives for the call; the descriptor is open.
        check(unsafe { libc::ioctl(self.file.as_raw_fd(), request(MTIOCGET), &raw mut g) })?;
        Ok(MtGet {
            mt_type: g.mt_type,
            resid: g.mt_resid,
            dsreg: g.mt_dsreg.cast_unsigned(),
            gstat: g.mt_gstat.cast_unsigned(),
            erreg: g.mt_erreg,
            fileno: g.mt_fileno,
            blkno: g.mt_blkno,
        })
    }

    fn mtpos(&mut self) -> io::Result<u64> {
        let mut p = RawMtpos::default();
        // SAFETY: MTIOCPOS writes one `struct mtpos` into `p`, which
        // lives for the call; the descriptor is open.
        check(unsafe { libc::ioctl(self.file.as_raw_fd(), request(MTIOCPOS), &raw mut p) })?;
        Ok(p.mt_blkno.cast_unsigned())
    }

    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.file.read(buf)
    }

    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.file.write(data)
    }
}

/// An open `sg` node.
#[derive(Debug)]
pub struct LinuxSg {
    file: File,
}

impl LinuxSg {
    /// Open a generic SCSI node.
    pub fn open(path: &Path) -> io::Result<Self> {
        Ok(Self { file: open(path)? })
    }
}

impl SgIoctl for LinuxSg {
    fn sg_io(
        &mut self,
        cdb: &[u8],
        dir: SgDir,
        data: &mut [u8],
        timeout_ms: u32,
    ) -> io::Result<SgReply> {
        let mut sense = [0u8; 64];
        let too_big = || io::Error::new(io::ErrorKind::InvalidInput, "SG_IO buffer too large");
        let mut hdr = RawSgIoHdr {
            interface_id: libc::c_int::from(b'S'),
            dxfer_direction: match dir {
                SgDir::None => SG_DXFER_NONE,
                SgDir::ToDevice => SG_DXFER_TO_DEV,
                SgDir::FromDevice => SG_DXFER_FROM_DEV,
            },
            cmd_len: u8::try_from(cdb.len()).map_err(|_| too_big())?,
            mx_sb_len: 64,
            iovec_count: 0,
            dxfer_len: u32::try_from(data.len()).map_err(|_| too_big())?,
            dxferp: data.as_mut_ptr().cast(),
            cmdp: cdb.as_ptr(),
            sbp: sense.as_mut_ptr(),
            timeout: timeout_ms,
            flags: 0,
            pack_id: 0,
            usr_ptr: std::ptr::null_mut(),
            status: 0,
            masked_status: 0,
            msg_status: 0,
            sb_len_wr: 0,
            host_status: 0,
            driver_status: 0,
            resid: 0,
            duration: 0,
            info: 0,
        };
        // SAFETY: SG_IO reads `hdr` and the command block, transfers at
        // most `dxfer_len` bytes to or from `data`, writes at most
        // `mx_sb_len` bytes of sense into `sense`, and writes the status
        // fields of `hdr`. All of them outlive the call, which is
        // synchronous; the descriptor is open.
        check(unsafe { libc::ioctl(self.file.as_raw_fd(), request(SG_IO), &raw mut hdr) })?;
        let n = usize::from(hdr.sb_len_wr).min(sense.len());
        Ok(SgReply {
            status: hdr.status,
            host_status: hdr.host_status,
            driver_status: hdr.driver_status,
            sense: sense[..n].to_vec(),
            resid: hdr.resid,
        })
    }
}
