// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! What the database pages read and write besides the pages: SQL
//! scripts split into statements ([`script`], the desktop's rules), the
//! SQL the table pages run ([`sqltext`]), the transfer and download
//! files (CSV, TSV, fixed width, XLSX, ODS, HTML, PDF and the
//! Excel-compatible XML; [`files`]) and the desktop's `.qdx` requests
//! ([`qdx`]). Values move as text: decimals are never floating point.

#![allow(clippy::many_single_char_names)] // rows, cells, bytes

pub mod files;
pub mod qdx;
pub mod script;
pub mod sqltext;
mod sheets;
mod text;
mod xml;
