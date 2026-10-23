// Copyright (C), 2026 Quartz Systems. Some rights reserved. This work is
// licensed under the terms of the MIT license which can be found in the
// root directory of this project.

//! The member being edited: records with their sequence numbers (in
//! hundredths), their dates (`yymmdd`, 0 when none) and their data.
//! Every record has an identifier that outlives moves and renumbering,
//! so a line command typed on a row still finds its record after the
//! rows above it changed.

/// The highest sequence number, `9999.99`.
pub const MAX_SEQ: u32 = 999_999;

/// One source record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    /// Stable identifier within the session.
    pub id: u64,
    /// Sequence number in hundredths.
    pub seq: u32,
    /// Change date `yymmdd`; 0 for none.
    pub date: u32,
    /// The data, without trailing blanks.
    pub data: String,
}

/// Why records could not be placed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BufferError {
    /// No sequence numbers are left: the member must be resequenced.
    #[error("no sequence numbers are left between {0} and {1}")]
    NoRoom(u32, u32),
}

/// The member's records in order.
#[derive(Clone, Debug, Default)]
pub struct Buffer {
    recs: Vec<Record>,
    next_id: u64,
    /// The data length of a record (the source file's record length
    /// less the 12 bytes of sequence and date).
    pub width: usize,
    changed: bool,
}

/// Trailing blanks off.
fn trimmed(s: &str) -> String {
    s.trim_end().to_string()
}

impl Buffer {
    /// A buffer over the member's records: sequence, date, data.
    #[must_use]
    pub fn new(width: usize, records: Vec<(u32, u32, String)>) -> Self {
        let mut b = Self {
            width,
            ..Self::default()
        };
        for (seq, date, data) in records {
            let id = b.new_id();
            b.recs.push(Record {
                id,
                seq,
                date,
                data: trimmed(&data),
            });
        }
        b
    }

    fn new_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Whether a record was added, removed or changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// Mark the buffer as saved.
    pub fn saved(&mut self) {
        self.changed = false;
    }

    /// The records.
    #[must_use]
    pub fn records(&self) -> &[Record] {
        &self.recs
    }

    /// The number of records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.recs.len()
    }

    /// Whether the member is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recs.is_empty()
    }

    /// A record by position.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<&Record> {
        self.recs.get(i)
    }

    /// The position of a record.
    #[must_use]
    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.recs.iter().position(|r| r.id == id)
    }

    /// The position of the first record at or after a sequence number.
    #[must_use]
    pub fn index_at_seq(&self, seq: u32) -> usize {
        self.recs.partition_point(|r| r.seq < seq)
    }

    /// Fit text to the record width.
    fn fit(&self, text: &str) -> String {
        trimmed(&text.chars().take(self.width.max(1)).collect::<String>())
    }

    /// Change a record's data; its date becomes `date` when the data
    /// differs. Whether it changed.
    pub fn set_data(&mut self, i: usize, text: &str, date: u32) -> bool {
        let t = self.fit(text);
        let Some(r) = self.recs.get_mut(i) else {
            return false;
        };
        if r.data == t {
            return false;
        }
        r.data = t;
        r.date = date;
        self.changed = true;
        true
    }

    /// Insert lines after position `after` (`None`: before the first
    /// record), numbered by `increment` between their neighbours; the
    /// records that follow are renumbered when there is no room.
    /// Returns the new identifiers.
    pub fn insert(
        &mut self,
        after: Option<usize>,
        lines: &[String],
        increment: u32,
        date: u32,
    ) -> Result<Vec<u64>, BufferError> {
        if lines.is_empty() {
            return Ok(Vec::new());
        }
        let at = after.map_or(0, |i| i + 1).min(self.recs.len());
        let prev = if at == 0 { 0 } else { self.recs[at - 1].seq };
        let next = self.recs.get(at).map_or(MAX_SEQ + 1, |r| r.seq);
        let n = u32::try_from(lines.len()).unwrap_or(u32::MAX);
        let inc = increment.max(1);
        // The increment when it fits, else whatever step fits, else 1
        // with the records after pushed along.
        let room = next.saturating_sub(prev);
        let step = if prev.saturating_add(inc.saturating_mul(n)) < next {
            inc
        } else if room > n {
            (room - 1) / n
        } else {
            1
        };
        let last = prev + step * n;
        if last > MAX_SEQ {
            return Err(BufferError::NoRoom(prev, next));
        }
        // Push the records after along when they collide.
        let mut floor = last;
        for r in &mut self.recs[at..] {
            if r.seq > floor {
                break;
            }
            floor += 1;
            if floor > MAX_SEQ {
                return Err(BufferError::NoRoom(prev, next));
            }
            r.seq = floor;
        }
        let mut ids = Vec::new();
        for (k, line) in lines.iter().enumerate() {
            let id = self.new_id();
            let seq = prev + step * (u32::try_from(k).unwrap_or(0) + 1);
            let text = self.fit(line);
            self.recs.insert(at + k, Record { id, seq, date, data: text });
            ids.push(id);
        }
        self.changed = true;
        Ok(ids)
    }

    /// Remove positions `from..=to`; the removed records.
    pub fn remove(&mut self, from: usize, to: usize) -> Vec<Record> {
        if from > to || from >= self.recs.len() {
            return Vec::new();
        }
        let to = to.min(self.recs.len() - 1);
        self.changed = true;
        self.recs.drain(from..=to).collect()
    }

    /// Renumber every record from `start` by `increment`. Fails when
    /// the numbers would pass 9999.99; nothing changes then.
    pub fn resequence(&mut self, start: u32, increment: u32) -> Result<(), BufferError> {
        let n = u32::try_from(self.recs.len()).unwrap_or(u32::MAX);
        let inc = increment.max(1);
        if n > 0 && start.saturating_add(inc.saturating_mul(n - 1)) > MAX_SEQ {
            return Err(BufferError::NoRoom(start, MAX_SEQ));
        }
        for (k, r) in self.recs.iter_mut().enumerate() {
            r.seq = start + inc * u32::try_from(k).unwrap_or(0);
        }
        self.changed = true;
        Ok(())
    }

    /// Every record's date set to `date` (the session default `Set
    /// records to date`).
    pub fn date_all(&mut self, date: u32) {
        for r in &mut self.recs {
            r.date = date;
        }
        self.changed = true;
    }

    /// The records as they are written back: sequence, date, data.
    #[must_use]
    pub fn to_records(&self) -> Vec<(u32, u32, String)> {
        self.recs
            .iter()
            .map(|r| (r.seq, r.date, r.data.clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(seqs: &[u32]) -> Buffer {
        Buffer::new(
            80,
            seqs.iter()
                .map(|s| (*s, 260101, format!("L{s}")))
                .collect(),
        )
    }

    fn seqs(b: &Buffer) -> Vec<u32> {
        b.records().iter().map(|r| r.seq).collect()
    }

    #[test]
    fn inserts_by_the_increment() {
        let mut b = buf(&[100, 200, 300]);
        b.insert(Some(0), &["a".into(), "b".into()], 1, 261023).unwrap();
        assert_eq!(seqs(&b), [100, 101, 102, 200, 300]);
        assert_eq!(b.get(1).unwrap().date, 261023);
        assert!(b.changed());
    }

    #[test]
    fn inserts_at_the_ends() {
        let mut b = buf(&[100, 200]);
        b.insert(None, &["first".into()], 100, 0).unwrap();
        assert_eq!(seqs(&b), [99, 100, 200]);
        b.insert(Some(2), &["last".into()], 100, 0).unwrap();
        assert_eq!(seqs(&b), [99, 100, 200, 300]);
    }

    #[test]
    fn narrows_the_step_then_pushes() {
        let mut b = buf(&[100, 104, 105]);
        b.insert(Some(0), &["a".into(), "b".into()], 100, 0).unwrap();
        assert_eq!(seqs(&b), [100, 101, 102, 104, 105]);
        b.insert(Some(3), &["c".into(), "d".into()], 1, 0).unwrap();
        assert_eq!(seqs(&b), [100, 101, 102, 104, 105, 106, 107]);
    }

    #[test]
    fn no_room_at_the_top() {
        let mut b = buf(&[MAX_SEQ]);
        assert!(b.insert(Some(0), &["x".into()], 1, 0).is_err());
    }

    #[test]
    fn data_changes_date_the_record() {
        let mut b = buf(&[100]);
        assert!(!b.set_data(0, "L100   ", 261023));
        assert!(b.set_data(0, "new", 261023));
        assert_eq!(b.get(0).unwrap().date, 261023);
        assert_eq!(b.get(0).unwrap().data, "new");
    }

    #[test]
    fn resequences() {
        let mut b = buf(&[5, 7, 300]);
        b.resequence(100, 100).unwrap();
        assert_eq!(seqs(&b), [100, 200, 300]);
        assert!(b.resequence(999_000, 1000).is_err());
    }

    #[test]
    fn data_is_cut_to_the_width() {
        let mut b = Buffer::new(5, vec![(100, 0, "abc".into())]);
        b.set_data(0, "abcdefgh", 0);
        assert_eq!(b.get(0).unwrap().data, "abcde");
    }

    #[test]
    fn removes() {
        let mut b = buf(&[100, 200, 300]);
        let gone = b.remove(0, 1);
        assert_eq!(gone.len(), 2);
        assert_eq!(seqs(&b), [300]);
    }
}
