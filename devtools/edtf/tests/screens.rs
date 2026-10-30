// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The edit and browse displays against the pub400 goldens of EDTF and
//! DSPF (the probe file `qsnet.txt`: three lines with CR LF, a tab,
//! CCSID 819), and the displays pub400 was not asked for against our
//! own goldens under `tests/golden/` (`QS400_UPDATE_GOLDEN=1` rewrites
//! them). Typing goes through a 5250 terminal, as at a real display.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use edtf::screen::{draw, read};
use edtf::session::{Action, Input, Key, Mode, Session};
use edtf::text::Text;
use tn5250::ds::wdsf::{Capabilities, Model};
use tn5250::record::Record;
use tn5250::screen::{Read, ScreenModel};
use tn5250::term::Terminal;

const PROBE: &[u8] = b"Quartz IFS probe\r\nline two\r\n\tline three after a tab\r\n";
const MASKS: &[&str] = &["USERNAME"];

fn session(mode: Mode) -> Session {
    Session::new(
        mode,
        "/home/USERNAME/qsnet.txt",
        Text::decode(PROBE, 819).unwrap(),
        819,
        37,
        PROBE.len(),
    )
}

fn phase10(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/conformance/golden/phase10/ifs")
        .join(format!("{name}.txt"))
}

fn ours(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"))
}

fn rows_of(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .replace("\r\n", "\n")
        .lines()
        .map(str::to_string)
        .collect()
}

/// Our row with the golden's masks copied over the same columns.
fn masked(golden: &str, ours: &str) -> String {
    let mut out: Vec<char> = ours.chars().collect();
    let g: Vec<char> = golden.chars().collect();
    for m in MASKS {
        let mc: Vec<char> = m.chars().collect();
        let mut i = 0;
        while i + mc.len() <= g.len() {
            if g[i..i + mc.len()] == mc[..] {
                if out.len() < i + mc.len() {
                    out.resize(i + mc.len(), ' ');
                }
                out[i..i + mc.len()].copy_from_slice(&mc);
                i += mc.len();
            } else {
                i += 1;
            }
        }
    }
    out.into_iter().collect()
}

fn compare(golden: &Path, got: &[String]) {
    let want = rows_of(golden);
    let mut diff = String::new();
    for (i, w) in want.iter().enumerate() {
        let o = masked(w, got.get(i).map_or("", String::as_str));
        if w.trim_end() != o.trim_end() {
            let _ = writeln!(diff, "row {}:\n  want |{}|\n  got  |{}|", i + 1, w.trim_end(), o.trim_end());
        }
    }
    assert!(diff.is_empty(), "{} differs:\n{diff}", golden.display());
}

/// Our golden: written when asked, else compared.
fn check_ours(name: &str, got: &[String]) {
    let p = ours(name);
    if std::env::var_os("QS400_UPDATE_GOLDEN").is_some() || !p.exists() {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let mut t = got.iter().map(|r| r.trim_end()).collect::<Vec<_>>().join("\n");
        t.push('\n');
        std::fs::write(&p, t).unwrap();
        return;
    }
    compare(&p, got);
}

fn shown(s: &Session) -> Vec<String> {
    let mut m = ScreenModel::new(false);
    draw(s, &mut m);
    m.text_rows()
}

/// The display drawn, sent to a terminal, typed into there, read back.
struct Display {
    m: ScreenModel,
    t: Terminal,
}

impl Display {
    fn new() -> Self {
        Self {
            m: ScreenModel::new(false),
            t: Terminal::new(Capabilities {
                model: Model::M3179,
                gui: false,
                color: true,
            }),
        }
    }

    fn show(&mut self, s: &Session) {
        draw(s, &mut self.m);
        let out = self.m.render(Read::MdtFields);
        self.t.apply(&Record::new(out.opcode, out.data)).unwrap();
    }

    /// Type `text` at a position (1-based) and press `key`.
    fn exchange(&mut self, s: &mut Session, typed: &[(usize, usize, &str)], key: Key) -> Action {
        for (r, c, text) in typed {
            self.t.set_cursor(*r, *c);
            self.t.type_str(text);
        }
        let k = match key {
            Key::Enter => tn5250::term::Key::Enter,
            Key::F(n) => tn5250::term::Key::F(n),
            Key::PageDown => tn5250::term::Key::PageDown,
            Key::PageUp => tn5250::term::Key::PageUp,
        };
        self.t.key(k);
        let rec = self.t.take_outbox().remove(0);
        self.m.accept(&rec.data).unwrap();
        let input: Input = read(s, &self.m);
        s.key(key, &input)
    }

    fn rows(&self) -> Vec<String> {
        self.t.text_rows()
    }
}

#[test]
fn edtf_matches_pub400() {
    compare(&phase10("edtf"), &shown(&session(Mode::Edit)));
}

#[test]
fn dspf_matches_pub400() {
    compare(&phase10("dspf"), &shown(&session(Mode::Browse)));
    // WRKLNK option 5 on a stream file is the same browse display.
    compare(&phase10("wrklnk-5-stmf"), &shown(&session(Mode::Browse)));
}

#[test]
fn line_commands_at_the_display() {
    let mut s = session(Mode::Edit);
    let mut d = Display::new();
    d.show(&s);
    // Insert two lines after the first, delete the third.
    let a = d.exchange(&mut s, &[(7, 1, "i2"), (9, 1, "d")], Key::Enter);
    assert_eq!(a, Action::Stay);
    d.show(&s);
    check_ours("edtf-insert", &d.rows());
    // Type into the first new line; the unused one goes.
    d.exchange(&mut s, &[(8, 5, "typed here")], Key::Enter);
    d.show(&s);
    check_ours("edtf-typed", &d.rows());
    assert_eq!(s.buf.texts(), vec!["Quartz IFS probe", "typed here", "line two"]);
    let bytes = s.bytes().unwrap();
    assert_eq!(bytes, b"Quartz IFS probe\r\ntyped here\r\nline two\r\n");
}

#[test]
fn copy_waits_for_its_target() {
    let mut s = session(Mode::Edit);
    let mut d = Display::new();
    d.show(&s);
    d.exchange(&mut s, &[(7, 1, "c")], Key::Enter);
    d.show(&s);
    check_ours("edtf-copy-pending", &d.rows());
    d.exchange(&mut s, &[(9, 1, "a")], Key::Enter);
    assert_eq!(s.buf.len(), 4);
    assert_eq!(s.buf.text(3), "Quartz IFS probe");
}

#[test]
fn find_and_unknown_commands() {
    let mut s = session(Mode::Edit);
    let mut d = Display::new();
    d.show(&s);
    d.exchange(&mut s, &[(3, 12, "find two")], Key::Enter);
    d.show(&s);
    check_ours("edtf-find", &d.rows());
    d.exchange(&mut s, &[(7, 1, "zz")], Key::Enter);
    d.show(&s);
    check_ours("edtf-bad-line-command", &d.rows());
}

#[test]
fn browse_hex_and_shift() {
    let mut s = session(Mode::Browse);
    let mut d = Display::new();
    d.show(&s);
    d.exchange(&mut s, &[], Key::F(10));
    d.show(&s);
    check_ours("dspf-hex", &d.rows());
    d.exchange(&mut s, &[], Key::F(10));
    d.show(&s);
    d.exchange(&mut s, &[(2, 59, "   8")], Key::Enter);
    d.show(&s);
    check_ours("dspf-column-8", &d.rows());
    assert_eq!(s.left, 8);
}

#[test]
fn services_display() {
    let mut s = session(Mode::Edit);
    let mut m = ScreenModel::new(false);
    edtf::services::draw(&s, &mut m, None);
    check_ours("edtf-services", &m.text_rows());
    let c = edtf::services::Choices {
        ccsid: "1208".into(),
        eol: "*LF".into(),
        shift: "40".into(),
        scroll: "8".into(),
    };
    edtf::services::apply(&mut s, &c).unwrap();
    assert_eq!(s.new_ccsid, Some(1208));
    assert_eq!(s.col_by, 40);
    assert_eq!(s.bytes().unwrap(), b"Quartz IFS probe\nline two\n\tline three after a tab\n");
    s.saved();
    assert_eq!(s.file_ccsid, 1208);
    assert_eq!(s.message.as_ref().unwrap().id, "CPIB612");
    let bad = edtf::services::Choices {
        eol: "*XX".into(),
        ..edtf::services::Choices::default()
    };
    assert_eq!(edtf::services::apply(&mut s, &bad).unwrap_err().id, "CPDB624");
}

/// Every message the editor shows is in the system message file with
/// the same text.
#[test]
fn messages_are_in_qcpfmsg() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/msgf/msgs/qcpfmsg.toml"),
    )
    .unwrap();
    for (id, text) in edtf::msg::TEXTS {
        let at = src
            .find(&format!("id = \"{id}\""))
            .unwrap_or_else(|| panic!("{id} is not in qcpfmsg.toml"));
        let rest = &src[at..];
        let line = rest.lines().find(|l| l.starts_with("text = ")).unwrap();
        assert_eq!(line, format!("text = {text:?}"), "{id}");
    }
}
