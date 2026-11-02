// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `System`: `SystemInfo`.

use ipc::wire_struct;

/// An installed product option.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Product {
    /// The product ID (`QS00SS1`).
    pub id: String,
    /// The option (`0000`).
    pub option: String,
    /// Its release.
    pub release: String,
}

wire_struct!(Product { id, option, release });

/// `SystemInfo`'s reply.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// The system's name.
    pub system_name: String,
    /// The release.
    pub release: String,
    /// The serial number.
    pub serial: String,
    /// The model.
    pub model: String,
    /// The last IPL, milliseconds since 1970-01-01 UTC.
    pub ipl_time: i64,
    /// Installed products.
    pub products: Vec<Product>,
}

wire_struct!(SystemInfo {
    system_name,
    release,
    serial,
    model,
    ipl_time,
    products
});
