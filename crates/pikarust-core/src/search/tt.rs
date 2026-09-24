// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2004-2026 The Stockfish developers (see notices/upstream/Pikafish-AUTHORS)
// Copyright (c) 2026 SpenserCai and PikaRust contributors
// Rust adaptation and modifications, 2026; see NOTICE.md for upstream sources.
// Distributed without warranty; see LICENSE and notices/upstream/Pikafish-COPYRIGHT.

use std::sync::atomic::{AtomicU8, AtomicU16, AtomicUsize, Ordering};

use crate::types::{Bound, DEPTH_NONE, Depth, Key, Move, VALUE_NONE, Value};

const GENERATION_BITS: u8 = 5;
const GENERATION_MASK: u8 = (1 << GENERATION_BITS) - 1;
const BOUND_SHIFT: u8 = GENERATION_BITS;
const PV_SHIFT: u8 = BOUND_SHIFT + 2;
const CLUSTER_SIZE: usize = 3;

#[derive(Copy, Clone, Debug)]
pub struct TTData {
    pub tt_move: Move,
    pub value: Value,
    pub eval: Value,
    pub depth: Depth,
    pub bound: Bound,
    pub is_pv: bool,
}

impl TTData {
    pub const EMPTY: Self = Self {
        tt_move: Move::NONE,
        value: VALUE_NONE,
        eval: VALUE_NONE,
        depth: DEPTH_NONE,
        bound: Bound::None,
        is_pv: false,
    };
}

// Individual atomic fields preserve Pikafish's 10-byte entry and table capacity.
// A probe may observe fields from different concurrent writes, so TT data remains
// an advisory search hint rather than a coherent snapshot. Each field access is
// nevertheless race-free; a C++-style "benign" non-atomic race is not sound Rust.
#[repr(C)]
struct TTEntry {
    key16: AtomicU16,
    depth8: AtomicU8,
    gen_bound8: AtomicU8,
    move16: AtomicU16,
    value16: AtomicU16,
    eval16: AtomicU16,
}

impl TTEntry {
    const fn zero() -> Self {
        Self {
            key16: AtomicU16::new(0),
            depth8: AtomicU8::new(0),
            gen_bound8: AtomicU8::new(0),
            move16: AtomicU16::new(0),
            value16: AtomicU16::new(0),
            eval16: AtomicU16::new(0),
        }
    }

    fn clear(&self) {
        self.key16.store(0, Ordering::Relaxed);
        self.depth8.store(0, Ordering::Relaxed);
        self.gen_bound8.store(0, Ordering::Relaxed);
        self.move16.store(0, Ordering::Relaxed);
        self.value16.store(0, Ordering::Relaxed);
        self.eval16.store(0, Ordering::Relaxed);
    }

    fn read(&self) -> (u16, u8, u8, u16, i16, i16) {
        (
            self.key16(),
            self.depth8(),
            self.gen_bound8(),
            self.move16.load(Ordering::Relaxed),
            self.value16.load(Ordering::Relaxed) as i16,
            self.eval16.load(Ordering::Relaxed) as i16,
        )
    }

    fn to_tt_data(&self) -> TTData {
        let (_, depth8, gen_bound8, move16, value16, eval16) = self.read();

        let bound_raw = (gen_bound8 >> BOUND_SHIFT) & 0x03;
        let bound = match bound_raw {
            1 => Bound::Upper,
            2 => Bound::Lower,
            3 => Bound::Exact,
            _ => Bound::None,
        };

        TTData {
            tt_move: Move::from_raw(move16),
            value: i32::from(value16),
            eval: i32::from(eval16),
            depth: i32::from(depth8) + DEPTH_NONE,
            bound,
            is_pv: (gen_bound8 >> PV_SHIFT) & 1 != 0,
        }
    }

    fn is_occupied(&self) -> bool {
        self.depth8() != 0
    }

    fn key16(&self) -> u16 {
        self.key16.load(Ordering::Relaxed)
    }

    fn depth8(&self) -> u8 {
        self.depth8.load(Ordering::Relaxed)
    }

    fn gen_bound8(&self) -> u8 {
        self.gen_bound8.load(Ordering::Relaxed)
    }

    fn relative_age(&self, curr_generation: u8) -> u8 {
        (curr_generation.wrapping_sub(self.gen_bound8())) & GENERATION_MASK
    }

    #[allow(clippy::many_single_char_names)]
    #[allow(clippy::too_many_arguments)]
    fn save(
        &self,
        k: Key,
        v: Value,
        pv: bool,
        b: Bound,
        d: Depth,
        m: Move,
        ev: Value,
        curr_generation: u8,
    ) {
        let new_key16 = k as u16;
        let old_key16 = self.key16();
        let old_depth8 = self.depth8();

        if m.raw() != 0 || new_key16 != old_key16 {
            self.move16.store(m.raw(), Ordering::Relaxed);
        }
        let pv_u8 = u8::from(pv);
        let new_depth8 = (d - DEPTH_NONE) as u8;

        if b == Bound::Exact
            || new_key16 != old_key16
            || i32::from(new_depth8) + 2 * i32::from(pv_u8) > i32::from(old_depth8) - 4
            || self.relative_age(curr_generation) != 0
        {
            let gen_bound8 = curr_generation | ((b as u8) << BOUND_SHIFT) | (pv_u8 << PV_SHIFT);

            self.key16.store(new_key16, Ordering::Relaxed);
            self.depth8.store(new_depth8, Ordering::Relaxed);
            self.gen_bound8.store(gen_bound8, Ordering::Relaxed);
            self.value16.store(v as u16, Ordering::Relaxed);
            self.eval16.store(ev as u16, Ordering::Relaxed);
        }
    }
}

// 3 entries × 10 bytes = 30 bytes + 2 padding = 32 bytes, aligned to 32.
#[repr(C, align(32))]
struct Cluster {
    entries: [TTEntry; CLUSTER_SIZE],
    _padding: [u8; 2],
}

const _: () = assert!(size_of::<Cluster>() == 32);
const _: () = assert!(size_of::<TTEntry>() == 10);

pub struct ProbeResult {
    pub found: bool,
    pub data: TTData,
    pub writer: TTWriter,
}

/// A reusable slot selected by a transposition-table probe.
///
/// This handle owns no table memory and can safely outlive its table. Writing
/// requires the original table explicitly; using another table or a table that
/// has since been resized is rejected. It does not retain an `Arc` or borrow a
/// worker across recursive searches.
pub struct TTWriter {
    table_id: usize,
    slot: usize,
}

impl TTWriter {
    /// Attempts to save data in the slot selected by the original probe.
    ///
    /// Returns `false` without writing if `table` is a different table or has
    /// been resized since the probe. Returns `true` for a current handle; the
    /// normal replacement policy still decides which entry fields to update.
    #[allow(clippy::many_single_char_names)]
    #[allow(clippy::too_many_arguments)]
    pub fn write(
        &self,
        table: &TranspositionTable,
        k: Key,
        v: Value,
        pv: bool,
        b: Bound,
        d: Depth,
        m: Move,
        ev: Value,
        generation: u8,
    ) -> bool {
        if self.table_id != table.id {
            return false;
        }
        let Some(cluster) = table.clusters.get(self.slot / CLUSTER_SIZE) else {
            return false;
        };
        let entry = &cluster.entries[self.slot % CLUSTER_SIZE];
        entry.save(k, v, pv, b, d, m, ev, generation);
        true
    }
}

pub struct TranspositionTable {
    clusters: Vec<Cluster>,
    cluster_count: usize,
    generation8: AtomicU8,
    id: usize,
}

// Handles must not accidentally become valid again when an allocation address
// is reused. IDs are assigned only when allocating/resizing a table; probing
// and writing never touch this shared counter. Exhaustion fails before reuse.
static NEXT_TABLE_ID: AtomicUsize = AtomicUsize::new(0);

fn next_table_id() -> usize {
    NEXT_TABLE_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("transposition table identity exhausted")
}

impl TranspositionTable {
    pub fn new(mb_size: usize) -> Self {
        let cluster_count = (mb_size * 1024 * 1024) / size_of::<Cluster>();
        let cluster_count = cluster_count.max(1);

        let mut clusters = Vec::with_capacity(cluster_count);
        for _ in 0..cluster_count {
            clusters.push(Cluster {
                entries: [const { TTEntry::zero() }; CLUSTER_SIZE],
                _padding: [0; 2],
            });
        }

        Self {
            clusters,
            cluster_count,
            generation8: AtomicU8::new(0),
            id: next_table_id(),
        }
    }

    pub fn resize(&mut self, mb_size: usize) {
        self.id = next_table_id();
        let cluster_count = (mb_size * 1024 * 1024) / size_of::<Cluster>();
        let cluster_count = cluster_count.max(1);

        self.clusters.clear();
        self.clusters.reserve(cluster_count);
        for _ in 0..cluster_count {
            self.clusters.push(Cluster {
                entries: [const { TTEntry::zero() }; CLUSTER_SIZE],
                _padding: [0; 2],
            });
        }
        self.cluster_count = cluster_count;
        self.generation8.store(0, Ordering::Relaxed);
    }

    pub fn clear(&self) {
        for cluster in &self.clusters {
            for entry in &cluster.entries {
                entry.clear();
            }
        }
        self.generation8.store(0, Ordering::Relaxed);
    }

    pub fn new_search(&self) {
        let old = self.generation8.load(Ordering::Relaxed);
        self.generation8
            .store(old.wrapping_add(1) & GENERATION_MASK, Ordering::Relaxed);
    }

    #[inline]
    pub fn generation(&self) -> u8 {
        self.generation8.load(Ordering::Relaxed)
    }

    pub fn probe(&self, key: Key) -> ProbeResult {
        let cluster_idx = mul_hi64(key, self.cluster_count as u64) as usize;
        let entries = &self.clusters[cluster_idx].entries;
        let key16 = key as u16;

        for (entry_idx, entry) in entries.iter().enumerate() {
            if entry.key16() == key16 {
                return ProbeResult {
                    found: entry.is_occupied(),
                    data: entry.to_tt_data(),
                    writer: TTWriter {
                        table_id: self.id,
                        slot: cluster_idx * CLUSTER_SIZE + entry_idx,
                    },
                };
            }
        }

        let mut replace_idx = 0usize;
        let curr_gen = self.generation8.load(Ordering::Relaxed);
        let mut replace_score =
            i32::from(entries[0].depth8()) - 8 * i32::from(entries[0].relative_age(curr_gen));

        for (i, entry) in entries.iter().enumerate().skip(1) {
            let score = i32::from(entry.depth8()) - 8 * i32::from(entry.relative_age(curr_gen));
            if score < replace_score {
                replace_score = score;
                replace_idx = i;
            }
        }

        ProbeResult {
            found: false,
            data: TTData::EMPTY,
            writer: TTWriter {
                table_id: self.id,
                slot: cluster_idx * CLUSTER_SIZE + replace_idx,
            },
        }
    }

    pub fn hashfull(&self, max_age: u8) -> i32 {
        let sample = self.cluster_count.min(1000);
        let curr_gen = self.generation8.load(Ordering::Relaxed);
        let mut cnt = 0;
        for i in 0..sample {
            for entry in &self.clusters[i].entries {
                if entry.is_occupied() && entry.relative_age(curr_gen) <= max_age {
                    cnt += 1;
                }
            }
        }
        cnt / CLUSTER_SIZE as i32
    }
}

#[inline]
fn mul_hi64(a: u64, b: u64) -> u64 {
    ((u128::from(a) * u128::from(b)) >> 64) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Square;

    #[test]
    fn test_entry_size() {
        assert_eq!(size_of::<TTEntry>(), 10);
    }

    #[test]
    fn test_cluster_size() {
        assert_eq!(size_of::<Cluster>(), 32);
    }

    #[test]
    fn test_one_mebibyte_capacity() {
        let tt = TranspositionTable::new(1);
        assert_eq!(tt.cluster_count, 32768);
    }

    #[test]
    fn test_tt_new_and_clear() {
        let tt = TranspositionTable::new(1);
        assert!(tt.cluster_count > 0);
        tt.clear();
        assert_eq!(tt.generation(), 0);
    }

    #[test]
    fn test_tt_new_search_generation() {
        let tt = TranspositionTable::new(1);
        assert_eq!(tt.generation(), 0);
        tt.new_search();
        assert_eq!(tt.generation(), 1);
        for _ in 0..31 {
            tt.new_search();
        }
        assert_eq!(tt.generation(), 0);
    }

    #[test]
    fn test_tt_probe_empty() {
        let tt = TranspositionTable::new(1);
        let result = tt.probe(12345);
        assert!(!result.found);
        assert_eq!(result.data.tt_move, Move::NONE);
        assert_eq!(result.data.depth, DEPTH_NONE);
    }

    #[test]
    fn test_tt_save_and_probe() {
        let tt = TranspositionTable::new(1);
        let key: Key = 0xDEAD_BEEF_1234_5678;
        let m = Move::make(Square::SQ_E0, Square::SQ_E1);

        let result = tt.probe(key);
        result
            .writer
            .write(&tt, key, 100, true, Bound::Exact, 5, m, 50, tt.generation());

        let result2 = tt.probe(key);
        assert!(result2.found);
        assert_eq!(result2.data.tt_move, m);
        assert_eq!(result2.data.value, 100);
        assert_eq!(result2.data.eval, 50);
        assert_eq!(result2.data.depth, 5);
        assert_eq!(result2.data.bound, Bound::Exact);
        assert!(result2.data.is_pv);
    }

    #[test]
    fn test_tt_overwrite_with_deeper() {
        let tt = TranspositionTable::new(1);
        let key: Key = 0xAAAA_BBBB_CCCC_DDDD;
        let m1 = Move::make(Square::SQ_A0, Square::SQ_A1);
        let m2 = Move::make(Square::SQ_B0, Square::SQ_B1);

        let r = tt.probe(key);
        r.writer
            .write(&tt, key, 10, false, Bound::Upper, 3, m1, 5, tt.generation());

        let r = tt.probe(key);
        r.writer
            .write(&tt, key, 20, true, Bound::Exact, 8, m2, 15, tt.generation());

        let r = tt.probe(key);
        assert!(r.found);
        assert_eq!(r.data.tt_move, m2);
        assert_eq!(r.data.value, 20);
        assert_eq!(r.data.depth, 8);
    }

    #[test]
    fn test_tt_updates_move_even_without_replacing_entry() {
        let tt = TranspositionTable::new(1);
        let key: Key = 0x1234_5678_9ABC_DEF0;
        let m1 = Move::make(Square::SQ_A0, Square::SQ_A1);
        let m2 = Move::make(Square::SQ_B0, Square::SQ_B1);

        let r = tt.probe(key);
        r.writer.write(
            &tt,
            key,
            10,
            false,
            Bound::Upper,
            10,
            m1,
            5,
            tt.generation(),
        );

        let r = tt.probe(key);
        r.writer.write(
            &tt,
            key,
            20,
            false,
            Bound::Upper,
            1,
            m2,
            15,
            tt.generation(),
        );

        let r = tt.probe(key);
        assert!(r.found);
        assert_eq!(r.data.tt_move, m2);
        assert_eq!(r.data.value, 10);
        assert_eq!(r.data.eval, 5);
        assert_eq!(r.data.depth, 10);
        assert_eq!(r.data.bound, Bound::Upper);
    }

    #[test]
    fn test_tt_hashfull() {
        let tt = TranspositionTable::new(1);
        assert_eq!(tt.hashfull(0), 0);
    }

    #[test]
    fn test_mul_hi64() {
        assert_eq!(mul_hi64(0, 100), 0);
        assert_eq!(mul_hi64(u64::MAX, 1), 0);
        assert_eq!(mul_hi64(u64::MAX, u64::MAX), u64::MAX - 1);
    }

    #[test]
    fn test_tt_data_empty() {
        let data = TTData::EMPTY;
        assert_eq!(data.tt_move, Move::NONE);
        assert_eq!(data.value, VALUE_NONE);
        assert_eq!(data.eval, VALUE_NONE);
        assert_eq!(data.depth, DEPTH_NONE);
        assert_eq!(data.bound, Bound::None);
        assert!(!data.is_pv);
    }

    #[test]
    fn test_move_from_raw_roundtrip() {
        let m = Move::make(Square::SQ_E0, Square::SQ_E1);
        let raw = m.raw();
        let m2 = Move::from_raw(raw);
        assert_eq!(m, m2);
    }

    #[test]
    fn test_tt_generation_wraps() {
        let tt = TranspositionTable::new(1);
        for _ in 0..100 {
            tt.new_search();
        }
        assert!(tt.generation() < 32);
    }

    #[test]
    fn test_tt_different_keys_same_cluster() {
        let tt = TranspositionTable::new(1);
        let key1: Key = 0x0000_0000_0000_0001;
        let key2: Key = 0x0000_0000_0000_0002;
        let m1 = Move::make(Square::SQ_A0, Square::SQ_A1);
        let m2 = Move::make(Square::SQ_B0, Square::SQ_B1);

        let r = tt.probe(key1);
        r.writer.write(
            &tt,
            key1,
            10,
            false,
            Bound::Lower,
            3,
            m1,
            5,
            tt.generation(),
        );

        let r = tt.probe(key2);
        r.writer.write(
            &tt,
            key2,
            20,
            false,
            Bound::Upper,
            5,
            m2,
            15,
            tt.generation(),
        );

        let r1 = tt.probe(key1);
        if r1.found {
            assert_eq!(r1.data.value, 10);
        }
    }

    #[test]
    fn test_writer_rejects_another_table() {
        let original = TranspositionTable::new(0);
        let other = TranspositionTable::new(0);
        let key = 1;
        let writer = original.probe(key).writer;

        assert!(!writer.write(&other, key, 100, false, Bound::Exact, 5, Move::NONE, 50, 0));
        assert!(!other.probe(key).found);
        assert!(writer.write(
            &original,
            key,
            100,
            false,
            Bound::Exact,
            5,
            Move::NONE,
            50,
            0
        ));
        assert!(original.probe(key).found);
    }

    #[test]
    fn test_writer_can_outlive_its_table() {
        let key = 1;
        let writer = {
            let original = TranspositionTable::new(0);
            original.probe(key).writer
        };
        let replacement = TranspositionTable::new(0);

        assert!(!writer.write(
            &replacement,
            key,
            100,
            false,
            Bound::Exact,
            5,
            Move::NONE,
            50,
            0,
        ));
        assert!(!replacement.probe(key).found);
    }

    #[test]
    fn test_resize_invalidates_writers_even_at_the_same_capacity() {
        let mut tt = TranspositionTable::new(1);
        let key = u64::MAX;
        let writer = tt.probe(key).writer;
        tt.resize(1);
        assert!(!writer.write(&tt, key, 100, false, Bound::Exact, 5, Move::NONE, 50, 0));

        let writer = tt.probe(key).writer;
        tt.resize(0);
        assert!(!writer.write(&tt, key, 100, false, Bound::Exact, 5, Move::NONE, 50, 0));
        assert!(!tt.probe(key).found);
        assert!(tt.probe(key).writer.write(
            &tt,
            key,
            100,
            false,
            Bound::Exact,
            5,
            Move::NONE,
            50,
            0,
        ));
    }

    #[test]
    fn test_writer_survives_moving_its_table() {
        let tt = TranspositionTable::new(0);
        let key = 1;
        let writer = tt.probe(key).writer;
        let moved = Box::new(tt);

        assert!(writer.write(&moved, key, 100, false, Bound::Exact, 5, Move::NONE, 50, 0));
        assert_eq!(moved.probe(key).data.value, 100);
    }

    #[test]
    fn test_retains_move_when_updating_without_one() {
        let tt = TranspositionTable::new(0);
        let key = 1;
        let m = Move::make(Square::SQ_E0, Square::SQ_E1);
        let writer = tt.probe(key).writer;

        writer.write(&tt, key, 100, false, Bound::Lower, 10, m, 50, 0);
        writer.write(&tt, key, -200, true, Bound::Exact, 1, Move::NONE, -100, 0);

        let data = tt.probe(key).data;
        assert_eq!(data.tt_move, m);
        assert_eq!(data.value, -200);
        assert_eq!(data.eval, -100);
        assert_eq!(data.depth, 1);
        assert!(data.is_pv);
    }

    #[test]
    fn test_concurrent_probe_write_and_clear() {
        const fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TranspositionTable>();
        assert_send_sync::<TTWriter>();

        // All writers contend for three slots. Readers may see mixed snapshots,
        // but every observed field must be an intact value actually stored.
        let tt = TranspositionTable::new(0);
        let moves = [
            Move::make(Square::SQ_A0, Square::SQ_A1),
            Move::make(Square::SQ_B0, Square::SQ_B1),
            Move::make(Square::SQ_C0, Square::SQ_C1),
            Move::make(Square::SQ_D0, Square::SQ_D1),
        ];
        std::thread::scope(|scope| {
            for (worker, &m) in moves.iter().enumerate() {
                let tt = &tt;
                let moves = &moves;
                scope.spawn(move || {
                    for _ in 0..2000 {
                        let key = worker as u64 + 1;
                        let value = 1000 + worker as i32;
                        assert!(tt.probe(key).writer.write(
                            tt,
                            key,
                            value,
                            false,
                            Bound::Exact,
                            worker as i32 + 1,
                            m,
                            -value,
                            0,
                        ));
                        let probe = tt.probe(key);
                        if probe.found {
                            let data = probe.data;
                            assert!(data.tt_move == Move::NONE || moves.contains(&data.tt_move));
                            assert!(data.value == 0 || (1000..=1003).contains(&data.value));
                            assert!(data.eval == 0 || (-1003..=-1000).contains(&data.eval));
                            assert!(data.depth == DEPTH_NONE || (1..=4).contains(&data.depth));
                            assert!(matches!(data.bound, Bound::None | Bound::Exact));
                        }
                    }
                });
            }
            scope.spawn(|| {
                for _ in 0..2000 {
                    tt.clear();
                }
            });
        });
        tt.clear();
        assert!(!tt.probe(1).found);
    }
}
