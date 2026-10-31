# The local console

The console is the display and keyboards on the machine itself: the SRC
display during the IPL, the IPL menu, DST and the installer, the
sign-on display and, once the IPL completes, a 5250 terminal of the
system's own telnet server. All of them are one 24×80 cell grid drawn by
one renderer (`lic/console`); `svc.screen` returns that grid as text and
does not depend on how it is drawn.

## Display mode

QS/400 does no modesetting. The firmware (UEFI GOP) picks the mode
before the kernel starts; `simpledrm` takes the GOP framebuffer over and
offers it as `/dev/fb0` through DRM's fbdev emulation. The console reads
the mode with `FBIOGET_VSCREENINFO` and `FBIOGET_FSCREENINFO`: `xres`,
`yres`, `bits_per_pixel`, the red, green and blue offsets and lengths,
and `line_length`. Native-mode modesetting (full DRM drivers for the
GPU, so a panel runs at its native mode when the firmware chose a lower
one) is deferred; on most firmware the GOP mode is already the panel's.

16, 24 and 32 bits per pixel are drawn, with each pixel packed from the
channel layout the mode reports (XRGB8888, XBGR8888, RGB565, BGR888, ...),
never a hard-coded one. A mode outside that (8 bpp palettes, channels
over eight bits) raises A6005008 and the IPL goes on over serial.

## The scaler

The 9×16 font on a 1920×1080 panel draws the 720×384 grid as a small
box in the middle, so each glyph pixel becomes an s×s block,
nearest neighbour, with

    s  = max(1, min(xres / (cols × 9), yres / (rows × 16)))
    x0 = (xres − cols × 9 × s) / 2
    y0 = (yres − rows × 16 × s) / 2

for the display size in use (24×80; the scaler takes 27×132 too, for
when the console's terminal offers that size). Some values:

| Mode | 24×80 scale | Grid | Origin |
|---|---|---|---|
| 1024×768 | 1 | 720×384 | (152, 192) |
| 1280×800 | 1 | 720×384 | (280, 208) |
| 1440×900 | 2 | 1440×768 | (0, 66) |
| 1920×1080 | 2 | 1440×768 | (240, 156) |
| 2560×1440 | 3 | 2160×1152 | (200, 144) |
| 3840×2160 | 5 | 3600×1920 | (120, 120) |

`qs400.console.scale=N` on the kernel command line asks for scale N (a
development aid; it is clamped to what fits), `qs400.console.scale=auto`
is the default. The mode and the scale chosen go to the IPL log on
serial (`slic: display: 1920x1080 at 32 bpp (r16/8 g8/8 b0/8), stride
7680, 80x24 grid at scale 2 from (240, 156), ...`) and to `svc.hw`
(`Display: ...`, asked of the console thread, which reads the mode again
first).

### Drawing

`console::scale::Canvas` holds three things: the grid drawn at 1:1 into
an off-screen bitmap (720×384 pixels for 24×80), the whole frame in the
framebuffer's own format (`yres × line_length` bytes, up to about 8 MB
at 1080p and 32 MB at 4K), and the cells and cursor it drew last. A
draw compares the new grid with the last one row by row; only the rows
whose cells changed, or that the cursor left or entered, are drawn into
the bitmap again and scaled into the frame. The draw returns the byte
span from the first dirty row's top scanline to the last one's bottom,
and the renderer writes that span in one `pwrite`, then `fsync`. A
keystroke in a field rewrites one cell row (32 scanlines at scale 2,
about 245 KB at 1080p) instead of the whole frame.

The span always covers whole scanlines and more than one: DRM's fbdev
emulation turns a write into a damage rectangle, full width when it
spans several lines, but narrowed to the written columns when it stays
in one line, and empty when such a write ends exactly at the line's end
(`drm_fb_helper_memory_range_to_clip`; the Phase 1 lesson). `mmap` is
not used: under `simpledrm`'s deferred I/O it is never flushed.

The first draw, and the first after a change of mode or grid size,
writes the whole frame, margins included. The mode is asked for again
whenever a write fails (EIO from a framebuffer that went away or
changed) and when the console leaves a TN5250 session; a changed mode
lays the grid out again and repaints. There is no splash screen to
resume from.

### Tests

`cargo test -p console` covers the scale for each common mode and for
27×132, the clamped override, the centering, the packing of each depth
and channel order, and the dirty-row spans (a cell, a cursor move
across rows and along one, a resize).

The QEMU suite's steps 71 and 72 (`tools/conformance/src/display.rs`)
boot the disk on a `virtio-vga` device with `xres`/`yres` of 1920×1080
and 1024×768 (OVMF takes the device's size as its GOP mode; the baked
command line has no `video=`). At sign-on they check `svc.hw`'s
`Display:` line for the mode and the expected scale and origin, take a
QMP screendump (PPM) and compare it with the scaler's rendering of the
`screen` text at the same size, cell by cell: every glyph pixel an s×s
block of one color, lit where the golden is lit (the inverse for
reverse image; the underline and cursor scanlines may be lit besides),
every pixel outside the grid dark. Both images are written beside the
suite's output as `display-WxH.png` and `display-WxH-golden.png`. First
run (2026-10-30, KVM): scale 2 at 1920×1080 and 1 at 1024×768, 1880
cells exact and 40 underlined (the input fields) of 1920 each.
