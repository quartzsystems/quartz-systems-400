// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The kernel's ublk interface (`include/uapi/linux/ublk_cmd.h`) and the
//! parts of `io_uring`'s entries it uses, as byte layouts: what the server
//! writes into a submission and reads from a descriptor.

/// `_IOC(dir, 'u', nr, size)`.
const fn ioc(dir: u32, nr: u32, size: u32) -> u32 {
    (dir << 30) | (size << 16) | ((b'u' as u32) << 8) | nr
}
const RW: u32 = 3;

const CTRL: u32 = 32;
const IO: u32 = 16;
/// Bytes of `struct ublksrv_ctrl_cmd`.
pub const CTRL_CMD_LEN: usize = CTRL as usize;
/// Bytes of `struct ublksrv_io_cmd`.
pub const IO_CMD_LEN: usize = IO as usize;
/// Bytes of `struct ublksrv_ctrl_dev_info`.
pub const DEV_INFO_LEN: usize = 64;
/// The same, as a command's `len`.
pub const DEV_INFO_LEN16: u16 = 64;
/// [`PARAMS_BASIC_LEN`] as a command's `len`.
pub const PARAMS_BASIC_LEN16: u16 = 40;
/// Bytes of `struct ublksrv_io_desc`.
pub const IO_DESC_LEN: usize = 24;
/// Bytes of `struct ublk_params` up to and including its basic part.
pub const PARAMS_BASIC_LEN: usize = 40;

/// `UBLK_U_CMD_ADD_DEV`.
pub const CMD_ADD_DEV: u32 = ioc(RW, 0x04, CTRL);
/// `UBLK_U_CMD_DEL_DEV`.
pub const CMD_DEL_DEV: u32 = ioc(RW, 0x05, CTRL);
/// `UBLK_U_CMD_START_DEV`.
pub const CMD_START_DEV: u32 = ioc(RW, 0x06, CTRL);
/// `UBLK_U_CMD_STOP_DEV`.
pub const CMD_STOP_DEV: u32 = ioc(RW, 0x07, CTRL);
/// `UBLK_U_CMD_SET_PARAMS`.
pub const CMD_SET_PARAMS: u32 = ioc(RW, 0x08, CTRL);
/// `UBLK_U_CMD_UPDATE_SIZE`.
pub const CMD_UPDATE_SIZE: u32 = ioc(RW, 0x15, CTRL);
/// `UBLK_U_IO_FETCH_REQ`.
pub const IO_FETCH_REQ: u32 = ioc(RW, 0x20, IO);
/// `UBLK_U_IO_COMMIT_AND_FETCH_REQ`.
pub const IO_COMMIT_AND_FETCH_REQ: u32 = ioc(RW, 0x21, IO);

/// `UBLK_U_CMD_START_USER_RECOVERY`.
pub const CMD_START_USER_RECOVERY: u32 = ioc(RW, 0x10, CTRL);
/// `UBLK_U_CMD_END_USER_RECOVERY`.
pub const CMD_END_USER_RECOVERY: u32 = ioc(RW, 0x11, CTRL);

/// `UBLK_F_USER_RECOVERY`: the device stays, quiesced, when its server
/// ends.
pub const F_USER_RECOVERY: u64 = 1 << 3;
/// `UBLK_F_USER_RECOVERY_REISSUE`: requests the old server had are
/// issued again to the new one.
pub const F_USER_RECOVERY_REISSUE: u64 = 1 << 4;
/// `UBLK_F_CMD_IOCTL_ENCODE`.
pub const F_CMD_IOCTL_ENCODE: u64 = 1 << 6;
/// `UBLK_F_UPDATE_SIZE`.
pub const F_UPDATE_SIZE: u64 = 1 << 10;

/// `UBLK_ATTR_VOLATILE_CACHE`.
pub const ATTR_VOLATILE_CACHE: u32 = 1 << 2;
/// `UBLK_ATTR_FUA`.
pub const ATTR_FUA: u32 = 1 << 3;
/// `UBLK_PARAM_TYPE_BASIC`.
pub const PARAM_TYPE_BASIC: u32 = 1;

/// `UBLK_IO_OP_READ`.
pub const OP_READ: u8 = 0;
/// `UBLK_IO_OP_WRITE`.
pub const OP_WRITE: u8 = 1;
/// `UBLK_IO_OP_FLUSH`.
pub const OP_FLUSH: u8 = 2;
/// `UBLK_IO_F_FUA`, in the descriptor's flags (bits 8 and up of `op_flags`).
pub const IO_F_FUA: u32 = 1 << 13;

/// The most tags one queue has (`UBLK_MAX_QUEUE_DEPTH`).
pub const MAX_QUEUE_DEPTH: usize = 4096;

/// `IORING_OP_URING_CMD`.
pub const IORING_OP_URING_CMD: u8 = 46;
/// Bytes of a 128-byte submission entry (`IORING_SETUP_SQE128`).
pub const SQE_LEN: usize = 128;
/// Bytes of a completion entry.
pub const CQE_LEN: usize = 16;
/// Where a uring command's own bytes start in a submission entry.
pub const SQE_CMD: usize = 48;

/// A uring command submission: `fd`, the command number, a tag of the
/// caller's (`user_data`) and the command's bytes.
#[must_use]
pub fn uring_cmd(fd: i32, cmd_op: u32, user_data: u64, cmd: &[u8]) -> [u8; SQE_LEN] {
    assert!(cmd.len() <= SQE_LEN - SQE_CMD, "command too long");
    let mut sqe = [0u8; SQE_LEN];
    sqe[0] = IORING_OP_URING_CMD;
    sqe[4..8].copy_from_slice(&fd.to_le_bytes());
    sqe[8..12].copy_from_slice(&cmd_op.to_le_bytes());
    sqe[32..40].copy_from_slice(&user_data.to_le_bytes());
    sqe[SQE_CMD..SQE_CMD + cmd.len()].copy_from_slice(cmd);
    sqe
}

/// `IORING_OP_READ`.
pub const IORING_OP_READ: u8 = 22;

/// A read of `len` bytes from `fd` into `addr`, tagged `user_data`.
#[must_use]
pub fn read_sqe(fd: i32, addr: u64, len: u32, user_data: u64) -> [u8; SQE_LEN] {
    let mut sqe = [0u8; SQE_LEN];
    sqe[0] = IORING_OP_READ;
    sqe[4..8].copy_from_slice(&fd.to_le_bytes());
    sqe[16..24].copy_from_slice(&addr.to_le_bytes());
    sqe[24..28].copy_from_slice(&len.to_le_bytes());
    sqe[32..40].copy_from_slice(&user_data.to_le_bytes());
    sqe
}

/// A completion: its tag and result.
#[must_use]
pub fn cqe(bytes: &[u8; CQE_LEN]) -> (u64, i32) {
    let user_data = u64::from_le_bytes(bytes[0..8].try_into().expect("8"));
    let res = i32::from_le_bytes(bytes[8..12].try_into().expect("4"));
    (user_data, res)
}

/// `struct ublksrv_ctrl_cmd`: the device, the queue (all ones for none),
/// a buffer the command reads or fills, and one word of inline data.
#[must_use]
pub fn ctrl_cmd(dev_id: u32, addr: u64, len: u16, data: u64) -> [u8; CTRL_CMD_LEN] {
    let mut b = [0u8; CTRL_CMD_LEN];
    b[0..4].copy_from_slice(&dev_id.to_le_bytes());
    b[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
    b[6..8].copy_from_slice(&len.to_le_bytes());
    b[8..16].copy_from_slice(&addr.to_le_bytes());
    b[16..24].copy_from_slice(&data.to_le_bytes());
    b
}

/// `struct ublksrv_io_cmd`: queue, tag, the last request's result and
/// the tag's buffer.
#[must_use]
pub fn io_cmd(q_id: u16, tag: u16, result: i32, addr: u64) -> [u8; IO_CMD_LEN] {
    let mut b = [0u8; IO_CMD_LEN];
    b[0..2].copy_from_slice(&q_id.to_le_bytes());
    b[2..4].copy_from_slice(&tag.to_le_bytes());
    b[4..8].copy_from_slice(&result.to_le_bytes());
    b[8..16].copy_from_slice(&addr.to_le_bytes());
    b
}

/// What a new device asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DevInfo {
    /// Hardware queues.
    pub queues: u16,
    /// Tags per queue.
    pub depth: u16,
    /// Largest request, bytes.
    pub max_io: u32,
    /// The device number; `u32::MAX` lets the kernel pick.
    pub dev_id: u32,
    /// The server's process.
    pub pid: i32,
    /// `UBLK_F_*`.
    pub flags: u64,
}

impl DevInfo {
    /// `struct ublksrv_ctrl_dev_info`.
    #[must_use]
    pub fn encode(&self) -> [u8; DEV_INFO_LEN] {
        let mut b = [0u8; DEV_INFO_LEN];
        b[0..2].copy_from_slice(&self.queues.to_le_bytes());
        b[2..4].copy_from_slice(&self.depth.to_le_bytes());
        b[8..12].copy_from_slice(&self.max_io.to_le_bytes());
        b[12..16].copy_from_slice(&self.dev_id.to_le_bytes());
        b[16..20].copy_from_slice(&self.pid.to_le_bytes());
        b[24..32].copy_from_slice(&self.flags.to_le_bytes());
        b
    }

    /// The device number the kernel wrote back.
    #[must_use]
    pub fn dev_id_of(bytes: &[u8; DEV_INFO_LEN]) -> u32 {
        u32::from_le_bytes(bytes[12..16].try_into().expect("4"))
    }
}

/// `struct ublk_params` with its basic part only: 4096-byte blocks,
/// `attrs`, the largest request and the size, in sectors.
#[must_use]
pub fn params_basic(attrs: u32, max_sectors: u32, dev_sectors: u64) -> [u8; PARAMS_BASIC_LEN] {
    let mut b = [0u8; PARAMS_BASIC_LEN];
    b[0..4].copy_from_slice(&40u32.to_le_bytes());
    b[4..8].copy_from_slice(&PARAM_TYPE_BASIC.to_le_bytes());
    b[8..12].copy_from_slice(&attrs.to_le_bytes());
    // logical, physical, io_opt and io_min shifts: 4096 bytes.
    b[12..16].copy_from_slice(&[12, 12, 12, 12]);
    b[16..20].copy_from_slice(&max_sectors.to_le_bytes());
    b[24..32].copy_from_slice(&dev_sectors.to_le_bytes());
    b
}

/// One request as its descriptor gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IoDesc {
    /// `UBLK_IO_OP_*`.
    pub op: u8,
    /// `UBLK_IO_F_*`.
    pub flags: u32,
    /// Sectors.
    pub sectors: u32,
    /// First sector.
    pub start: u64,
}

impl IoDesc {
    /// `struct ublksrv_io_desc`.
    #[must_use]
    pub fn decode(b: &[u8; IO_DESC_LEN]) -> Self {
        let op_flags = u32::from_le_bytes(b[0..4].try_into().expect("4"));
        Self {
            op: (op_flags & 0xff) as u8,
            flags: op_flags & !0xff,
            sectors: u32::from_le_bytes(b[4..8].try_into().expect("4")),
            start: u64::from_le_bytes(b[8..16].try_into().expect("8")),
        }
    }
}

/// `XFS_IOC_FSGROWFSDATA`: `_IOW('X', 110, struct xfs_growfs_data)`.
pub const XFS_IOC_FSGROWFSDATA: u32 = (1 << 30) | (16 << 16) | ((b'X' as u32) << 8) | 0x6E;

/// `struct xfs_growfs_data`: the new size in filesystem blocks and the
/// inode space limit, percent.
#[must_use]
pub fn growfs_data(newblocks: u64, imaxpct: u32) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0..8].copy_from_slice(&newblocks.to_le_bytes());
    b[8..12].copy_from_slice(&imaxpct.to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_numbers_match_the_header() {
        // _IOWR('u', 0x04, 32 bytes) and the rest, as the kernel computes them.
        assert_eq!(CMD_ADD_DEV, 0xC020_7504);
        assert_eq!(CMD_DEL_DEV, 0xC020_7505);
        assert_eq!(CMD_START_DEV, 0xC020_7506);
        assert_eq!(CMD_STOP_DEV, 0xC020_7507);
        assert_eq!(CMD_SET_PARAMS, 0xC020_7508);
        assert_eq!(CMD_UPDATE_SIZE, 0xC020_7515);
        assert_eq!(IO_FETCH_REQ, 0xC010_7520);
        assert_eq!(IO_COMMIT_AND_FETCH_REQ, 0xC010_7521);
        assert_eq!(XFS_IOC_FSGROWFSDATA, 0x4010_586E);
    }

    #[test]
    fn layouts_put_fields_where_the_kernel_reads_them() {
        let sqe = uring_cmd(7, CMD_ADD_DEV, 0x55, &ctrl_cmd(u32::MAX, 0x1000, 64, 9));
        assert_eq!(sqe[0], 46);
        assert_eq!(&sqe[4..8], &7i32.to_le_bytes());
        assert_eq!(&sqe[8..12], &CMD_ADD_DEV.to_le_bytes());
        assert_eq!(&sqe[32..40], &0x55u64.to_le_bytes());
        assert_eq!(&sqe[48..52], &u32::MAX.to_le_bytes());
        assert_eq!(&sqe[52..54], &u16::MAX.to_le_bytes());
        assert_eq!(&sqe[54..56], &64u16.to_le_bytes());
        assert_eq!(&sqe[56..64], &0x1000u64.to_le_bytes());
        assert_eq!(&sqe[64..72], &9u64.to_le_bytes());
        let info = DevInfo {
            queues: 1,
            depth: 32,
            max_io: 512 << 10,
            dev_id: u32::MAX,
            pid: 100,
            flags: F_CMD_IOCTL_ENCODE | F_UPDATE_SIZE,
        }
        .encode();
        assert_eq!(DevInfo::dev_id_of(&info), u32::MAX);
        assert_eq!(&info[24..32], &(0x440u64).to_le_bytes());
        let p = params_basic(ATTR_VOLATILE_CACHE | ATTR_FUA, 1024, 8 << 20);
        assert_eq!(&p[0..8], &[40, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(&p[8..12], &12u32.to_le_bytes());
        assert_eq!(&p[24..32], &(8u64 << 20).to_le_bytes());
        let mut d = [0u8; IO_DESC_LEN];
        d[0..4].copy_from_slice(&(u32::from(OP_WRITE) | IO_F_FUA).to_le_bytes());
        d[4..8].copy_from_slice(&16u32.to_le_bytes());
        d[8..16].copy_from_slice(&800u64.to_le_bytes());
        assert_eq!(
            IoDesc::decode(&d),
            IoDesc {
                op: OP_WRITE,
                flags: IO_F_FUA,
                sectors: 16,
                start: 800
            }
        );
        let c = io_cmd(0, 3, -5, 0xABC);
        assert_eq!(&c[2..4], &3u16.to_le_bytes());
        assert_eq!(&c[4..8], &(-5i32).to_le_bytes());
    }
}
