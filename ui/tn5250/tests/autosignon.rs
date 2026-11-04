// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The client's RFC 4777 variables as the server reads them.

use tn5250::telnet::client::{Client, Config, Encoding};
use tn5250::telnet::server::Server;

#[test]
fn the_server_gets_the_user_seed_and_substitute() {
    let mut s = Server::new([9, 8, 7, 6, 5, 4, 3, 2]);
    let mut c = Client::new(Config {
        user: Some("autousr".into()),
        password: Some("Auto2026".into()),
        encoding: Encoding::Sha,
        ..Config::default()
    });
    let mut to_client = s.opening();
    for _ in 0..20 {
        let (to_server, _) = c.feed(&to_client);
        let (back, _) = s.feed(&to_server);
        to_client = back;
        if s.done {
            break;
        }
    }
    assert!(s.done, "negotiation: {:?} / {:?}", c.trace, s.trace);
    let n = &s.negotiated;
    assert_eq!(n.var("USER").as_deref(), Some("AUTOUSR"), "{:?}", n.vars);
    assert_eq!(n.var_bytes("IBMRSEED").map(<[u8]>::len), Some(8), "{:?}", n.vars);
    let want = pwdsub_sha("AUTOUSR", "Auto2026", [9, 8, 7, 6, 5, 4, 3, 2], Config::default().client_seed);
    assert_eq!(n.var_bytes("IBMSUBSPW"), Some(&want[..]));
}

fn pwdsub_sha(user: &str, pw: &str, server: [u8; 8], client: [u8; 8]) -> [u8; 20] {
    tn5250::telnet::signon::sha_password(user, pw, &server, &client)
}
