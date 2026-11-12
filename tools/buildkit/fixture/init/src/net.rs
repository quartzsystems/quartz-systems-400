// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A route of QSDEMO's own: added through `netlink` under QSDEMO's
//! routing protocol number, and read back from the kernel with it.

use std::net::{IpAddr, Ipv4Addr};

use netlink::Family;
use netlink::route::{Route, Rtnl};

/// QSDEMO's routing protocol number: unassigned in iproute2, and not
/// QS/400's 81.
pub const RTPROT_QSDEMO: u8 = 83;

/// Add the route and find it again. One line for the console.
pub fn prove() -> Result<String, String> {
    let err = |what: &str, e: netlink::Error| format!("{what}: {e}");
    let mut rt = Rtnl::open().map_err(|e| err("rtnetlink", e))?;
    let lo = rt.link_by_name("lo").map_err(|e| err("lo", e))?;
    rt.set_up(lo.index, true).map_err(|e| err("lo up", e))?;
    let dst = IpAddr::V4(Ipv4Addr::new(198, 51, 100, 0));
    let mut route = Route::new(Family::V4, Some(dst), 24, RTPROT_QSDEMO);
    route.oif = Some(lo.index);
    rt.add_route(&route).map_err(|e| err("add route", e))?;
    let found = rt
        .routes(Some(Family::V4))
        .map_err(|e| err("routes", e))?
        .into_iter()
        .find(|r| r.dst == Some(dst) && r.dst_len == 24)
        .ok_or("the kernel does not list the route")?;
    if found.protocol != RTPROT_QSDEMO {
        return Err(format!(
            "the route carries protocol {}, not {RTPROT_QSDEMO}",
            found.protocol
        ));
    }
    Ok(format!(
        "198.51.100.0/24 on lo added with routing protocol {}",
        found.protocol
    ))
}
