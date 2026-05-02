use std::sync::atomic::{AtomicU64, Ordering};

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

// TTEntry is stored as two AtomicU64 for lock-free concurrent access.
// Layout of data64 (first u64):
//   bits  0..15: key16
//   bits 16..23: depth8
//   bits 24..31: genBound8
//   bits 32..47: move16
//   bits 48..63: value16
// Layout of eval64 (second u64):
//   bits  0..15: eval16
//   bits 16..63: unused
#[repr(C)]
struct TTEntry {
    data64: AtomicU64,
    eval64: AtomicU64,
}

impl TTEntry {
    fn read(&self) -> (u16, u8, u8, u16, i16, i16) {
        let d = self.data64.load(Ordering::Relaxed);
        let e = self.eval64.load(Ordering::Relaxed);

        let key16 = d as u16;
        let depth8 = (d >> 16) as u8;
        let gen_bound8 = (d >> 24) as u8;
        let move16 = (d >> 32) as u16;
        let value16 = (d >> 48) as i16;
        let eval16 = e as i16;

        (key16, depth8, gen_bound8, move16, value16, eval16)
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
        let d = self.data64.load(Ordering::Relaxed);
        (d >> 16) as u8 != 0
    }

    fn key16(&self) -> u16 {
        self.data64.load(Ordering::Relaxed) as u16
    }

    fn depth8(&self) -> u8 {
        (self.data64.load(Ordering::Relaxed) >> 16) as u8
    }

    fn gen_bound8(&self) -> u8 {
        (self.data64.load(Ordering::Relaxed) >> 24) as u8
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

        let move16 = if m.raw() != 0 || new_key16 != old_key16 {
            m.raw()
        } else {
            let old_d = self.data64.load(Ordering::Relaxed);
            (old_d >> 32) as u16
        };

        let old_depth8 = self.depth8();
        let pv_u8 = u8::from(pv);
        let new_depth8 = (d - DEPTH_NONE) as u8;

        if b == Bound::Exact
            || new_key16 != old_key16
            || new_depth8.wrapping_add(2 * pv_u8) > old_depth8.wrapping_sub(4)
            || self.relative_age(curr_generation) != 0
        {
            let gen_bound8 = curr_generation | ((b as u8) << BOUND_SHIFT) | (pv_u8 << PV_SHIFT);

            let data = u64::from(new_key16)
                | (u64::from(new_depth8) << 16)
                | (u64::from(gen_bound8) << 24)
                | (u64::from(move16) << 32)
                | (u64::from(v as u16) << 48);

            let eval = u64::from(ev as i16 as u16);

            self.data64.store(data, Ordering::Relaxed);
            self.eval64.store(eval, Ordering::Relaxed);
        }
    }
}

#[repr(C, align(32))]
struct Cluster {
    entries: [TTEntry; CLUSTER_SIZE],
    _padding: [u8; 2],
}

const _: () = assert!(size_of::<Cluster>() == 64);

pub struct ProbeResult {
    pub found: bool,
    pub data: TTData,
    pub writer: TTWriter,
}

pub struct TTWriter {
    entry_ptr: *const TTEntry,
}

// SAFETY: TTEntry uses AtomicU64 internally, so concurrent access is safe.
#[allow(unsafe_code)]
unsafe impl Send for TTWriter {}
#[allow(unsafe_code)]
unsafe impl Sync for TTWriter {}

impl TTWriter {
    #[allow(clippy::many_single_char_names)]
    #[allow(clippy::too_many_arguments)]
    #[allow(unsafe_code)]
    pub fn write(
        &self,
        k: Key,
        v: Value,
        pv: bool,
        b: Bound,
        d: Depth,
        m: Move,
        ev: Value,
        generation: u8,
    ) {
        // SAFETY: The pointer is valid for the lifetime of the TT.
        let entry = unsafe { &*self.entry_ptr };
        entry.save(k, v, pv, b, d, m, ev, generation);
    }
}

pub struct TranspositionTable {
    clusters: Vec<Cluster>,
    cluster_count: usize,
    generation8: u8,
}

// SAFETY: All access to entries is through AtomicU64 with Relaxed ordering.
#[allow(unsafe_code)]
unsafe impl Send for TranspositionTable {}
#[allow(unsafe_code)]
unsafe impl Sync for TranspositionTable {}

impl TranspositionTable {
    pub fn new(mb_size: usize) -> Self {
        let cluster_count = (mb_size * 1024 * 1024) / size_of::<Cluster>();
        let cluster_count = cluster_count.max(1);

        let mut clusters = Vec::with_capacity(cluster_count);
        for _ in 0..cluster_count {
            clusters.push(Cluster {
                entries: [
                    TTEntry {
                        data64: AtomicU64::new(0),
                        eval64: AtomicU64::new(0),
                    },
                    TTEntry {
                        data64: AtomicU64::new(0),
                        eval64: AtomicU64::new(0),
                    },
                    TTEntry {
                        data64: AtomicU64::new(0),
                        eval64: AtomicU64::new(0),
                    },
                ],
                _padding: [0; 2],
            });
        }

        Self {
            clusters,
            cluster_count,
            generation8: 0,
        }
    }

    pub fn resize(&mut self, mb_size: usize) {
        let cluster_count = (mb_size * 1024 * 1024) / size_of::<Cluster>();
        let cluster_count = cluster_count.max(1);

        self.clusters.clear();
        self.clusters.reserve(cluster_count);
        for _ in 0..cluster_count {
            self.clusters.push(Cluster {
                entries: [
                    TTEntry {
                        data64: AtomicU64::new(0),
                        eval64: AtomicU64::new(0),
                    },
                    TTEntry {
                        data64: AtomicU64::new(0),
                        eval64: AtomicU64::new(0),
                    },
                    TTEntry {
                        data64: AtomicU64::new(0),
                        eval64: AtomicU64::new(0),
                    },
                ],
                _padding: [0; 2],
            });
        }
        self.cluster_count = cluster_count;
        self.generation8 = 0;
    }

    pub fn clear(&mut self) {
        for cluster in &self.clusters {
            for entry in &cluster.entries {
                entry.data64.store(0, Ordering::Relaxed);
                entry.eval64.store(0, Ordering::Relaxed);
            }
        }
        self.generation8 = 0;
    }

    pub const fn new_search(&mut self) {
        self.generation8 = self.generation8.wrapping_add(1) & GENERATION_MASK;
    }

    #[inline]
    pub const fn generation(&self) -> u8 {
        self.generation8
    }

    fn first_entry(&self, key: Key) -> &[TTEntry; CLUSTER_SIZE] {
        let idx = mul_hi64(key, self.cluster_count as u64) as usize;
        &self.clusters[idx].entries
    }

    pub fn probe(&self, key: Key) -> ProbeResult {
        let entries = self.first_entry(key);
        let key16 = key as u16;

        for entry in entries {
            if entry.key16() == key16 {
                return ProbeResult {
                    found: entry.is_occupied(),
                    data: entry.to_tt_data(),
                    writer: TTWriter {
                        entry_ptr: std::ptr::from_ref(entry),
                    },
                };
            }
        }

        let mut replace_idx = 0usize;
        let mut replace_score = i32::from(entries[0].depth8())
            - 8 * i32::from(entries[0].relative_age(self.generation8));

        for (i, entry) in entries.iter().enumerate().skip(1) {
            let score =
                i32::from(entry.depth8()) - 8 * i32::from(entry.relative_age(self.generation8));
            if score < replace_score {
                replace_score = score;
                replace_idx = i;
            }
        }

        ProbeResult {
            found: false,
            data: TTData::EMPTY,
            writer: TTWriter {
                entry_ptr: std::ptr::from_ref(&entries[replace_idx]),
            },
        }
    }

    pub fn hashfull(&self, max_age: u8) -> i32 {
        let sample = self.cluster_count.min(1000);
        let mut cnt = 0;
        for i in 0..sample {
            for entry in &self.clusters[i].entries {
                if entry.is_occupied() && entry.relative_age(self.generation8) <= max_age {
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
    fn test_tt_new_and_clear() {
        let mut tt = TranspositionTable::new(1);
        assert!(tt.cluster_count > 0);
        tt.clear();
        assert_eq!(tt.generation(), 0);
    }

    #[test]
    fn test_tt_new_search_generation() {
        let mut tt = TranspositionTable::new(1);
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
            .write(key, 100, true, Bound::Exact, 5, m, 50, tt.generation());

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
            .write(key, 10, false, Bound::Upper, 3, m1, 5, tt.generation());

        let r = tt.probe(key);
        r.writer
            .write(key, 20, true, Bound::Exact, 8, m2, 15, tt.generation());

        let r = tt.probe(key);
        assert!(r.found);
        assert_eq!(r.data.tt_move, m2);
        assert_eq!(r.data.value, 20);
        assert_eq!(r.data.depth, 8);
    }

    #[test]
    fn test_tt_hashfull() {
        let tt = TranspositionTable::new(1);
        let hf = tt.hashfull(0);
        assert_eq!(hf, 0);
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
        let mut tt = TranspositionTable::new(1);
        for _ in 0..100 {
            tt.new_search();
        }
        assert!(tt.generation() < 32);
    }

    #[test]
    fn test_tt_different_keys_same_cluster() {
        let tt = TranspositionTable::new(1);
        // Use keys with different low 16 bits so they occupy separate entries
        // within the same cluster (same high bits → same cluster index).
        let key1: Key = 0x0000_0000_0000_0001;
        let key2: Key = 0x0000_0000_0000_0002;
        let m1 = Move::make(Square::SQ_A0, Square::SQ_A1);
        let m2 = Move::make(Square::SQ_B0, Square::SQ_B1);

        let r = tt.probe(key1);
        r.writer
            .write(key1, 10, false, Bound::Lower, 3, m1, 5, tt.generation());

        let r = tt.probe(key2);
        r.writer
            .write(key2, 20, false, Bound::Upper, 5, m2, 15, tt.generation());

        let r1 = tt.probe(key1);
        if r1.found {
            assert_eq!(r1.data.value, 10);
        }
    }
}
