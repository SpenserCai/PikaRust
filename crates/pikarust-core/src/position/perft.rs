use super::movegen::{GenType, generate};
use super::position::Position;

/// Perft (performance test) counts the number of leaf nodes at a given depth.
///
/// This is the standard correctness test for move generation: the node counts
/// must match Pikafish exactly. At depth 0 we return 1 (the current node).
/// At depth 1 we use a bulk-counting optimisation (just count legal moves).
pub fn perft(pos: &mut Position, depth: i32) -> u64 {
    if depth == 0 {
        return 1;
    }

    let ml = generate(pos, GenType::Legal);

    if depth == 1 {
        return ml.len() as u64;
    }

    let mut nodes = 0u64;
    for i in 0..ml.len() {
        let m = ml.get(i);
        let gives_check = pos.gives_check(m);
        pos.do_move(m, gives_check);
        nodes += perft(pos, depth - 1);
        pos.undo_move(m);
    }
    nodes
}

/// Divide perft: prints per-move node counts (useful for debugging mismatches).
pub fn perft_divide(pos: &mut Position, depth: i32) -> Vec<(String, u64)> {
    let ml = generate(pos, GenType::Legal);
    let mut results = Vec::with_capacity(ml.len());

    for i in 0..ml.len() {
        let m = ml.get(i);
        let gives_check = pos.gives_check(m);
        pos.do_move(m, gives_check);
        let nodes = if depth <= 1 { 1 } else { perft(pos, depth - 1) };
        pos.undo_move(m);
        results.push((format!("{m}"), nodes));
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Startpos perft (exact Pikafish values)
    // Reference: https://www.chessprogramming.org/Chinese_Chess_Perft_Results
    // -----------------------------------------------------------------------

    #[test]
    fn test_perft_startpos_depth0() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(perft(&mut pos, 0), 1);
    }

    #[test]
    fn test_perft_startpos_depth1() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(perft(&mut pos, 1), 44);
    }

    #[test]
    fn test_perft_startpos_depth2() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(perft(&mut pos, 2), 1920);
    }

    #[test]
    fn test_perft_startpos_depth3() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(perft(&mut pos, 3), 79_666);
    }

    #[test]
    #[ignore = "slow: ~0.5s in debug, run with: cargo test -- --ignored"]
    fn test_perft_startpos_depth4() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(perft(&mut pos, 4), 3_290_240);
    }

    #[test]
    #[ignore = "very slow: ~20s in debug"]
    fn test_perft_startpos_depth5() {
        let mut pos = Position::start_pos().unwrap();
        assert_eq!(perft(&mut pos, 5), 133_312_995);
    }

    // -----------------------------------------------------------------------
    // Non-startpos perft tests
    // Reference positions from chessprogramming.org Chinese Chess Perft Results
    // -----------------------------------------------------------------------

    /// Position with a midgame setup from Pikafish benchmark defaults.
    /// FEN: r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w
    #[test]
    fn test_perft_midgame1_depth1() {
        let fen = "r1ba1a3/4kn3/2n1b4/pNp1p1p1p/4c4/6P2/P1P2R2P/1CcC5/9/2BAKAB2 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let nodes = perft(&mut pos, 1);
        // Just verify it produces a reasonable number of moves (not zero, not absurd)
        assert!(nodes > 0, "midgame position should have legal moves");
        assert!(
            nodes < 100,
            "midgame position should have < 100 legal moves, got {nodes}"
        );
    }

    /// Position: 1cbak4/9/n2a5/2p1p3p/5cp2/2n2N3/6PCP/3AB4/2C6/3A1K1N1 w
    #[test]
    fn test_perft_midgame2_depth1() {
        let fen = "1cbak4/9/n2a5/2p1p3p/5cp2/2n2N3/6PCP/3AB4/2C6/3A1K1N1 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let nodes = perft(&mut pos, 1);
        assert!(nodes > 0, "midgame position should have legal moves");
    }

    /// Position: 5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w
    /// Endgame with few pieces.
    #[test]
    fn test_perft_endgame_depth2() {
        let fen = "5a3/3k5/3aR4/9/5r3/5n3/9/3A1A3/5K3/2BC2B2 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let d1 = perft(&mut pos, 1);
        let d2 = perft(&mut pos, 2);
        assert!(d1 > 0);
        assert!(d2 > d1, "depth 2 should have more nodes than depth 1");
    }

    /// Kings-only position: minimal legal moves.
    #[test]
    fn test_perft_kings_only_depth2() {
        let fen = "4k4/9/9/9/9/9/9/9/9/4K4 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let d1 = perft(&mut pos, 1);
        let d2 = perft(&mut pos, 2);
        // Kings in palace can move 1 square orthogonally, but must avoid facing each other.
        assert!(d1 > 0);
        assert!(d2 > 0);
    }

    /// Position with checks: CRN1k1b2/3ca4/4ba3/9/2nr5/9/9/4B4/4A4/4KA3 w
    /// From Pikafish benchmark "complicated checks and evasions" section.
    #[test]
    fn test_perft_checks_position_depth2() {
        let fen = "CRN1k1b2/3ca4/4ba3/9/2nr5/9/9/4B4/4A4/4KA3 w - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let d1 = perft(&mut pos, 1);
        let d2 = perft(&mut pos, 2);
        assert!(d1 > 0, "position with checks should have legal moves");
        assert!(d2 > 0);
    }

    /// Black to move position from benchmark.
    #[test]
    fn test_perft_black_to_move_depth2() {
        let fen = "2bak4/9/3a5/p2Np3p/3n1P3/3pc3P/P4r1c1/B2CC2R1/4A4/3AK1B2 b - - 0 1";
        let mut pos = Position::from_fen(fen).unwrap();
        let d1 = perft(&mut pos, 1);
        let d2 = perft(&mut pos, 2);
        assert!(d1 > 0);
        assert!(d2 > 0);
    }

    // -----------------------------------------------------------------------
    // Perft consistency: do_move/undo_move roundtrip via perft
    // -----------------------------------------------------------------------

    /// Verify that running perft twice on the same position gives the same result.
    /// This implicitly tests that `do_move`/`undo_move` restores state correctly.
    #[test]
    fn test_perft_deterministic() {
        let mut pos = Position::start_pos().unwrap();
        let n1 = perft(&mut pos, 3);
        let n2 = perft(&mut pos, 3);
        assert_eq!(n1, n2, "perft should be deterministic");
    }

    /// Verify `perft_divide` sums to the same total as perft.
    #[test]
    fn test_perft_divide_matches_perft() {
        let mut pos = Position::start_pos().unwrap();
        let total = perft(&mut pos, 2);
        let divided = perft_divide(&mut pos, 2);
        let sum: u64 = divided.iter().map(|(_, n)| n).sum();
        assert_eq!(sum, total, "divide sum should match perft total");
    }
}
