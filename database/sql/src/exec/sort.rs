// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The external merge sort behind `ORDER BY`: rows gather in memory up
//! to the job's row budget, each full batch is sorted and written to
//! the spill area as a run, and the runs are merged on the way out. A
//! sort that fits the budget never touches the spill area.

use std::cmp::Ordering;
use std::rc::Rc;

use super::eval::{Env, Row, Schema};
use super::query::Produced;
use super::spill::{RunId, SpillArea};
use crate::error::{Result, SqlError};
use crate::value::{Timestamp, Value};
use db::record::datetime::{Date, Time};
use qdecimal::Decimal;

/// How one `ORDER BY` item orders.
#[derive(Clone, Copy, Debug)]
pub(crate) struct KeySpec {
    pub(crate) desc: bool,
    pub(crate) nulls_first: bool,
}

/// Compare two keys item by item.
pub(crate) fn cmp_keys(left: &[Value], right: &[Value], specs: &[KeySpec]) -> Result<Ordering> {
    for (i, s) in specs.iter().enumerate() {
        let (x, y) = (&left[i], &right[i]);
        let o = match (x.is_null(), y.is_null()) {
            (true, true) => Ordering::Equal,
            (true, false) => {
                if s.nulls_first {
                    Ordering::Less
                } else {
                    Ordering::Greater
                }
            }
            (false, true) => {
                if s.nulls_first {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            }
            (false, false) => match x.compare(y)? {
                Some(o) if s.desc => o.reverse(),
                Some(o) => o,
                None => Ordering::Equal,
            },
        };
        if o != Ordering::Equal {
            return Ok(o);
        }
    }
    Ok(Ordering::Equal)
}

fn sort_batch(batch: &mut [(Vec<Value>, Produced)], specs: &[KeySpec]) -> Result<()> {
    let mut failure = None;
    batch.sort_by(|a, b| match cmp_keys(&a.0, &b.0, specs) {
        Ok(o) => o,
        Err(e) => {
            failure = Some(e);
            Ordering::Equal
        }
    });
    failure.map_or(Ok(()), Err)
}

/// The frame schemas a row's environment carries; rows of one sort
/// share a few such lists, encoded as an index into them.
type Shape = Vec<Rc<Schema>>;

fn shape_of(p: &Produced) -> Shape {
    p.source.frames.iter().map(|(s, _)| s.clone()).collect()
}

fn same_shape(a: &Shape, b: &Shape) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| Rc::ptr_eq(x, y))
}

/// Rows sorted by their keys, spilled past the budget.
pub(crate) struct Sorter {
    specs: Vec<KeySpec>,
    budget: usize,
    pending: Vec<(Vec<Value>, Produced)>,
    runs: Vec<RunId>,
    shapes: Vec<Shape>,
}

impl Sorter {
    pub(crate) fn new(specs: Vec<KeySpec>, budget: usize) -> Self {
        Self {
            specs,
            budget: budget.max(1),
            pending: Vec::new(),
            runs: Vec::new(),
            shapes: Vec::new(),
        }
    }

    /// One more row.
    pub(crate) fn push(
        &mut self,
        spill: &mut dyn SpillArea,
        key: Vec<Value>,
        p: Produced,
    ) -> Result<()> {
        self.pending.push((key, p));
        if self.pending.len() >= self.budget {
            self.flush(spill)?;
        }
        Ok(())
    }

    fn shape_index(&mut self, p: &Produced) -> u16 {
        let shape = shape_of(p);
        if let Some(i) = self.shapes.iter().position(|s| same_shape(s, &shape)) {
            return u16::try_from(i).expect("few shapes");
        }
        self.shapes.push(shape);
        u16::try_from(self.shapes.len() - 1).expect("few shapes")
    }

    /// Sort what is pending and write it out as a run.
    fn flush(&mut self, spill: &mut dyn SpillArea) -> Result<()> {
        if self.pending.is_empty() {
            return Ok(());
        }
        let mut batch = std::mem::take(&mut self.pending);
        sort_batch(&mut batch, &self.specs)?;
        let mut bytes = Vec::new();
        for (key, p) in &batch {
            let shape = self.shape_index(p);
            let mut entry = Vec::new();
            encode_row(key, &mut entry);
            encode_produced(p, shape, &mut entry);
            bytes.extend_from_slice(&u32::try_from(entry.len()).expect("fits").to_le_bytes());
            bytes.extend_from_slice(&entry);
        }
        self.runs.push(spill.write_run(&bytes)?);
        Ok(())
    }

    /// The rows in order. How many runs were merged is told too.
    pub(crate) fn finish(mut self, spill: &mut dyn SpillArea) -> Result<(Vec<Produced>, usize)> {
        if self.runs.is_empty() {
            let mut batch = std::mem::take(&mut self.pending);
            sort_batch(&mut batch, &self.specs)?;
            return Ok((batch.into_iter().map(|(_, p)| p).collect(), 0));
        }
        self.flush(spill)?;
        let mut readers: Vec<RunReader> = self.runs.iter().map(|&r| RunReader::new(r)).collect();
        let mut heads: Vec<Option<(Vec<Value>, Produced)>> = Vec::with_capacity(readers.len());
        for r in &mut readers {
            heads.push(r.next_entry(spill, &self.shapes)?);
        }
        let mut out = Vec::new();
        loop {
            let mut best: Option<usize> = None;
            for (i, h) in heads.iter().enumerate() {
                let Some((k, _)) = h else { continue };
                match best {
                    None => best = Some(i),
                    Some(b) => {
                        let (bk, _) = heads[b].as_ref().expect("set");
                        if cmp_keys(k, bk, &self.specs)? == Ordering::Less {
                            best = Some(i);
                        }
                    }
                }
            }
            let Some(b) = best else { break };
            let (_, p) = heads[b].take().expect("set");
            out.push(p);
            heads[b] = readers[b].next_entry(spill, &self.shapes)?;
        }
        let runs = self.runs.len();
        spill.clear();
        Ok((out, runs))
    }
}

/// Reads one run's entries in order, a chunk at a time.
struct RunReader {
    run: RunId,
    off: u64,
    buf: Vec<u8>,
    pos: usize,
    at_end: bool,
}

const CHUNK: usize = 64 * 1024;

impl RunReader {
    fn new(run: RunId) -> Self {
        Self {
            run,
            off: 0,
            buf: Vec::new(),
            pos: 0,
            at_end: false,
        }
    }

    /// Make sure `n` unread bytes are buffered, if the run has them.
    fn ensure(&mut self, spill: &mut dyn SpillArea, n: usize) -> Result<bool> {
        while self.buf.len() - self.pos < n {
            if self.at_end {
                return Ok(false);
            }
            let more = spill.read(self.run, self.off, CHUNK.max(n))?;
            if more.is_empty() {
                self.at_end = true;
                return Ok(false);
            }
            self.off += u64::try_from(more.len()).expect("fits");
            self.buf.drain(..self.pos);
            self.pos = 0;
            self.buf.extend_from_slice(&more);
        }
        Ok(true)
    }

    fn next_entry(
        &mut self,
        spill: &mut dyn SpillArea,
        shapes: &[Shape],
    ) -> Result<Option<(Vec<Value>, Produced)>> {
        if !self.ensure(spill, 4)? {
            return Ok(None);
        }
        let len = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().expect("4"));
        self.pos += 4;
        let len = usize::try_from(len).expect("fits");
        if !self.ensure(spill, len)? {
            return Err(SqlError::Database {
                what: "sort run cut short".into(),
                msgid: None,
            });
        }
        let entry = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        let mut cur = Cursor { b: entry, at: 0 };
        let key = decode_row(&mut cur)?;
        let p = decode_produced(&mut cur, shapes)?;
        Ok(Some((key, p)))
    }
}

// The run encoding: little-endian lengths, values tagged by type.

fn put_u32(n: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(&u32::try_from(n).expect("fits").to_le_bytes());
}

fn put_bytes(b: &[u8], out: &mut Vec<u8>) {
    put_u32(b.len(), out);
    out.extend_from_slice(b);
}

fn encode_value(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Null => out.push(0),
        Value::Num(d) => {
            out.push(1);
            put_bytes(d.to_string().as_bytes(), out);
        }
        Value::Str(s) => {
            out.push(2);
            put_bytes(s.as_bytes(), out);
        }
        Value::Date(d) => {
            out.push(3);
            encode_date(*d, out);
        }
        Value::Time(t) => {
            out.push(4);
            encode_time(*t, out);
        }
        Value::Timestamp(ts) => {
            out.push(5);
            encode_date(ts.date, out);
            encode_time(ts.time, out);
            out.extend_from_slice(&ts.micros.to_le_bytes());
        }
    }
}

fn encode_date(d: Date, out: &mut Vec<u8>) {
    out.extend_from_slice(&d.year.to_le_bytes());
    out.push(d.month);
    out.push(d.day);
}

fn encode_time(t: Time, out: &mut Vec<u8>) {
    out.extend([t.hour, t.minute, t.second]);
}

fn encode_row(r: &[Value], out: &mut Vec<u8>) {
    put_u32(r.len(), out);
    for v in r {
        encode_value(v, out);
    }
}

fn encode_produced(p: &Produced, shape: u16, out: &mut Vec<u8>) {
    encode_row(&p.out, out);
    out.extend_from_slice(&shape.to_le_bytes());
    for (_, r) in &p.source.frames {
        encode_row(r, out);
    }
    match &p.source.group {
        None => out.push(0),
        Some(g) => {
            out.push(1);
            put_u32(g.len(), out);
            for r in g.iter() {
                encode_row(r, out);
            }
        }
    }
}

struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.at + n;
        if end > self.b.len() {
            return Err(SqlError::Database {
                what: "sort run entry cut short".into(),
                msgid: None,
            });
        }
        let s = &self.b[self.at..end];
        self.at = end;
        Ok(s)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().expect("2")))
    }

    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().expect("4")))
    }

    fn bytes(&mut self) -> Result<Vec<u8>> {
        let n = usize::try_from(self.u32()?).expect("fits");
        Ok(self.take(n)?.to_vec())
    }
}

fn bad(what: &str) -> SqlError {
    SqlError::Database {
        what: format!("sort run holds {what}"),
        msgid: None,
    }
}

fn decode_date(c: &mut Cursor<'_>) -> Result<Date> {
    let year = c.u16()?;
    let month = c.u8()?;
    let day = c.u8()?;
    Date::new(year, month, day).ok_or_else(|| bad("a bad date"))
}

fn decode_time(c: &mut Cursor<'_>) -> Result<Time> {
    let h = c.u8()?;
    let m = c.u8()?;
    let s = c.u8()?;
    Time::new(h, m, s).ok_or_else(|| bad("a bad time"))
}

fn decode_value(c: &mut Cursor<'_>) -> Result<Value> {
    Ok(match c.u8()? {
        0 => Value::Null,
        1 => {
            let text = String::from_utf8(c.bytes()?).map_err(|_| bad("a bad number"))?;
            Value::Num(Decimal::parse(&text).map_err(|_| bad("a bad number"))?)
        }
        2 => Value::Str(String::from_utf8(c.bytes()?).map_err(|_| bad("bad text"))?),
        3 => Value::Date(decode_date(c)?),
        4 => Value::Time(decode_time(c)?),
        5 => {
            let date = decode_date(c)?;
            let time = decode_time(c)?;
            let micros = c.u32()?;
            Value::Timestamp(Timestamp { date, time, micros })
        }
        _ => return Err(bad("an unknown value tag")),
    })
}

fn decode_row(c: &mut Cursor<'_>) -> Result<Row> {
    let n = usize::try_from(c.u32()?).expect("fits");
    let mut r = Vec::with_capacity(n);
    for _ in 0..n {
        r.push(decode_value(c)?);
    }
    Ok(r)
}

fn decode_produced(c: &mut Cursor<'_>, shapes: &[Shape]) -> Result<Produced> {
    let out = decode_row(c)?;
    let shape = shapes
        .get(usize::from(c.u16()?))
        .ok_or_else(|| bad("an unknown row shape"))?;
    let mut frames = Vec::with_capacity(shape.len());
    for s in shape {
        frames.push((s.clone(), decode_row(c)?));
    }
    let group = if c.u8()? == 0 {
        None
    } else {
        let n = usize::try_from(c.u32()?).expect("fits");
        let mut rows = Vec::with_capacity(n);
        for _ in 0..n {
            rows.push(decode_row(c)?);
        }
        Some(Rc::new(rows))
    };
    Ok(Produced {
        out,
        source: Env { frames, group },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::spill::MemSpill;

    fn row(n: i64, s: &str) -> Produced {
        let out = vec![Value::int(n), Value::Str(s.into())];
        Produced {
            source: Env::default().with(Rc::new(Schema::default()), out.clone()),
            out,
        }
    }

    #[test]
    fn spills_and_merges() {
        let specs = vec![KeySpec {
            desc: false,
            nulls_first: false,
        }];
        let mut spill = MemSpill::default();
        let mut s = Sorter::new(specs, 3);
        for n in [9, 2, 7, 4, 1, 8, 3, 6, 5, 0] {
            s.push(&mut spill, vec![Value::int(n)], row(n, "x"))
                .unwrap();
        }
        let (rows, runs) = s.finish(&mut spill).unwrap();
        assert_eq!(runs, 4);
        let got: Vec<i64> = rows.iter().map(|p| p.out[0].as_i64().unwrap()).collect();
        assert_eq!(got, (0..10).collect::<Vec<_>>());
        assert_eq!(rows[3].source.frames[0].1[1], Value::Str("x".into()));
    }

    #[test]
    fn values_round_trip() {
        let vals = vec![
            Value::Null,
            Value::Num(Decimal::parse("-12.500").unwrap()),
            Value::Str("héllo".into()),
            Value::Date(Date::new(2026, 10, 22).unwrap()),
            Value::Time(Time::new(23, 59, 1).unwrap()),
            Value::Timestamp(Timestamp::parse("2026-10-22 12:00:00.000123").unwrap()),
        ];
        let mut b = Vec::new();
        encode_row(&vals, &mut b);
        let mut c = Cursor { b: &b, at: 0 };
        assert_eq!(decode_row(&mut c).unwrap(), vals);
    }
}
