// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! PPM (P6) to PNG, for QEMUs without PNG screendump support. The image
//! is stored uncompressed (deflate "stored" blocks), which every reader
//! accepts.

use std::fs;
use std::path::Path;

use crc::{CRC_32_ISO_HDLC, Crc};

const CRC32: Crc<u32> = Crc::<u32>::new(&CRC_32_ISO_HDLC);

/// Convert a binary PPM file to PNG.
pub fn ppm_to_png(ppm: &Path, png: &Path) -> Result<(), String> {
    let data = fs::read(ppm).map_err(|e| format!("read {}: {e}", ppm.display()))?;
    let (w, h, pixels) = parse_ppm(&data)?;
    fs::write(png, encode_png(w, h, pixels)).map_err(|e| format!("write {}: {e}", png.display()))
}

/// An RGB image as a PNG file.
pub(crate) fn write_png(path: &Path, image: &crate::display::Image) -> Result<(), String> {
    let (w, h) = (
        u32::try_from(image.w).map_err(|_| "width")?,
        u32::try_from(image.h).map_err(|_| "height")?,
    );
    fs::write(path, encode_png(w, h, &image.rgb))
        .map_err(|e| format!("write {}: {e}", path.display()))
}

/// Header `P6 <w> <h> 255` then raw RGB.
pub(crate) fn parse_ppm(data: &[u8]) -> Result<(u32, u32, &[u8]), String> {
    let mut fields = Vec::new();
    let mut pos = 0;
    while fields.len() < 4 {
        while pos < data.len() && data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        let start = pos;
        while pos < data.len() && !data[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if start == pos {
            return Err("truncated PPM header".into());
        }
        fields.push(String::from_utf8_lossy(&data[start..pos]).into_owned());
    }
    pos += 1; // single whitespace after maxval
    if fields[0] != "P6" || fields[3] != "255" {
        return Err("not an 8-bit P6 PPM".into());
    }
    let w: u32 = fields[1].parse().map_err(|_| "bad width")?;
    let h: u32 = fields[2].parse().map_err(|_| "bad height")?;
    let need = (w as usize) * (h as usize) * 3;
    data.get(pos..pos + need)
        .map(|px| (w, h, px))
        .ok_or_else(|| "PPM pixel data truncated".to_string())
}

fn encode_png(w: u32, h: u32, rgb: &[u8]) -> Vec<u8> {
    let stride = w as usize * 3;
    let mut raw = Vec::with_capacity((stride + 1) * h as usize);
    for row in rgb.chunks(stride) {
        raw.push(0); // filter: none
        raw.extend_from_slice(row);
    }
    let mut out = Vec::with_capacity(raw.len() + 1024);
    out.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit RGB
    chunk(&mut out, *b"IHDR", &ihdr);
    chunk(&mut out, *b"IDAT", &zlib_stored(&raw));
    chunk(&mut out, *b"IEND", &[]);
    out
}

/// A zlib stream of stored deflate blocks.
fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let mut z = vec![0x78, 0x01];
    let blocks: Vec<&[u8]> = if data.is_empty() {
        vec![&[][..]]
    } else {
        data.chunks(65_535).collect()
    };
    let last = blocks.len() - 1;
    for (i, b) in blocks.iter().enumerate() {
        z.push(u8::from(i == last));
        let len = u16::try_from(b.len()).expect("block under 64 KiB");
        z.extend_from_slice(&len.to_le_bytes());
        z.extend_from_slice(&(!len).to_le_bytes());
        z.extend_from_slice(b);
    }
    z.extend_from_slice(&adler32(data).to_be_bytes());
    z
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: [u8; 4], data: &[u8]) {
    out.extend_from_slice(
        &u32::try_from(data.len())
            .expect("chunk under 4 GiB")
            .to_be_bytes(),
    );
    let mut body = Vec::with_capacity(4 + data.len());
    body.extend_from_slice(&kind);
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&CRC32.checksum(&body).to_be_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_a_tiny_image() {
        let ppm = b"P6\n2 1\n255\n\xff\x00\x00\x00\xff\x00";
        let (w, h, px) = parse_ppm(ppm).unwrap();
        assert_eq!((w, h), (2, 1));
        let png = encode_png(w, h, px);
        assert!(png.starts_with(b"\x89PNG"));
        assert_eq!(&png[12..16], b"IHDR");
        assert_eq!(adler32(b"abc"), 0x024d_0127);
    }
}
