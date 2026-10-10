// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The Linux framebuffer: query geometry, keep the virtual terminal out of
//! the way, write pixel rows. Only 32 bits per pixel XRGB is supported.

mod raw;

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::FileExt;

use thiserror::Error;

/// Why the framebuffer could not be used.
#[derive(Debug, Error)]
pub enum FbError {
    /// `/dev/fb0` could not be opened or queried.
    #[error("framebuffer: {0}")]
    Io(#[from] io::Error),
    /// Pixel format other than 32 bpp XRGB.
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
    /// Visible width in pixels.
    pub width: usize,
    /// Visible height in pixels.
    pub height: usize,
    /// Bytes per scanline.
    pub stride: usize,
    /// The kernel console was detached from the framebuffer.
    pub fbcon_unbound: bool,
    _vt: Option<File>,
}

impl Framebuffer {
    /// Open `/dev/fb0`, check the format, detach the kernel console from
    /// it (fbcon repaints whenever it likes) and put the VT in graphics
    /// mode so no cursor is drawn either.
    pub fn open() -> Result<Self, FbError> {
        let file = OpenOptions::new().read(true).write(true).open("/dev/fb0")?;
        let (var, fix) = raw::screen_info(&file)?;
        let bpp = var.bits_per_pixel;
        let (r, g, b) = (var.red.offset, var.green.offset, var.blue.offset);
        if bpp != 32 || r != 16 || g != 8 || b != 0 {
            return Err(FbError::Unsupported { bpp, r, g, b });
        }
        let fbcon_unbound = unbind_fbcon();
        let vt = raw::vt_graphics("/dev/tty0").ok();
        Ok(Self {
            file,
            width: var.xres as usize,
            height: var.yres as usize,
            stride: fix.line_length as usize,
            fbcon_unbound,
            _vt: vt,
        })
    }

    /// Write one row of pixels at (x, y), clipped to the screen. Written
    /// in pieces under a page: the fbdev deferred-I/O write path loses
    /// damage for larger single writes. Call `flush` when a frame is done.
    pub fn write_row(&self, x: usize, y: usize, pixels: &[u32]) -> io::Result<()> {
        if y >= self.height || x >= self.width {
            return Ok(());
        }
        let n = pixels.len().min(self.width - x);
        let mut bytes = Vec::with_capacity(n * 4);
        for p in &pixels[..n] {
            bytes.extend_from_slice(&p.to_le_bytes());
        }
        let mut offset = (y * self.stride + x * 4) as u64;
        for chunk in bytes.chunks(2048) {
            self.file.write_all_at(chunk, offset)?;
            offset += chunk.len() as u64;
        }
        Ok(())
    }

    /// Push everything written so far to the display: `fsync` runs the
    /// kernel's deferred I/O immediately.
    pub fn flush(&self) -> io::Result<()> {
        self.file.sync_all()
    }

    /// Fill the whole screen with one color.
    pub fn fill(&self, pixel: u32) -> io::Result<()> {
        let row = vec![pixel; self.width];
        for y in 0..self.height {
            self.write_row(0, y, &row)?;
        }
        self.flush()
    }
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
