use crate::types::{Color, Move, Piece, PieceType, Square};

const BUTTERFLY_HISTORY_LIMIT: i16 = 7183;
const CAPTURE_HISTORY_LIMIT: i16 = 10692;
const CONTINUATION_HISTORY_LIMIT: i16 = 30000;
const LOW_PLY_HISTORY_LIMIT: i16 = 7183;
pub const CORRECTION_HISTORY_LIMIT: i32 = 1024;
pub const LOW_PLY_HISTORY_SIZE: usize = 5;

const UINT_16_HISTORY_SIZE: usize = u16::MAX as usize + 1;
const PAWN_HISTORY_SIZE: usize = 8192;
const CORRHIST_BASE_SIZE: usize = UINT_16_HISTORY_SIZE;

fn update_entry(entry: &mut i16, bonus: i32, limit: i16) {
    let clamped = bonus.clamp(-i32::from(limit), i32::from(limit));
    let val = i32::from(*entry);
    *entry = (val + clamped - val * clamped.abs() / i32::from(limit)) as i16;
}

// ---------------------------------------------------------------------------
// ButterflyHistory — indexed by [color][move.raw()]
// ---------------------------------------------------------------------------

pub struct ButterflyHistory {
    table: Box<[[i16; UINT_16_HISTORY_SIZE]; Color::NUM]>,
}

impl ButterflyHistory {
    #[allow(unsafe_code)]
    pub fn new() -> Self {
        let layout = std::alloc::Layout::new::<[[i16; UINT_16_HISTORY_SIZE]; Color::NUM]>();
        // SAFETY: layout is non-zero sized. alloc_zeroed returns properly aligned memory.
        // Zero-init is valid for i16 arrays.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = std::alloc::alloc_zeroed(layout)
                .cast::<[[i16; UINT_16_HISTORY_SIZE]; Color::NUM]>();
            Self {
                table: Box::from_raw(ptr),
            }
        }
    }

    pub fn fill(&mut self, val: i16) {
        for color_table in self.table.iter_mut() {
            color_table.fill(val);
        }
    }

    pub fn scale(&mut self, num: i32, den: i32) {
        for color_table in self.table.iter_mut() {
            for entry in color_table.iter_mut() {
                *entry = (i32::from(*entry) * num / den) as i16;
            }
        }
    }

    #[inline]
    pub fn get(&self, c: Color, m: Move) -> i16 {
        self.table[c.index()][m.raw() as usize]
    }

    #[inline]
    pub fn update(&mut self, c: Color, m: Move, bonus: i32) {
        update_entry(
            &mut self.table[c.index()][m.raw() as usize],
            bonus,
            BUTTERFLY_HISTORY_LIMIT,
        );
    }
}

impl Default for ButterflyHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// LowPlyHistory — indexed by [ply][move.raw()]
// ---------------------------------------------------------------------------

pub struct LowPlyHistory {
    table: Box<[[i16; UINT_16_HISTORY_SIZE]; LOW_PLY_HISTORY_SIZE]>,
}

impl LowPlyHistory {
    #[allow(unsafe_code)]
    pub fn new() -> Self {
        let layout =
            std::alloc::Layout::new::<[[i16; UINT_16_HISTORY_SIZE]; LOW_PLY_HISTORY_SIZE]>();
        // SAFETY: layout is non-zero sized. alloc_zeroed returns properly aligned memory.
        // Zero-init is valid for i16 arrays.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = std::alloc::alloc_zeroed(layout)
                .cast::<[[i16; UINT_16_HISTORY_SIZE]; LOW_PLY_HISTORY_SIZE]>();
            Self {
                table: Box::from_raw(ptr),
            }
        }
    }

    pub fn fill(&mut self, val: i16) {
        for ply_table in self.table.iter_mut() {
            ply_table.fill(val);
        }
    }

    #[inline]
    pub fn get(&self, ply: usize, m: Move) -> i16 {
        if ply < LOW_PLY_HISTORY_SIZE {
            self.table[ply][m.raw() as usize]
        } else {
            0
        }
    }

    #[inline]
    pub fn update(&mut self, ply: usize, m: Move, bonus: i32) {
        if ply < LOW_PLY_HISTORY_SIZE {
            update_entry(
                &mut self.table[ply][m.raw() as usize],
                bonus,
                LOW_PLY_HISTORY_LIMIT,
            );
        }
    }
}

impl Default for LowPlyHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// CapturePieceToHistory — indexed by [piece][to_sq][captured_piece_type]
// ---------------------------------------------------------------------------

pub struct CapturePieceToHistory {
    table: Box<[[[i16; PieceType::PIECE_TYPE_NB]; Square::NUM]; Piece::NUM]>,
}

impl CapturePieceToHistory {
    #[allow(unsafe_code)]
    pub fn new() -> Self {
        let layout = std::alloc::Layout::new::<
            [[[i16; PieceType::PIECE_TYPE_NB]; Square::NUM]; Piece::NUM],
        >();
        // SAFETY: layout is non-zero sized. alloc_zeroed returns properly aligned memory.
        // Zero-init is valid for i16 arrays.
        #[allow(clippy::cast_ptr_alignment)]
        unsafe {
            let ptr = std::alloc::alloc_zeroed(layout)
                .cast::<[[[i16; PieceType::PIECE_TYPE_NB]; Square::NUM]; Piece::NUM]>();
            Self {
                table: Box::from_raw(ptr),
            }
        }
    }

    pub fn fill(&mut self, val: i16) {
        for pc_table in self.table.iter_mut() {
            for sq_table in pc_table.iter_mut() {
                sq_table.fill(val);
            }
        }
    }

    #[inline]
    pub fn get(&self, pc: Piece, to: Square, captured_pt: PieceType) -> i16 {
        self.table[pc.index()][to.index()][captured_pt.index()]
    }

    #[inline]
    pub fn update(&mut self, pc: Piece, to: Square, captured_pt: PieceType, bonus: i32) {
        update_entry(
            &mut self.table[pc.index()][to.index()][captured_pt.index()],
            bonus,
            CAPTURE_HISTORY_LIMIT,
        );
    }
}

impl Default for CapturePieceToHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PieceToHistory — indexed by [piece][square], used for continuation history
// ---------------------------------------------------------------------------

pub struct PieceToHistory {
    pub table: [[i16; Square::NUM]; Piece::NUM],
}

impl PieceToHistory {
    pub const fn new() -> Self {
        Self {
            table: [[0i16; Square::NUM]; Piece::NUM],
        }
    }

    pub fn fill(&mut self, val: i16) {
        for pc_table in &mut self.table {
            pc_table.fill(val);
        }
    }

    #[inline]
    pub const fn get(&self, pc: Piece, sq: Square) -> i16 {
        self.table[pc.index()][sq.index()]
    }

    #[inline]
    pub fn update(&mut self, pc: Piece, sq: Square, bonus: i32) {
        update_entry(
            &mut self.table[pc.index()][sq.index()],
            bonus,
            CONTINUATION_HISTORY_LIMIT,
        );
    }

    /// Mutable reference to a specific entry, for use by `update_continuation_histories`
    /// which needs to read-then-write with a custom multiplier.
    #[inline]
    pub const fn entry_mut(&mut self, pc: Piece, sq: Square) -> &mut i16 {
        &mut self.table[pc.index()][sq.index()]
    }
}

impl Default for PieceToHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ContinuationHistoryTable / ContinuationHistory
// Indexed by [in_check][capture][piece][square] -> PieceToHistory
// ---------------------------------------------------------------------------

pub type ContinuationHistoryTable = Box<[[PieceToHistory; Square::NUM]; Piece::NUM]>;

#[allow(clippy::large_stack_frames)]
pub fn new_continuation_history() -> ContinuationHistoryTable {
    let mut table: Vec<[PieceToHistory; Square::NUM]> = Vec::with_capacity(Piece::NUM);
    for _ in 0..Piece::NUM {
        let mut sq_arr: Vec<PieceToHistory> = Vec::with_capacity(Square::NUM);
        for _ in 0..Square::NUM {
            sq_arr.push(PieceToHistory::new());
        }
        let Ok(arr) = sq_arr.try_into() else {
            unreachable!()
        };
        table.push(arr);
    }
    let Ok(boxed) = table.try_into() else {
        unreachable!()
    };
    boxed
}

pub struct ContinuationHistory {
    pub table: [[ContinuationHistoryTable; 2]; 2],
}

impl ContinuationHistory {
    pub fn new() -> Self {
        Self {
            table: [
                [new_continuation_history(), new_continuation_history()],
                [new_continuation_history(), new_continuation_history()],
            ],
        }
    }

    pub fn fill(&mut self, val: i16) {
        for in_check in &mut self.table {
            for capture in in_check {
                for pc_table in capture.iter_mut() {
                    for sq_table in pc_table.iter_mut() {
                        sq_table.fill(val);
                    }
                }
            }
        }
    }

    #[inline]
    pub fn get(&self, in_check: bool, capture: bool, pc: Piece, sq: Square) -> &PieceToHistory {
        &self.table[usize::from(in_check)][usize::from(capture)][pc.index()][sq.index()]
    }

    #[inline]
    pub fn get_mut(
        &mut self,
        in_check: bool,
        capture: bool,
        pc: Piece,
        sq: Square,
    ) -> &mut PieceToHistory {
        &mut self.table[usize::from(in_check)][usize::from(capture)][pc.index()][sq.index()]
    }
}

impl Default for ContinuationHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// PawnHistory — indexed by pawn_key hash -> [piece][square]
// ---------------------------------------------------------------------------

pub struct PawnHistoryEntry {
    table: [[i16; Square::NUM]; Piece::NUM],
}

impl PawnHistoryEntry {
    pub const fn new() -> Self {
        Self {
            table: [[0i16; Square::NUM]; Piece::NUM],
        }
    }

    pub fn fill(&mut self, val: i16) {
        for pc_table in &mut self.table {
            pc_table.fill(val);
        }
    }

    #[inline]
    pub const fn get(&self, pc: Piece, sq: Square) -> i16 {
        self.table[pc.index()][sq.index()]
    }

    #[inline]
    pub fn update(&mut self, pc: Piece, sq: Square, bonus: i32) {
        update_entry(&mut self.table[pc.index()][sq.index()], bonus, 8192);
    }
}

impl Default for PawnHistoryEntry {
    fn default() -> Self {
        Self::new()
    }
}

pub struct PawnHistory {
    table: Vec<PawnHistoryEntry>,
}

impl PawnHistory {
    pub fn new() -> Self {
        let mut table = Vec::with_capacity(PAWN_HISTORY_SIZE);
        for _ in 0..PAWN_HISTORY_SIZE {
            table.push(PawnHistoryEntry::new());
        }
        Self { table }
    }

    pub fn fill(&mut self, val: i16) {
        for entry in &mut self.table {
            entry.fill(val);
        }
    }

    #[inline]
    pub fn entry(&self, pawn_key: u64) -> &PawnHistoryEntry {
        &self.table[(pawn_key as usize) & (PAWN_HISTORY_SIZE - 1)]
    }

    #[inline]
    pub fn entry_mut(&mut self, pawn_key: u64) -> &mut PawnHistoryEntry {
        &mut self.table[(pawn_key as usize) & (PAWN_HISTORY_SIZE - 1)]
    }
}

impl Default for PawnHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// CorrectionBundle — per-color bundle of pawn/minor/nonpawn correction entries
// Matches C++ CorrectionBundle<i16, 1024>
// ---------------------------------------------------------------------------

pub struct CorrectionBundle {
    pub pawn: i16,
    pub minor: i16,
    pub non_pawn_white: i16,
    pub non_pawn_black: i16,
}

impl CorrectionBundle {
    pub const fn new() -> Self {
        Self {
            pawn: 0,
            minor: 0,
            non_pawn_white: 0,
            non_pawn_black: 0,
        }
    }

    fn update_field(field: &mut i16, bonus: i32) {
        let limit = CORRECTION_HISTORY_LIMIT;
        let clamped = bonus.clamp(-limit, limit);
        let val = i32::from(*field);
        *field = (val + clamped - val * clamped.abs() / limit) as i16;
    }

    pub fn update_pawn(&mut self, bonus: i32) {
        Self::update_field(&mut self.pawn, bonus);
    }

    pub fn update_minor(&mut self, bonus: i32) {
        Self::update_field(&mut self.minor, bonus);
    }

    pub fn update_non_pawn_white(&mut self, bonus: i32) {
        Self::update_field(&mut self.non_pawn_white, bonus);
    }

    pub fn update_non_pawn_black(&mut self, bonus: i32) {
        Self::update_field(&mut self.non_pawn_black, bonus);
    }

    pub const fn clear(&mut self) {
        self.pawn = 0;
        self.minor = 0;
        self.non_pawn_white = 0;
        self.non_pawn_black = 0;
    }
}

impl Default for CorrectionBundle {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// UnifiedCorrectionHistory — large table indexed by hash key, per-color bundles
// Matches C++ UnifiedCorrectionHistory
// ---------------------------------------------------------------------------

pub struct UnifiedCorrectionHistory {
    table: Vec<[CorrectionBundle; Color::NUM]>,
    size_minus_1: usize,
}

impl UnifiedCorrectionHistory {
    pub fn new(thread_count: usize) -> Self {
        let size = thread_count.next_power_of_two() * CORRHIST_BASE_SIZE;
        let mut table = Vec::with_capacity(size);
        for _ in 0..size {
            table.push([CorrectionBundle::new(), CorrectionBundle::new()]);
        }
        Self {
            table,
            size_minus_1: size - 1,
        }
    }

    pub fn clear(&mut self) {
        for entry in &mut self.table {
            entry[0].clear();
            entry[1].clear();
        }
    }

    #[inline]
    pub fn entry(&self, key: u64) -> &[CorrectionBundle; Color::NUM] {
        &self.table[(key as usize) & self.size_minus_1]
    }

    #[inline]
    pub fn entry_mut(&mut self, key: u64) -> &mut [CorrectionBundle; Color::NUM] {
        let idx = (key as usize) & self.size_minus_1;
        &mut self.table[idx]
    }
}

impl Default for UnifiedCorrectionHistory {
    fn default() -> Self {
        Self::new(1)
    }
}

// ---------------------------------------------------------------------------
// PieceToCorrHist — inner table for continuation correction history
// Indexed by [piece][square], limit = CORRECTION_HISTORY_LIMIT (1024)
// ---------------------------------------------------------------------------

pub struct PieceToCorrHist {
    pub table: [[i16; Square::NUM]; Piece::NUM],
}

impl PieceToCorrHist {
    pub const fn new() -> Self {
        Self {
            table: [[0i16; Square::NUM]; Piece::NUM],
        }
    }

    pub fn fill(&mut self, val: i16) {
        for pc_table in &mut self.table {
            pc_table.fill(val);
        }
    }

    #[inline]
    pub const fn get(&self, pc: Piece, sq: Square) -> i16 {
        self.table[pc.index()][sq.index()]
    }

    #[inline]
    pub fn update(&mut self, pc: Piece, sq: Square, bonus: i32) {
        let limit = CORRECTION_HISTORY_LIMIT;
        let entry = &mut self.table[pc.index()][sq.index()];
        let clamped = bonus.clamp(-limit, limit);
        let val = i32::from(*entry);
        *entry = (val + clamped - val * clamped.abs() / limit) as i16;
    }
}

impl Default for PieceToCorrHist {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ContinuationCorrectionHistory — outer [piece][square] -> PieceToCorrHist
// Each search stack entry stores an index (piece, square) into this table.
// ---------------------------------------------------------------------------

pub struct ContinuationCorrectionHistory {
    table: Box<[[PieceToCorrHist; Square::NUM]; Piece::NUM]>,
}

impl ContinuationCorrectionHistory {
    #[allow(clippy::large_stack_frames)]
    pub fn new() -> Self {
        let mut outer: Vec<[PieceToCorrHist; Square::NUM]> = Vec::with_capacity(Piece::NUM);
        for _ in 0..Piece::NUM {
            let mut inner: Vec<PieceToCorrHist> = Vec::with_capacity(Square::NUM);
            for _ in 0..Square::NUM {
                inner.push(PieceToCorrHist::new());
            }
            let Ok(arr) = inner.try_into() else {
                unreachable!()
            };
            outer.push(arr);
        }
        let Ok(boxed) = outer.try_into() else {
            unreachable!()
        };
        Self { table: boxed }
    }

    pub fn fill(&mut self, val: i16) {
        for pc_table in self.table.iter_mut() {
            for sq_table in pc_table.iter_mut() {
                sq_table.fill(val);
            }
        }
    }

    #[inline]
    pub fn get(&self, pc: Piece, sq: Square) -> &PieceToCorrHist {
        &self.table[pc.index()][sq.index()]
    }

    #[inline]
    pub fn get_mut(&mut self, pc: Piece, sq: Square) -> &mut PieceToCorrHist {
        &mut self.table[pc.index()][sq.index()]
    }
}

impl Default for ContinuationCorrectionHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// CorrectionHistoryEntry — legacy single-entry type (kept for compatibility)
// ---------------------------------------------------------------------------

pub struct CorrectionHistoryEntry {
    pub value: i16,
}

impl CorrectionHistoryEntry {
    pub const fn new() -> Self {
        Self { value: 0 }
    }

    /// Gravity-based update matching C++ `StatsEntry<i16, 1024>::operator<<`
    pub fn update(&mut self, bonus: i32) {
        let limit = CORRECTION_HISTORY_LIMIT;
        let clamped = bonus.clamp(-limit, limit);
        let val = i32::from(self.value);
        self.value = (val + clamped - val * clamped.abs() / limit) as i16;
    }
}

impl Default for CorrectionHistoryEntry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// TTMoveHistory
// ---------------------------------------------------------------------------

pub struct TTMoveHistory {
    value: i16,
}

impl TTMoveHistory {
    pub const fn new() -> Self {
        Self { value: 0 }
    }

    #[inline]
    pub const fn get(&self) -> i16 {
        self.value
    }

    pub fn update(&mut self, bonus: i32) {
        update_entry(&mut self.value, bonus, 8192);
    }

    pub const fn reset(&mut self) {
        self.value = 0;
    }
}

impl Default for TTMoveHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// ContHistIndex — index into ContinuationHistory / ContinuationCorrectionHistory
// Replaces C++ raw pointer (ss->continuationHistory / ss->continuationCorrectionHistory)
// ---------------------------------------------------------------------------

/// Index into `ContinuationHistory` and `ContinuationCorrectionHistory` tables.
/// Stores the (`in_check`, capture, piece, square) tuple that identifies which
/// `PieceToHistory` / `PieceToCorrHist` to use.
#[derive(Clone, Copy)]
pub struct ContHistIndex {
    pub in_check: bool,
    pub capture: bool,
    pub pc: Piece,
    pub sq: Square,
}

impl ContHistIndex {
    /// Sentinel value used for uninitialized stack entries.
    /// Points to `[false][false][NONE][SQ_A0]` which is always zero-filled.
    pub const SENTINEL: Self = Self {
        in_check: false,
        capture: false,
        pc: Piece::NONE,
        sq: Square::SQ_A0,
    };
}
