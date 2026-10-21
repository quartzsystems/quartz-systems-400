// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The ioctls: `FBIOGET_VSCREENINFO`, `FBIOGET_FSCREENINFO`, `KDSETMODE`.
//! The only `unsafe` in the console crate.

#![allow(unsafe_code)]

use std::fs::{File, OpenOptions};
use std::io;

use rustix::ioctl::{Getter, IntegerSetter, ioctl};

/// `struct fb_bitfield`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Bitfield {
    /// Bit offset.
    pub offset: u32,
    /// Bit length.
    pub length: u32,
    /// Most significant bit is right.
    pub msb_right: u32,
}

/// `struct fb_var_screeninfo`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct VarInfo {
    /// Visible resolution.
    pub xres: u32,
    /// Visible resolution.
    pub yres: u32,
    /// Virtual resolution.
    pub xres_virtual: u32,
    /// Virtual resolution.
    pub yres_virtual: u32,
    /// Offset from virtual to visible.
    pub xoffset: u32,
    /// Offset from virtual to visible.
    pub yoffset: u32,
    /// Bits per pixel.
    pub bits_per_pixel: u32,
    /// Grayscale.
    pub grayscale: u32,
    /// Red channel.
    pub red: Bitfield,
    /// Green channel.
    pub green: Bitfield,
    /// Blue channel.
    pub blue: Bitfield,
    /// Alpha channel.
    pub transp: Bitfield,
    nonstd: u32,
    activate: u32,
    height: u32,
    width: u32,
    accel_flags: u32,
    pixclock: u32,
    left_margin: u32,
    right_margin: u32,
    upper_margin: u32,
    lower_margin: u32,
    hsync_len: u32,
    vsync_len: u32,
    sync: u32,
    vmode: u32,
    rotate: u32,
    colorspace: u32,
    reserved: [u32; 4],
}

/// `struct fb_fix_screeninfo`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FixInfo {
    id: [u8; 16],
    smem_start: usize,
    /// Length of the frame buffer memory.
    pub smem_len: u32,
    kind: u32,
    type_aux: u32,
    visual: u32,
    xpanstep: u16,
    ypanstep: u16,
    ywrapstep: u16,
    /// Bytes per scanline.
    pub line_length: u32,
    mmio_start: usize,
    mmio_len: u32,
    accel: u32,
    capabilities: u16,
    reserved: [u16; 2],
}

const FBIOGET_VSCREENINFO: u32 = 0x4600;
const FBIOGET_FSCREENINFO: u32 = 0x4602;
const KDSETMODE: u32 = 0x4B3A;
const KD_GRAPHICS: usize = 1;

/// Variable and fixed screen info of an open framebuffer.
pub fn screen_info(file: &File) -> io::Result<(VarInfo, FixInfo)> {
    // SAFETY: FBIOGET_VSCREENINFO fills exactly one fb_var_screeninfo and
    // FBIOGET_FSCREENINFO exactly one fb_fix_screeninfo; the structs above
    // match the kernel layouts field for field.
    let var = unsafe { ioctl(file, Getter::<FBIOGET_VSCREENINFO, VarInfo>::new()) }?;
    // SAFETY: as above.
    let fix = unsafe { ioctl(file, Getter::<FBIOGET_FSCREENINFO, FixInfo>::new()) }?;
    Ok((var, fix))
}

/// Put a VT into graphics mode so the kernel console stops drawing. The
/// returned handle keeps the device open; the mode is left as is.
pub fn vt_graphics(path: &str) -> io::Result<File> {
    let tty = OpenOptions::new().read(true).write(true).open(path)?;
    // SAFETY: KDSETMODE takes the mode as an integer argument, not a pointer.
    unsafe { ioctl(&tty, IntegerSetter::<KDSETMODE>::new_usize(KD_GRAPHICS)) }?;
    Ok(tty)
}
