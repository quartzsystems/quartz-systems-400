// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The element tree: tokens become nested [`Element`]s by the tag
//! table's rules. Containers close on their end tag, leaves when a tag
//! their parent takes arrives, inlines on their end tag or, with a
//! message, when their text ends. Nesting, unknown tags and attribute
//! names and required attributes are checked here; values are the
//! model builder's.

use std::collections::BTreeSet;

use crate::diag::{Diagnostics, ids};
use crate::lex::{Attr, Text, Token};
use crate::tags::{self, Kind, TagDef};

/// An element with its attributes and content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    /// Upper-case tag name; `ROOT` for the tree's root.
    pub name: String,
    /// The tag's definition; `None` only for the root.
    pub def: Option<&'static TagDef>,
    /// Attributes in source order.
    pub attrs: Vec<Attr>,
    /// The line the tag starts on.
    pub line: usize,
    /// Content in source order.
    pub children: Vec<Node>,
}

/// One piece of content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Node {
    /// Text on one line.
    Text(Text),
    /// A nested element.
    Element(Element),
}

impl Element {
    fn new(name: &str, def: Option<&'static TagDef>, attrs: Vec<Attr>, line: usize) -> Self {
        Self {
            name: name.to_string(),
            def,
            attrs,
            line,
            children: Vec::new(),
        }
    }

    fn kind(&self) -> Kind {
        self.def.map_or(Kind::Container, |d| d.kind)
    }

    fn takes_text(&self) -> bool {
        self.def.is_some_and(|d| d.text)
    }

    fn takes(&self, child: &TagDef) -> bool {
        if child.kind == Kind::Inline {
            return self.takes_text();
        }
        child.parents.contains(&self.name.as_str())
    }

    /// The last value of an attribute.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&Attr> {
        self.attrs.iter().rev().find(|a| a.name == name)
    }

    /// The child elements.
    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    /// The child elements with one name.
    pub fn named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elements().filter(move |e| e.name == name)
    }

    /// Text and inline children, up to the first block child.
    #[must_use]
    pub fn leading(&self) -> &[Node] {
        let end = self
            .children
            .iter()
            .position(|n| match n {
                Node::Text(_) => false,
                Node::Element(e) => e.kind() != Kind::Inline,
            })
            .unwrap_or(self.children.len());
        &self.children[..end]
    }
}

/// Build the tree.
#[must_use]
pub fn build(tokens: Vec<Token>) -> (Element, Diagnostics) {
    let mut b = Builder {
        stack: vec![Element::new("ROOT", None, Vec::new(), 0)],
        diags: Diagnostics::new(),
        last_line: 0,
    };
    for t in tokens {
        match t {
            Token::Tag(tag) => b.tag(&tag.name, tag.attrs, tag.line),
            Token::Text(text) => b.text(text),
            Token::Comment { .. } | Token::Imbed { .. } => {}
        }
    }
    let line = b.last_line;
    b.close_to(0, line, "the end of the source");
    let root = b.stack.pop().expect("root");
    (root, b.diags)
}

struct Builder {
    stack: Vec<Element>,
    diags: Diagnostics,
    last_line: usize,
}

impl Builder {
    fn top(&self) -> &Element {
        self.stack.last().expect("root stays")
    }

    /// Pop one element onto its parent.
    fn pop(&mut self) {
        let e = self.stack.pop().expect("more than the root");
        self.stack
            .last_mut()
            .expect("root stays")
            .children
            .push(Node::Element(e));
    }

    /// Close everything above stack index `keep`, reporting what should
    /// have been closed explicitly.
    fn close_to(&mut self, keep: usize, line: usize, by: &str) {
        while self.stack.len() > keep + 1 {
            let e = self.top();
            match e.kind() {
                Kind::Container => self.diags.push(ids::not_closed(line, &e.name.clone(), by)),
                Kind::Inline => self
                    .diags
                    .push(ids::inline_not_closed(line, &e.name.clone())),
                Kind::Leaf | Kind::Empty => {}
            }
            self.pop();
        }
    }

    fn tag(&mut self, name: &str, attrs: Vec<Attr>, line: usize) {
        self.last_line = line;
        if let Some(def) = tags::lookup(name) {
            self.start(def, attrs, line);
        } else if let Some(def) = tags::end_of(name) {
            self.end(def, line, name);
        } else {
            self.diags.push(ids::unknown_tag(line, name));
        }
    }

    fn end(&mut self, def: &'static TagDef, line: usize, name: &str) {
        let Some(at) = self.stack.iter().rposition(|e| e.name == def.name) else {
            self.diags.push(ids::end_without_start(line, name));
            return;
        };
        let by = format!(":{name}");
        self.close_to(at, line, &by);
        self.pop();
    }

    fn start(&mut self, def: &'static TagDef, attrs: Vec<Attr>, line: usize) {
        let attrs = self.check_attrs(def, attrs, line);
        if def.kind == Kind::Inline {
            if !self.top().takes_text() {
                let parent = self.top().name.clone();
                self.diags
                    .push(ids::not_allowed_here(line, def.name, &parent));
            }
            self.stack
                .push(Element::new(def.name, Some(def), attrs, line));
            return;
        }
        // Leaves and open inlines close when a tag they do not take arrives.
        while self.stack.len() > 1 {
            let top = self.top();
            if top.takes(def) || !matches!(top.kind(), Kind::Leaf | Kind::Inline) {
                break;
            }
            if top.kind() == Kind::Inline {
                self.diags
                    .push(ids::inline_not_closed(line, &top.name.clone()));
            }
            self.pop();
        }
        if def.name == "PNLGRP" {
            let parent = self.stack.first().expect("root");
            if parent.named("PNLGRP").next().is_some() || self.stack.len() > 1 {
                self.diags.push(ids::second_pnlgrp(line));
            }
        } else if !self.top().takes(def) {
            let parent = self.top().name.clone();
            self.diags
                .push(ids::not_allowed_here(line, def.name, &parent));
        }
        let e = Element::new(def.name, Some(def), attrs, line);
        match def.kind {
            Kind::Empty => self
                .stack
                .last_mut()
                .expect("root stays")
                .children
                .push(Node::Element(e)),
            _ => self.stack.push(e),
        }
    }

    /// Drop unknown and repeated attributes, report missing ones.
    fn check_attrs(&mut self, def: &TagDef, attrs: Vec<Attr>, line: usize) -> Vec<Attr> {
        let mut seen = BTreeSet::new();
        let mut kept = Vec::new();
        for a in attrs {
            if !def.attrs.contains(&a.name.as_str()) {
                self.diags
                    .push(ids::unknown_attr(a.line, def.name, &a.name));
                continue;
            }
            if !seen.insert(a.name.clone()) {
                self.diags
                    .push(ids::attr_repeated(a.line, def.name, &a.name));
            }
            kept.push(a);
        }
        for r in def.required {
            if !seen.contains(*r) {
                self.diags.push(ids::attr_missing(line, def.name, r));
            }
        }
        kept
    }

    fn text(&mut self, text: Text) {
        self.last_line = text.line;
        if self.top().takes_text() {
            self.stack
                .last_mut()
                .expect("root stays")
                .children
                .push(Node::Text(text));
        } else if !text.is_blank() {
            let parent = self.top().name.clone();
            self.diags.push(ids::text_not_allowed(text.line, &parent));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lex::lex;
    use crate::source::{NoImbeds, Source};

    fn tree(text: &str) -> (Element, Vec<&'static str>) {
        let (s, _) = Source::read(text, &NoImbeds);
        let (t, _) = lex(&s);
        let (root, d) = build(t);
        (root, d.items().iter().map(|d| d.id).collect())
    }

    #[test]
    fn leaves_close_implicitly_and_inlines_nest() {
        let (root, ids) = tree(
            ":PNLGRP.\n:HELP NAME=h.Title\n:P.one :HP2.two:EHP2. three\n:UL.\n:LI.a\n:LI.b\n:EUL.\n:P.last\n:EHELP.\n:EPNLGRP.",
        );
        assert!(ids.is_empty(), "{ids:?}");
        let help = root
            .named("PNLGRP")
            .next()
            .unwrap()
            .named("HELP")
            .next()
            .unwrap();
        let names: Vec<_> = help.elements().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["P", "UL", "P"]);
        let p = help.named("P").next().unwrap();
        assert_eq!(p.children.len(), 3);
        assert!(matches!(&p.children[1], Node::Element(e) if e.name == "HP2"));
        let ul = help.named("UL").next().unwrap();
        assert_eq!(ul.named("LI").count(), 2);
        assert_eq!(help.leading().len(), 1);
    }

    #[test]
    fn structure_errors() {
        let (_, ids) = tree(
            ":PNLGRP.\n:VAR NAME=v.\n:PANEL NAME=p KEYL=k.\n:KEYI KEY=F3 ACTION=EXIT.\n:P.text :HP2.x\n:EPANEL.\n:ECLASS.\n:NOPE.\n:PANEL NAME=q KEYL=k BOGUS=1 NAME=q.\n:EPNLGRP.",
        );
        assert_eq!(
            ids,
            [
                "UIM0302", "UIM0204", "UIM0204", "UIM0208", "UIM0202", "UIM0201", "UIM0301",
                "UIM0304", "UIM0203"
            ]
        );
    }

    #[test]
    fn text_where_none_is_allowed() {
        let (_, ids) = tree(":PNLGRP.\nstray\n:EPNLGRP.\n");
        assert_eq!(ids, ["UIM0205"]);
    }
}
