// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! `ls [-1ACFLRSadgiklmnopqrstux] [file...]`: list files. One name a
//! line unless standard output is a terminal (`-C`); `-l` is the long
//! format with mode, links, owner, group, size and date. Under
//! `/QSYS.LIB` the system names objects `NAME.TYPE` and reports
//! libraries and files as directories, members as files.

use qsh::sys::{FileType, Stat, UtilCtx};

use crate::common::args::getopt;
use crate::common::io::Out;
use crate::common::time::{Zone, ls_date};
use crate::common::{complain_sys, mode, usage};
use crate::file::join;

#[derive(Clone)]
struct Item {
    /// The name shown.
    name: String,
    /// The path used for status.
    path: String,
    st: Stat,
    /// `lstat` of a link, for `-l` and `-F`.
    link: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    One,
    Columns,
    Rows,
    Stream,
    Long,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SortBy {
    Name,
    Time,
    Size,
}

struct Opts {
    layout: Layout,
    all: bool,
    almost_all: bool,
    dir_self: bool,
    classify: bool,
    slash_dirs: bool,
    inode: bool,
    kilo: bool,
    numeric: bool,
    no_owner: bool,
    no_group: bool,
    hide_control: bool,
    reverse: bool,
    recursive: bool,
    size: bool,
    sort: SortBy,
    follow: bool,
    width: usize,
}

struct Lister<'c, 'a> {
    cx: &'c UtilCtx<'a>,
    o: Opts,
    zone: Zone,
    now: i64,
    out: Out,
    status: i32,
}

fn visible(name: &str, hide: bool) -> String {
    if !hide {
        return name.to_string();
    }
    name.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

impl Lister<'_, '_> {
    /// The status of a name; `operand` names given on the command
    /// line, whose links to directories are followed unless `-d`, `-F`
    /// or `-l`.
    fn item(&mut self, name: &str, path: &str, operand: bool) -> Option<Item> {
        let sys = self.cx.sys;
        let lst = match sys.lstat(path) {
            Ok(st) => st,
            Err(e) => {
                complain_sys(self.cx, "ls", name, e);
                self.status = 1;
                return None;
            }
        };
        let (st, link) = if lst.ftype == FileType::Symlink {
            let target = sys.readlink(path).ok();
            let follow_operand = operand
                && !self.o.dir_self
                && !self.o.classify
                && self.o.layout != Layout::Long;
            if self.o.follow || follow_operand {
                (sys.stat(path).unwrap_or(lst), target)
            } else {
                (lst, target)
            }
        } else {
            (lst, None)
        };
        Some(Item {
            name: name.to_string(),
            path: path.to_string(),
            st,
            link,
        })
    }

    fn sort(&self, items: &mut [Item]) {
        items.sort_by(|a, b| {
            let o = match self.o.sort {
                SortBy::Name => a.name.as_bytes().cmp(b.name.as_bytes()),
                SortBy::Time => b
                    .st
                    .mtime
                    .cmp(&a.st.mtime)
                    .then_with(|| a.name.as_bytes().cmp(b.name.as_bytes())),
                SortBy::Size => b
                    .st
                    .size
                    .cmp(&a.st.size)
                    .then_with(|| a.name.as_bytes().cmp(b.name.as_bytes())),
            };
            if self.o.reverse { o.reverse() } else { o }
        });
    }

    fn suffix(&self, it: &Item) -> &'static str {
        let is_link = it.link.is_some() && !self.o.follow;
        if self.o.classify {
            if is_link && it.st.ftype == FileType::Symlink {
                // The long format shows the target instead.
                return if self.o.layout == Layout::Long { "" } else { "@" };
            }
            match it.st.ftype {
                FileType::Dir => "/",
                FileType::Fifo => "|",
                FileType::Socket => "=",
                FileType::File if it.st.mode & 0o111 != 0 => "*",
                _ => "",
            }
        } else if self.o.slash_dirs && it.st.ftype == FileType::Dir {
            "/"
        } else {
            ""
        }
    }

    fn blocks(&self, st: &Stat) -> u64 {
        if self.o.kilo {
            st.blocks.div_ceil(2)
        } else {
            st.blocks
        }
    }

    /// Qshell's total line: the storage of the entries listed.
    fn total(&mut self, items: &[Item]) {
        let bytes: u64 = items.iter().map(|i| i.st.blocks * 512).sum();
        let line = total_line(bytes);
        self.out.text(&line);
    }

    /// The name with its inode and size prefixes.
    fn short(&self, it: &Item) -> String {
        let mut s = String::new();
        if self.o.inode {
            s.push_str(&format!("{:>8} ", it.st.ino));
        }
        if self.o.size {
            s.push_str(&format!("{:>4} ", self.blocks(&it.st)));
        }
        s.push_str(&visible(&it.name, self.o.hide_control));
        s.push_str(self.suffix(it));
        s
    }

    /// One `-l` line in Qshell's columns: the mode, the link count
    /// right-aligned one past the widest, owner and group in eight
    /// columns each, the size in fourteen, the date and the name.
    fn long_line(&self, it: &Item, nlink_w: usize) -> String {
        let st = &it.st;
        let mut s = String::new();
        if self.o.inode {
            s.push_str(&format!("{:>8} ", st.ino));
        }
        if self.o.size {
            s.push_str(&format!("{:>4} ", self.blocks(st)));
        }
        s.push_str(&mode::string(st));
        s.push_str(&format!(" {:>width$}", st.nlink, width = nlink_w + 1));
        if !self.o.no_owner {
            s.push_str(&format!(" {:<8}", self.owner(st)));
        }
        if !self.o.no_group {
            s.push_str(&format!(" {:<8}", self.group(st)));
        }
        s.push_str(&format!(" {:>14}", st.size));
        s.push(' ');
        s.push_str(&ls_date(st.mtime, self.now, &self.zone));
        s.push(' ');
        s.push_str(&visible(&it.name, self.o.hide_control));
        s.push_str(self.suffix(it));
        if let Some(t) = &it.link
            && it.st.ftype == FileType::Symlink
        {
            s.push_str(" -> ");
            s.push_str(t);
        }
        s
    }

    fn owner(&self, st: &Stat) -> String {
        if self.o.numeric {
            return st.uid.to_string();
        }
        self.cx
            .sys
            .user_name(st.uid)
            .unwrap_or_else(|| st.uid.to_string())
    }

    fn group(&self, st: &Stat) -> String {
        if self.o.numeric {
            return st.gid.to_string();
        }
        self.cx
            .sys
            .group_name(st.gid)
            .unwrap_or_else(|| st.gid.to_string())
    }

    fn print_items(&mut self, items: &[Item], total: bool) {
        match self.o.layout {
            Layout::Long => {
                let nlink_w = items
                    .iter()
                    .map(|it| it.st.nlink.to_string().len())
                    .max()
                    .unwrap_or(1);
                if total {
                    self.total(items);
                }
                for it in items {
                    let l = self.long_line(it, nlink_w);
                    self.out.text(&l);
                    self.out.text("\n");
                }
            }
            Layout::One => {
                if total && self.o.size {
                    self.total(items);
                }
                for it in items {
                    let s = self.short(it);
                    self.out.text(&s);
                    self.out.text("\n");
                }
            }
            Layout::Stream => {
                let names: Vec<String> = items.iter().map(|i| self.short(i)).collect();
                let mut line = String::new();
                for (i, n) in names.iter().enumerate() {
                    let piece = if i + 1 < names.len() {
                        format!("{n}, ")
                    } else {
                        n.clone()
                    };
                    if !line.is_empty() && line.len() + piece.trim_end().len() > self.o.width {
                        line.pop();
                        self.out.text(&line);
                        self.out.text("\n");
                        line.clear();
                    }
                    line.push_str(&piece);
                }
                if !line.is_empty() {
                    self.out.text(&line);
                    self.out.text("\n");
                }
            }
            Layout::Columns | Layout::Rows => {
                if total && self.o.size {
                    self.total(items);
                }
                let names: Vec<String> = items.iter().map(|i| self.short(i)).collect();
                let text = columns(&names, self.o.width, self.o.layout == Layout::Rows);
                self.out.text(&text);
            }
        }
    }

    fn list_dir(&mut self, shown: &str, path: &str, header: bool) {
        let sys = self.cx.sys;
        let names = match sys.read_dir(path) {
            Ok(n) => n,
            Err(e) => {
                complain_sys(self.cx, "ls", shown, e);
                self.status = 1;
                return;
            }
        };
        if header {
            self.out.text(&format!("{shown}:\n"));
        }
        let mut items = Vec::new();
        if self.o.all {
            if let Some(it) = self.item(".", path, false) {
                items.push(it);
            }
            let parent = match path.trim_end_matches('/').rfind('/') {
                Some(0) | None => "/".to_string(),
                Some(i) => path[..i].to_string(),
            };
            if let Some(it) = self.item("..", &parent, false) {
                items.push(it);
            }
        }
        for n in names {
            if n.starts_with('.') && !(self.o.all || self.o.almost_all) {
                continue;
            }
            if let Some(it) = self.item(&n, &join(path, &n), false) {
                items.push(it);
            }
        }
        self.sort(&mut items);
        self.print_items(&items, true);
        if self.o.recursive {
            for it in &items {
                if it.name == "." || it.name == ".." {
                    continue;
                }
                if it.st.ftype == FileType::Dir && (it.link.is_none() || self.o.follow) {
                    let sub = join(shown, &it.name);
                    self.out.text("\n");
                    self.list_dir(&sub, &it.path, true);
                }
            }
        }
    }
}

/// `total: N kilobytes` below a megabyte, else megabytes or gigabytes
/// with three decimals, as Qshell prints it.
pub fn total_line(bytes: u64) -> String {
    const K: u64 = 1024;
    if bytes < K * K {
        return format!("total: {} kilobytes\n", bytes.div_ceil(K));
    }
    let (div, unit) = if bytes < K * K * K {
        (K * K, "megabytes")
    } else {
        (K * K * K, "gigabytes")
    };
    let thousandths = (u128::from(bytes) * 1000).div_ceil(u128::from(div));
    format!("total: {}.{:03} {unit}\n", thousandths / 1000, thousandths % 1000)
}

/// Names in columns across `width`, down each column unless `rows`.
fn columns(names: &[String], width: usize, rows: bool) -> String {
    if names.is_empty() {
        return String::new();
    }
    let longest = names.iter().map(|n| n.chars().count()).max().unwrap_or(0);
    let colw = longest + 2;
    let ncols = (width / colw).max(1).min(names.len());
    let nrows = names.len().div_ceil(ncols);
    let ncols = names.len().div_ceil(nrows);
    let mut out = String::new();
    for r in 0..nrows {
        let mut line = String::new();
        for c in 0..ncols {
            let idx = if rows { r * ncols + c } else { c * nrows + r };
            if idx >= names.len() {
                continue;
            }
            let last_in_row = if rows {
                c + 1 == ncols || idx + 1 >= names.len()
            } else {
                c + 1 == ncols || idx + nrows >= names.len()
            };
            if last_in_row {
                line.push_str(&names[idx]);
            } else {
                line.push_str(&format!("{:<colw$}", names[idx]));
            }
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// Run `ls`.
pub fn run(cx: &mut UtilCtx<'_>) -> i32 {
    let name = "ls";
    let p = match getopt(&cx.argv[1..], "1ACFLRSadfgiklmnopqrstux") {
        Ok(p) => p,
        Err(e) => return usage(cx, name, &e.to_string()),
    };
    let terminal = cx.fd(1).is_some_and(|f| f.is_terminal());
    let mut layout = if terminal { Layout::Columns } else { Layout::One };
    let mut sort = SortBy::Name;
    let mut o = Opts {
        layout,
        all: false,
        almost_all: false,
        dir_self: false,
        classify: false,
        slash_dirs: false,
        inode: false,
        kilo: false,
        numeric: false,
        no_owner: false,
        no_group: false,
        hide_control: terminal,
        reverse: false,
        recursive: false,
        size: false,
        sort,
        follow: false,
        width: cx
            .getenv("COLUMNS")
            .and_then(|c| c.parse().ok())
            .unwrap_or(80),
    };
    for (c, _) in &p.opts {
        match c {
            '1' => layout = Layout::One,
            'C' => layout = Layout::Columns,
            'x' => layout = Layout::Rows,
            'm' => layout = Layout::Stream,
            'l' => layout = Layout::Long,
            'g' => {
                layout = Layout::Long;
                o.no_owner = true;
            }
            'o' => {
                layout = Layout::Long;
                o.no_group = true;
            }
            'n' => {
                layout = Layout::Long;
                o.numeric = true;
            }
            'A' => o.almost_all = true,
            'a' => o.all = true,
            'F' => o.classify = true,
            'p' => o.slash_dirs = true,
            'L' => o.follow = true,
            'R' => o.recursive = true,
            'S' => sort = SortBy::Size,
            't' => sort = SortBy::Time,
            'd' => o.dir_self = true,
            'i' => o.inode = true,
            'k' => o.kilo = true,
            'q' => o.hide_control = true,
            'r' => o.reverse = true,
            's' => o.size = true,
            _ => {}
        }
    }
    o.layout = layout;
    o.sort = sort;
    if o.dir_self {
        o.recursive = false;
    }
    let mut l = Lister {
        cx,
        o,
        zone: Zone::of(cx),
        now: i64::try_from(cx.sys.now().as_secs()).unwrap_or(0),
        out: Out::stdout(cx),
        status: 0,
    };
    let operands = if p.operands.is_empty() {
        vec![".".to_string()]
    } else {
        p.operands.clone()
    };
    let mut files: Vec<Item> = Vec::new();
    let mut dirs: Vec<Item> = Vec::new();
    for op in &operands {
        let path = cx.path(op);
        match l.item(op, &path, true) {
            Some(it) => {
                if it.st.ftype == FileType::Dir && !l.o.dir_self {
                    dirs.push(it);
                } else {
                    files.push(it);
                }
            }
            None => l.status = 1,
        }
    }
    l.sort(&mut files);
    l.sort(&mut dirs);
    if !files.is_empty() {
        l.print_items(&files, false);
    }
    let header = operands.len() > 1 || l.o.recursive || !files.is_empty();
    for (i, d) in dirs.iter().enumerate() {
        if i > 0 || !files.is_empty() {
            l.out.text("\n");
        }
        let shown = d.name.clone();
        let path = d.path.clone();
        l.list_dir(&shown, &path, header);
    }
    let status = l.status;
    l.out.finish(cx, name, status)
}
