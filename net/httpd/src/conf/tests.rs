// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

use super::*;

const SAMPLE: &str = r#"# Instance WEBSTORE
ServerRoot /www/webstore
Listen *:80
Listen 443
LogFormat "%h %l %u %t \"%r\" %>s %b" common
CustomLog logs/access_log \
   common

<Directory /www/webstore/admin>
   Order Deny,Allow
   Deny From all
   AuthName "Web store administration"
</Directory>
<VirtualHost *:443>
   SSLEngine On
</VirtualHost>
"#;

#[test]
fn round_trip_and_queries() {
    let c = parse(SAMPLE).unwrap();
    assert_eq!(c.print(), SAMPLE);
    assert_eq!(c.all("listen").len(), 2);
    assert_eq!(c.get("LogFormat").unwrap().args, vec!["%h %l %u %t \"%r\" %>s %b", "common"]);
    assert_eq!(c.get("CustomLog").unwrap().args, vec!["logs/access_log", "common"]);
    let d = c.section("Directory", "/www/webstore/admin").unwrap();
    assert_eq!(d.value("AuthName"), "Web store administration");
    assert_eq!(d.line, 9);
    assert_eq!(c.section("VirtualHost", "*:443").unwrap().value("SSLEngine"), "On");
}

#[test]
fn edits_keep_the_rest() {
    let mut c = parse(SAMPLE).unwrap();
    c.set("ServerRoot", &["/www/other"]);
    c.add(Directive::new("Listen", &["10.0.10.21:8081"]));
    c.section_mut("Directory", "/www/webstore/admin").set("Require", &["valid-user"]);
    c.section_mut("Directory", "/www/new").set("Options", &["-Indexes"]);
    c.remove_section("VirtualHost", "*:443");
    let text = c.print();
    assert!(text.starts_with("# Instance WEBSTORE\nServerRoot /www/other\nListen *:80\nListen 443\nListen 10.0.10.21:8081\n"));
    assert!(text.contains("   AuthName \"Web store administration\"\n   Require valid-user\n</Directory>\n"));
    assert!(text.ends_with("<Directory /www/new>\n   Options -Indexes\n</Directory>\n"));
    assert!(!text.contains("VirtualHost"));
    assert_eq!(parse(&text).unwrap().print(), text);
    assert_eq!(c.unset("listen"), 3);
}

#[test]
fn errors_name_line_and_column() {
    let e = parse("ServerRoot /www/a\n  Lissten 80\n").unwrap_err();
    assert_eq!((e.line, e.col), (2, 3));
    assert!(e.text.contains("Lissten"), "{e}");
    let e = parse("Listen 99999\n").unwrap_err();
    assert_eq!((e.line, e.col), (1, 1));
    let e = parse("<Directory /a>\nOrder Deny,Allow\n").unwrap_err();
    assert_eq!(e.line, 1);
    assert!(e.text.contains("not closed"));
    let e = parse("<Directory /a>\n</VirtualHost>\n").unwrap_err();
    assert_eq!(e.line, 2);
    let e = parse("Order Deny,Allow\n").unwrap_err();
    assert!(e.text.contains("outside a section"), "{e}");
    let e = parse("AuthName \"open\n").unwrap_err();
    assert_eq!((e.line, e.col), (1, 10));
    assert!(parse("<VirtualHost *:443>\n<Directory /x>\nRequire valid-user\n</Directory>\n</VirtualHost>\n").is_ok());
    assert_eq!(rules::listen_addr("[::1]:8443"), Ok((Some("::1".into()), 8443)));
    assert_eq!(rules::listen_addr("10.0.10.21:8081"), Ok((Some("10.0.10.21".into()), 8081)));
    assert!(rules::listen_addr("host:80").is_err());
}

#[test]
fn log_maintenance() {
    assert!(parse("LogMaint logs/access_log 30 0\nLogMaint logs/error_log 7 0 03:30\n").is_ok());
    assert!(parse("LogMaint logs/a 30 0 3:30\n").unwrap_err().text.contains("HH:MM"));
    assert!(parse("LogMaint logs/a 30 x\n").is_err());
    assert!(rules::is_time("23:59") && !rules::is_time("24:00"));
}
