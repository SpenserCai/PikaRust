use crate::types::{Piece, Square};

use super::model::{PSQT_BUCKETS, TRANSFORMED_DIMS};

#[repr(C, align(64))]
#[derive(Clone)]
pub struct Accumulator {
    pub accumulation: [[i16; TRANSFORMED_DIMS]; 2],
    pub psqt_accumulation: [[i32; PSQT_BUCKETS]; 2],
    pub computed: [bool; 2],
}

impl Accumulator {
    pub const fn new() -> Self {
        Self {
            accumulation: [[0i16; TRANSFORMED_DIMS]; 2],
            psqt_accumulation: [[0i32; PSQT_BUCKETS]; 2],
            computed: [false; 2],
        }
    }

    pub const fn reset(&mut self) {
        self.accumulation = [[0i16; TRANSFORMED_DIMS]; 2];
        self.psqt_accumulation = [[0i32; PSQT_BUCKETS]; 2];
        self.computed = [false; 2];
    }
}

impl Default for Accumulator {
    fn default() -> Self {
        Self::new()
    }
}

pub const MAX_DIRTY_PIECES: usize = 3;

#[derive(Clone)]
pub struct DirtyPiece {
    pub dirty_num: usize,
    pub pc: [Piece; MAX_DIRTY_PIECES],
    pub from: [Square; MAX_DIRTY_PIECES],
    pub to: [Square; MAX_DIRTY_PIECES],
    pub requires_refresh: [bool; 2],
}

impl DirtyPiece {
    pub const fn new() -> Self {
        Self {
            dirty_num: 0,
            pc: [Piece::NONE; MAX_DIRTY_PIECES],
            from: [Square::NONE; MAX_DIRTY_PIECES],
            to: [Square::NONE; MAX_DIRTY_PIECES],
            requires_refresh: [false; 2],
        }
    }
}

impl Default for DirtyPiece {
    fn default() -> Self {
        Self::new()
    }
}

pub const MAX_DIRTY_THREATS: usize = 64;

#[derive(Copy, Clone)]
pub struct DirtyThreat(pub u32);

impl DirtyThreat {
    #[inline]
    pub const fn is_add(self) -> bool {
        (self.0 >> 31) != 0
    }

    #[inline]
    pub const fn pc_raw(self) -> u8 {
        ((self.0 >> 20) & 0xF) as u8
    }

    #[inline]
    pub const fn threatened_pc_raw(self) -> u8 {
        ((self.0 >> 16) & 0xF) as u8
    }

    #[inline]
    pub const fn threatened_sq_raw(self) -> u8 {
        ((self.0 >> 8) & 0xFF) as u8
    }

    #[inline]
    pub const fn pc_sq_raw(self) -> u8 {
        (self.0 & 0xFF) as u8
    }
}

#[derive(Clone)]
pub struct DirtyThreats {
    pub count: usize,
    pub threats: [DirtyThreat; MAX_DIRTY_THREATS],
}

impl DirtyThreats {
    pub const fn new() -> Self {
        Self {
            count: 0,
            threats: [DirtyThreat(0); MAX_DIRTY_THREATS],
        }
    }
}

impl Default for DirtyThreats {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
#[allow(clippy::large_enum_variant)]
pub enum DiffType {
    None,
    DirtyPiece(DirtyPiece),
    DirtyThreats(DirtyThreats),
}

impl Default for DiffType {
    fn default() -> Self {
        Self::None
    }
}

#[derive(Clone)]
pub struct AccumulatorState {
    pub acc: Accumulator,
    pub diff: DiffType,
}

impl AccumulatorState {
    pub const fn new() -> Self {
        Self {
            acc: Accumulator::new(),
            diff: DiffType::None,
        }
    }
}

impl Default for AccumulatorState {
    fn default() -> Self {
        Self::new()
    }
}

pub struct AccumulatorStack {
    psq: Vec<AccumulatorState>,
    threat: Vec<AccumulatorState>,
    size: usize,
}

impl AccumulatorStack {
    pub fn new(capacity: usize) -> Self {
        let mut psq = Vec::with_capacity(capacity);
        let mut threat = Vec::with_capacity(capacity);
        for _ in 0..capacity {
            psq.push(AccumulatorState::new());
            threat.push(AccumulatorState::new());
        }
        Self {
            psq,
            threat,
            size: 0,
        }
    }

    pub fn reset(&mut self) {
        self.size = 0;
        if let Some(entry) = self.psq.first_mut() {
            entry.acc.reset();
            entry.diff = DiffType::None;
        }
        if let Some(entry) = self.threat.first_mut() {
            entry.acc.reset();
            entry.diff = DiffType::None;
        }
    }

    pub const fn size(&self) -> usize {
        self.size
    }

    pub fn push(&mut self) {
        self.size += 1;
        if self.size >= self.psq.len() {
            self.psq.push(AccumulatorState::new());
            self.threat.push(AccumulatorState::new());
        }
        self.psq[self.size].acc.computed = [false; 2];
        self.psq[self.size].diff = DiffType::None;
        self.threat[self.size].acc.computed = [false; 2];
        self.threat[self.size].diff = DiffType::None;
    }

    pub fn pop(&mut self) {
        debug_assert!(self.size > 0);
        self.size -= 1;
    }

    pub fn current_psq(&self) -> &AccumulatorState {
        &self.psq[self.size]
    }

    pub fn current_psq_mut(&mut self) -> &mut AccumulatorState {
        &mut self.psq[self.size]
    }

    pub fn current_threat(&self) -> &AccumulatorState {
        &self.threat[self.size]
    }

    pub fn current_threat_mut(&mut self) -> &mut AccumulatorState {
        &mut self.threat[self.size]
    }

    pub fn prev_psq(&self) -> Option<&AccumulatorState> {
        if self.size > 0 {
            Some(&self.psq[self.size - 1])
        } else {
            None
        }
    }

    pub fn prev_threat(&self) -> Option<&AccumulatorState> {
        if self.size > 0 {
            Some(&self.threat[self.size - 1])
        } else {
            None
        }
    }

    pub fn set_psq_diff(&mut self, dirty: DirtyPiece) {
        self.psq[self.size].diff = DiffType::DirtyPiece(dirty);
    }

    pub fn set_threat_diff(&mut self, dirty: DirtyThreats) {
        self.threat[self.size].diff = DiffType::DirtyThreats(dirty);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_accumulator_new() {
        let acc = Accumulator::new();
        assert!(!acc.computed[0]);
        assert!(!acc.computed[1]);
        assert_eq!(acc.accumulation[0][0], 0);
        assert_eq!(acc.psqt_accumulation[1][15], 0);
    }

    #[test]
    fn test_accumulator_reset() {
        let mut acc = Accumulator::new();
        acc.accumulation[0][0] = 42;
        acc.computed[0] = true;
        acc.reset();
        assert_eq!(acc.accumulation[0][0], 0);
        assert!(!acc.computed[0]);
    }

    #[test]
    fn test_dirty_piece_new() {
        let dp = DirtyPiece::new();
        assert_eq!(dp.dirty_num, 0);
        assert_eq!(dp.pc[0], Piece::NONE);
        assert_eq!(dp.from[0], Square::NONE);
        assert!(!dp.requires_refresh[0]);
    }

    #[test]
    fn test_dirty_threat_encoding() {
        let dt = DirtyThreat(0x8034_1A05);
        assert!(dt.is_add());
        assert_eq!(dt.pc_raw(), 3);
        assert_eq!(dt.threatened_pc_raw(), 4);
        assert_eq!(dt.threatened_sq_raw(), 0x1A);
        assert_eq!(dt.pc_sq_raw(), 0x05);
    }

    #[test]
    fn test_dirty_threat_not_add() {
        let dt = DirtyThreat(0x0034_1A05);
        assert!(!dt.is_add());
    }

    #[test]
    fn test_accumulator_stack_push_pop() {
        let mut stack = AccumulatorStack::new(8);
        assert_eq!(stack.size(), 0);

        stack.push();
        assert_eq!(stack.size(), 1);
        assert!(!stack.current_psq().acc.computed[0]);

        stack.push();
        assert_eq!(stack.size(), 2);

        stack.pop();
        assert_eq!(stack.size(), 1);

        stack.pop();
        assert_eq!(stack.size(), 0);
    }

    #[test]
    fn test_accumulator_stack_reset() {
        let mut stack = AccumulatorStack::new(8);
        stack.push();
        stack.push();
        stack.reset();
        assert_eq!(stack.size(), 0);
    }

    #[test]
    fn test_accumulator_stack_prev() {
        let mut stack = AccumulatorStack::new(8);
        assert!(stack.prev_psq().is_none());
        assert!(stack.prev_threat().is_none());

        stack.push();
        assert!(stack.prev_psq().is_some());
        assert!(stack.prev_threat().is_some());
    }

    #[test]
    fn test_accumulator_stack_grows() {
        let mut stack = AccumulatorStack::new(2);
        for _ in 0..10 {
            stack.push();
        }
        assert_eq!(stack.size(), 10);
    }
}
