// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! A service's result built by column name: every column starts null,
//! a service sets what it has.

use std::collections::HashMap;

use db::record::datetime::{Date, Time};
use qdecimal::Decimal;

use super::super::super::eval::{Row, Schema};
use super::super::Table;
use super::shape::Shape;
use crate::value::{Timestamp, Value};

/// The result being built.
pub struct Out {
    shape: &'static Shape,
    schema: Schema,
    index: HashMap<&'static str, usize>,
    rows: Vec<Row>,
    unknown: Vec<String>,
}

impl Out {
    /// An empty result of `shape`'s columns, correlation `alias`; the
    /// `NOT NULL` columns in `no_value` described nullable.
    #[must_use]
    pub fn new(shape: &'static Shape, alias: &str, no_value: &'static [&'static str]) -> Self {
        let cols = shape.cols.iter().map(|c| c.info(alias, c.nullable || no_value.contains(&c.name.as_str()))).collect();
        let index = shape.cols.iter().enumerate().map(|(i, c)| (c.name.as_str(), i)).collect();
        Self { shape, schema: Schema { cols }, index, rows: Vec::new(), unknown: Vec::new() }
    }

    /// Whether the shape has `col`.
    #[must_use]
    pub fn has(&self, col: &str) -> bool {
        self.index.contains_key(col)
    }

    /// Start a row, every column null.
    pub fn row(&mut self) {
        self.rows.push(vec![Value::Null; self.shape.cols.len()]);
    }

    /// Rows so far.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// No rows yet.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Set `col` of the current row. A `CHAR` value is padded to its
    /// length, a `VARCHAR` one cut to it. A column the shape does not
    /// have (a `_BASIC` view's) is left out, and noted for the tests.
    pub fn set(&mut self, col: &str, v: Value) {
        let Some(&i) = self.index.get(col) else {
            if !self.unknown.iter().any(|u| u == col) {
                self.unknown.push(col.to_string());
            }
            return;
        };
        let c = &self.shape.cols[i];
        let v = match v {
            Value::Str(s) if matches!(c.ty.as_str(), "CHAR" | "GRAPHIC") => {
                let len = c.len as usize;
                let mut s: String = s.chars().take(len).collect();
                while s.chars().count() < len {
                    s.push(' ');
                }
                Value::Str(s)
            }
            Value::Str(s) if matches!(c.ty.as_str(), "VARCHAR" | "VARG") && s.chars().count() > c.len as usize => {
                Value::Str(s.chars().take(c.len as usize).collect())
            }
            other => other,
        };
        if let Some(r) = self.rows.last_mut() {
            r[i] = v;
        }
    }

    /// Set `col` to text.
    pub fn text(&mut self, col: &str, s: impl Into<String>) {
        self.set(col, Value::Str(s.into()));
    }

    /// Set `col` to text, null when it is empty.
    pub fn text_or_null(&mut self, col: &str, s: &str) {
        let t = s.trim();
        if t.is_empty() {
            self.set(col, Value::Null);
        } else {
            self.text(col, t);
        }
    }

    /// Set `col` to an integer.
    pub fn int(&mut self, col: &str, n: impl TryInto<i64>) {
        self.set(col, n.try_into().map_or(Value::Null, Value::int));
    }

    /// Set `col` to `YES` or `NO`.
    pub fn yes(&mut self, col: &str, b: bool) {
        self.text(col, if b { "YES" } else { "NO" });
    }

    /// Set `col` to a decimal given as text (`12.50`).
    pub fn dec(&mut self, col: &str, s: &str) {
        self.set(col, Decimal::parse(s).map_or(Value::Null, Value::Num));
    }

    /// Set `col` to a float as a decimal of `scale` places.
    pub fn float(&mut self, col: &str, x: f64, scale: usize) {
        self.dec(col, &format!("{x:.scale$}"));
    }

    /// Set `col` to the timestamp of Unix seconds; null for 0.
    pub fn stamp(&mut self, col: &str, secs: u64) {
        self.stamp_micros(col, secs, 0);
    }

    /// Set `col` to the timestamp of Unix seconds and microseconds;
    /// null for 0.
    pub fn stamp_micros(&mut self, col: &str, secs: u64, micros: u32) {
        if secs == 0 {
            self.set(col, Value::Null);
            return;
        }
        let ts = Timestamp::new(Date::from_unix(secs), Time::from_unix(secs), u64::from(micros) * 1_000_000);
        self.set(col, Value::Timestamp(ts));
    }

    /// The result. A `NOT NULL` column left null is a column the
    /// service forgot: the tests find it from the description.
    #[must_use]
    pub fn finish(self) -> Table {
        (self.schema, self.rows)
    }

    /// The result and the columns set that the shape does not have.
    #[must_use]
    pub fn finish_noting(self) -> (Table, Vec<String>) {
        ((self.schema, self.rows), self.unknown)
    }
}
