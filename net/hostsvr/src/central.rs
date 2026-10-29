// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The central server (`0xE000`, port 8470): NLS exchange attributes
//! (the job CCSID and the CCSIDs the server converts), conversion table
//! downloads, and license management. Every template starts with a u16
//! chain field the clients leave zero.
//!
//! The primary oracle's central server drops every connection, so the
//! reply layouts here are from IBM's documentation of the datastream as
//! `JTOpen` reads it, not from traces; license request templates are kept
//! as bytes.

use crate::cp::central as cp;
use crate::frame::Param;
use crate::template::{template, templates};

/// Request and reply IDs (replies reuse the request ID).
pub mod id {
    crate::consts! {
        /// Request a license.
        REQUEST_LICENSE = 0x1001,
        /// Release a license.
        RELEASE_LICENSE = 0x1002,
        /// Retrieve license information.
        RETRIEVE_LICENSE_INFO = 0x1003,
        /// Retrieve a conversion table.
        CONVERSION_TABLE = 0x1201,
        /// NLS exchange attributes.
        EXCHANGE_ATTRIBUTES = 0x1301,
    }
}

/// Conversion table mapping types.
pub mod mapping {
    /// Round trip.
    pub const ROUND_TRIP: u16 = 1;
    /// Substitution.
    pub const SUBSTITUTION: u16 = 2;
}

template! {
    /// NLS exchange attributes request (`0x1301`).
    pub struct ExchangeAttributes {
        /// Chain (0).
        chain: u16,
        /// Datastream level.
        level: u16,
        /// Client CCSID (13488).
        ccsid: u32,
        /// Client version.
        client_version: u32,
        /// License management level.
        license_level: u16,
        /// System management level.
        system_level: u16,
        /// NLS level.
        nls_level: u16,
    }
}

template! {
    /// NLS exchange attributes reply (`0x1301`); a CCSID list (`0x0008`)
    /// may follow.
    pub struct ExchangeAttributesReply {
        /// Chain.
        chain: u16,
        /// Primary return code.
        rc: u16,
        /// Secondary return code.
        secondary_rc: u16,
        /// Datastream level.
        level: u16,
        /// The server job's CCSID.
        ccsid: u32,
        /// Server version.
        server_version: u32,
        /// License management level.
        license_level: u16,
        /// System management level.
        system_level: u16,
        /// NLS level.
        nls_level: u16,
    }
}

template! {
    /// Conversion table request (`0x1201`); the code points to map
    /// follow in `0x0004`.
    pub struct ConversionTable {
        /// Chain.
        chain: u16,
        /// From CCSID.
        from_ccsid: u32,
        /// To CCSID.
        to_ccsid: u32,
        /// Mapping type.
        mapping: u16,
        /// Parameter count.
        parameter_count: u16,
    }
}

template! {
    /// Conversion table reply (`0x1201`); the table follows as one LL/CP.
    pub struct ConversionTableReply {
        /// Chain.
        chain: u16,
        /// Primary return code.
        rc: u16,
        /// Secondary return code.
        secondary_rc: u16,
        /// Reserved.
        reserved: u16,
    }
}

templates! {
    /// The central server's templates.
    pub enum Template {
        /// `0x1301` request.
        ExchangeAttributes(ExchangeAttributes) = Request id::EXCHANGE_ATTRIBUTES,
        /// `0x1301` reply.
        ExchangeAttributesReply(ExchangeAttributesReply) = Reply id::EXCHANGE_ATTRIBUTES,
        /// `0x1201` request.
        ConversionTable(ConversionTable) = Request id::CONVERSION_TABLE,
        /// `0x1201` reply.
        ConversionTableReply(ConversionTableReply) = Reply id::CONVERSION_TABLE,
    }
}

/// The CCSID list of an exchange attributes reply.
pub fn ccsid_list(params: &[Param]) -> Option<Vec<u32>> {
    let p = params.iter().find(|p| p.cp == cp::CCSID_LIST)?;
    let n = p
        .data
        .get(..4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))? as usize;
    let body = p.data.get(4..4 + 4 * n)?;
    Some(
        body.chunks(4)
            .map(|c| u32::from_be_bytes([c[0], c[1], c[2], c[3]]))
            .collect(),
    )
}

/// The code points parameter of a conversion table request: every byte
/// value of a single-byte CCSID, or every u16 of a double-byte one.
pub fn code_points(single_byte: bool) -> Param {
    let data: Vec<u8> = if single_byte {
        (0..=255u8).collect()
    } else {
        (0..=0xFFFFu16).flat_map(u16::to_be_bytes).collect()
    };
    Param::new(cp::CODE_POINTS, data)
}
