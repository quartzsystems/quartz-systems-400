// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Linux framebuffer: query the mode, keep the virtual terminal out
//! of the way, write spans of whole scanlines. 16, 24 and 32 bits per
//! pixel, in whatever channel layout the mode reports.

mod raw;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;

use thiserror::Error;

use crate::scale::{Channel, Mode, PixelFormat};

/// Why the framebuffer could not be used.
#[derive(Debug, Error)]
pub enum FbError {
    /// The console mode leaves the display alone.
    #[error("framebuffer: not wanted in this console mode")]
    NotWanted,
    /// `/dev/fb0` could not be opened or queried.
    #[error("framebuffer: {0}")]
    Io(#[from] io::Error),
    /// A pixel format the scaler cannot pack.
    #[error("framebuffer: unsupported format ({bpp} bpp, r{r} g{g} b{b})")]
    Unsupported {
        /// Bits per pixel.
        bpp: u32,
        /// Red offset.
        r: u32,
        /// Green offset.
        g: u32,
        /// Blue offset.
        b: u32,
    },
}

/// An open framebuffer.
pub struct Framebuffer {
    file: File,
    /// The mode as last queried.
    pub mode: Mode,
    /// The kernel console was detached from the framebuffer.
    pub fbcon_unbound: bool,
    _vt: Option<File>,
}

impl Framebuffer {
    /// Open `/dev/fb0`, read its mode, detach the kernel console from it
    /// (fbcon repaints whenever it likes) and put the VT in graphics mode
    /// so no cursor is drawn either.
    pub fn open() -> Result<Self, FbError> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/fb0")?;
        let mode = query(&file)?;
        let fbcon_unbound = unbind_fbcon();
        let vt = raw::vt_graphics("/dev/tty0").ok();
        Ok(Self {
            file,
            mode,
            fbcon_unbound,
            _vt: vt,
        })
    }

    /// Ask for the mode again (`FBIOGET_VSCREENINFO`). True when it
    /// changed.
    pub fn requery(&mut self) -> Result<bool, FbError> {
        let mode = query(&self.file)?;
        let changed = mode != self.mode;
        self.mode = mode;
        Ok(changed)
    }

    /// Write `bytes` at `offset` of the frame in one `pwrite`, then
    /// `fsync` so the kernel's deferred I/O flushes it.
    ///
    /// The span must cover whole scanlines, more than one: the fbdev
    /// damage tracking turns a write that stays within a single scanline
    /// and ends exactly at its end into an empty clip (seen on a
    /// 1024-wide display with a 4096-byte stride, where half of every
    /// row went missing). A write spanning several lines damages the
    /// full width.
    pub fn write_span(&self, bytes: &[u8], offset: usize) -> io::Result<()> {
        self.file.write_all_at(bytes, offset as u64)?;
        self.file.sync_all()
    }
}

/// The mode of an open framebuffer.
fn query(file: &File) -> Result<Mode, FbError> {
    let (var, fix) = raw::screen_info(file)?;
    let ch = |b: raw::Bitfield| Channel {
        offset: b.offset,
        length: b.length,
    };
    let format = PixelFormat {
        bpp: var.bits_per_pixel,
        red: ch(var.red),
        green: ch(var.green),
        blue: ch(var.blue),
    };
    if !format.supported() {
        return Err(FbError::Unsupported {
            bpp: format.bpp,
            r: format.red.offset,
            g: format.green.offset,
            b: format.blue.offset,
        });
    }
    let xres = var.xres as usize;
    Ok(Mode {
        xres,
        yres: var.yres as usize,
        stride: (fix.line_length as usize).max(xres * format.bytes()),
        format,
    })
}

/// Detach the frame buffer console from the display via
/// `/sys/class/vtconsole/vtcon*/bind`. Returns whether it was done.
fn unbind_fbcon() -> bool {
    let Ok(dir) = fs::read_dir("/sys/class/vtconsole") else {
        return false;
    };
    let mut done = false;
    for entry in dir.filter_map(Result::ok) {
        let path = entry.path();
        let name = fs::read_to_string(path.join("name")).unwrap_or_default();
        if name.contains("frame buffer") && fs::write(path.join("bind"), "0").is_ok() {
            done = true;
        }
    }
    done
}
