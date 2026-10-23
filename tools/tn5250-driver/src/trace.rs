// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The trace: the negotiation, each record both ways with its header,
//! the commands and orders it carried, and hex when asked.

use tn5250::ds::aid;
use tn5250::ds::inbound::Inbound;
use tn5250::ds::parse::{Command, Order};
use tn5250::ds::wdsf::{Sf, kind};
use tn5250::record::{Record, opcode};

/// Where trace lines go.
pub struct Trace {
    on: bool,
    hex: bool,
}

impl Trace {
    /// A trace that prints when `on`.
    pub fn new(on: bool, hex: bool) -> Self {
        Self { on, hex }
    }

    /// A negotiation line.
    pub fn negotiation(&self, line: &str) {
        if self.on {
            println!("  tn {line}");
        }
    }

    /// A free note.
    pub fn note(&self, line: &str) {
        if self.on {
            println!("  ** {line}");
        }
    }

    fn hexdump(&self, bytes: &[u8]) {
        if !self.hex {
            return;
        }
        for chunk in bytes.chunks(32) {
            let hex: Vec<String> = chunk.iter().map(|b| format!("{b:02x}")).collect();
            println!("       {}", hex.join(" "));
        }
    }

    /// A record from the host.
    pub fn inbound_record(&self, rec: &Record, bytes: &[u8]) {
        if !self.on {
            return;
        }
        println!(
            "  <- record {} bytes, opcode {} ({}), flags {:#06x}",
            bytes.len(),
            rec.opcode,
            opcode::name(rec.opcode),
            rec.flags
        );
        self.hexdump(bytes);
    }

    /// A record to the host.
    pub fn outbound_record(&self, rec: &Record, bytes: &[u8], hidden: &[(u8, u8)]) {
        if !self.on {
            return;
        }
        let what = if rec.opcode == opcode::SAVE_SCREEN {
            "saved screen".to_string()
        } else if rec.flags != 0 {
            format!("flags {:#06x}", rec.flags)
        } else if let Some(i) = Inbound::decode(&rec.data) {
            let fields: Vec<String> = i
                .fields
                .iter()
                .map(|f| {
                    if hidden.contains(&(f.row, f.col)) {
                        format!("({},{})=<hidden>", f.row, f.col)
                    } else {
                        format!("({},{})={:?}", f.row, f.col, ccsid::from_ebcdic(&f.data))
                    }
                })
                .collect();
            format!(
                "aid {} cursor ({},{}) {}",
                aid::name(i.aid),
                i.row,
                i.col,
                fields.join(" ")
            )
        } else {
            format!("{} bytes", rec.data.len())
        };
        println!("  -> record {} bytes: {what}", bytes.len());
        if hidden.is_empty() {
            self.hexdump(bytes);
        }
    }

    /// The commands a record carried.
    pub fn commands(&self, cmds: &[Command]) {
        if !self.on {
            return;
        }
        for c in cmds {
            match c {
                Command::Wtd { cc1, cc2, orders } => {
                    println!("     wtd cc1={cc1:#04x} cc2={cc2:#04x}");
                    self.orders(orders);
                }
                Command::WriteErrorCode(orders) => {
                    println!("     write-error-code");
                    self.orders(orders);
                }
                Command::WriteErrorCodeWindow(a, b, orders) => {
                    println!("     write-error-code-window {a:#04x} {b:#04x}");
                    self.orders(orders);
                }
                Command::Wsf(bytes) => match Sf::decode(bytes) {
                    Some(sf) => println!(
                        "     wsf class {:#04x} type {:#04x} ({}) body {}",
                        sf.class,
                        sf.kind,
                        kind::name(sf.kind),
                        hex(&sf.body)
                    ),
                    None => println!("     wsf (short)"),
                },
                other => println!("     {other:?}"),
            }
        }
    }

    #[allow(clippy::unused_self)] // a trace method
    fn orders(&self, orders: &[Order]) {
        for o in orders {
            match o {
                Order::Data(b) | Order::Td(b) => {
                    let text = ccsid::from_ebcdic(
                        &b.iter()
                            .map(|&x| if x < 0x40 { 0x40 } else { x })
                            .collect::<Vec<_>>(),
                    );
                    let attrs: Vec<String> = b
                        .iter()
                        .filter(|&&x| (0x20..=0x3F).contains(&x))
                        .map(|x| format!("{x:#04x}"))
                        .collect();
                    if attrs.is_empty() {
                        println!("       data {text:?}");
                    } else {
                        println!("       data {text:?} attrs {}", attrs.join(","));
                    }
                }
                Order::Sf {
                    ffw,
                    fcws,
                    attr,
                    len,
                } => println!(
                    "       sf ffw={ffw:#06x} fcws={:?} attr={attr:#04x} len={len}",
                    fcws.iter().map(|w| format!("{w:#06x}")).collect::<Vec<_>>()
                ),
                Order::Soh(b) => println!("       soh {}", hex(b)),
                other => println!("       {other:?}"),
            }
        }
    }
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}
