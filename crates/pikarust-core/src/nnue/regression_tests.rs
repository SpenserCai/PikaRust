#[cfg(test)]
mod tests {
    use crate::nnue::accumulator::Accumulator;
    use crate::nnue::feature_transformer::{refresh_psq_accumulator, refresh_threat_accumulator};
    use crate::nnue::model::NnueModel;
    use crate::nnue::network::Network;
    use crate::nnue::simd::SimdOps;
    use crate::nnue::simd::scalar::Scalar;
    use crate::position::Position;

    fn load_network() -> Option<Network> {
        let model_path = std::path::Path::new("../../models/pikafish.nnue");
        if !model_path.exists() {
            return None;
        }
        let model = NnueModel::load(model_path).ok()?;
        Some(Network::new(model))
    }

    fn eval_position(net: &Network, pos: &Position) -> (i32, i32) {
        let mut psq_acc = Accumulator::new();
        let mut threat_acc = Accumulator::new();
        refresh_psq_accumulator(net.model(), pos, &mut psq_acc);
        refresh_threat_accumulator(net.model(), pos, &mut threat_acc);
        net.evaluate(
            &psq_acc.accumulation,
            &threat_acc.accumulation,
            &psq_acc.psqt_accumulation,
            &threat_acc.psqt_accumulation,
            &pos.piece_count,
            pos.side_to_move(),
        )
    }

    // ---------------------------------------------------------------
    // End-to-end NNUE snapshot tests
    // ---------------------------------------------------------------

    #[test]
    fn test_nnue_snapshot_startpos() {
        let Some(net) = load_network() else {
            return;
        };
        let pos = Position::start_pos().expect("start_pos");
        let (psqt, positional) = eval_position(&net, &pos);
        assert_eq!(psqt, 0, "startpos psqt changed: got {psqt}");
        assert_eq!(
            positional, 133,
            "startpos positional changed: got {positional}"
        );
    }

    #[test]
    fn test_nnue_snapshot_midgame() {
        let Some(net) = load_network() else {
            return;
        };
        let fen = "r1bakab1r/9/2n1c2c1/p1p1p1p1p/9/2P6/P3P1P1P/1C2C1N2/9/RNBAKAB1R w - - 0 5";
        let pos = Position::from_fen(fen).expect("parse fen");
        let (psqt, positional) = eval_position(&net, &pos);
        assert_eq!(psqt, 775, "midgame psqt changed: got {psqt}");
        assert_eq!(
            positional, -154,
            "midgame positional changed: got {positional}"
        );
    }

    #[test]
    fn test_nnue_snapshot_endgame() {
        let Some(net) = load_network() else {
            return;
        };
        let fen = "4k4/9/9/9/9/9/9/9/4r4/4K4 w - - 0 1";
        let pos = Position::from_fen(fen).expect("parse fen");
        let (psqt, positional) = eval_position(&net, &pos);
        assert_eq!(psqt, -1247, "endgame psqt changed: got {psqt}");
        assert_eq!(
            positional, -675,
            "endgame positional changed: got {positional}"
        );
    }

    // ---------------------------------------------------------------
    // Accumulator snapshot tests
    // ---------------------------------------------------------------

    #[test]
    fn test_accumulator_snapshot_startpos() {
        let Some(net) = load_network() else {
            return;
        };
        let pos = Position::start_pos().expect("start_pos");
        let mut psq_acc = Accumulator::new();
        let mut threat_acc = Accumulator::new();
        refresh_psq_accumulator(net.model(), &pos, &mut psq_acc);
        refresh_threat_accumulator(net.model(), &pos, &mut threat_acc);

        assert_eq!(psq_acc.accumulation[0][0..4], [2, -84, 28, 187]);
        assert_eq!(psq_acc.accumulation[1][0..4], [2, -84, 28, 187]);
        assert_eq!(threat_acc.accumulation[0][0..4], [-15, 40, -20, 127]);
    }

    // ---------------------------------------------------------------
    // Full refresh consistency: two independent refreshes must match
    // ---------------------------------------------------------------

    #[test]
    fn test_full_refresh_deterministic() {
        let Some(net) = load_network() else {
            return;
        };
        let pos = Position::start_pos().expect("start_pos");

        let mut acc1 = Accumulator::new();
        let mut acc2 = Accumulator::new();
        refresh_psq_accumulator(net.model(), &pos, &mut acc1);
        refresh_psq_accumulator(net.model(), &pos, &mut acc2);

        assert_eq!(
            acc1.accumulation[0][..],
            acc2.accumulation[0][..],
            "two full refreshes must produce identical results"
        );
        assert_eq!(acc1.psqt_accumulation[0][..], acc2.psqt_accumulation[0][..],);
    }

    // ---------------------------------------------------------------
    // NEON vs scalar comparison tests for missing operations
    // ---------------------------------------------------------------

    #[test]
    fn test_sqr_clipped_relu_neon_matches_scalar() {
        let mut input = [0i32; 32];
        for (i, val) in input.iter_mut().enumerate() {
            *val = (i as i32) * 300 - 4000;
        }

        let mut out_scalar = [0u8; 32];
        let mut out_dispatch = [0u8; 32];
        Scalar::sqr_clipped_relu(&input, &mut out_scalar, 6);

        let d = crate::nnue::simd::Dispatch::new();
        d.sqr_clipped_relu(&input, &mut out_dispatch, 6);
        assert_eq!(
            out_scalar, out_dispatch,
            "sqr_clipped_relu dispatch must match scalar"
        );
    }

    #[test]
    fn test_affine_propagate_neon_matches_scalar() {
        let mut input = [0u8; 64];
        for (i, val) in input.iter_mut().enumerate() {
            *val = if i % 3 == 0 { 0 } else { (i * 7 % 128) as u8 };
        }
        let mut weights = vec![0i8; 64 * 8];
        for (i, w) in weights.iter_mut().enumerate() {
            *w = ((i * 13 + 7) % 256) as i8;
        }
        let biases = [10i32, 20, 30, 40, 50, 60, 70, 80];

        let mut out_scalar = [0i32; 8];
        let mut out_dispatch = [0i32; 8];
        Scalar::affine_propagate(&input, &weights, &biases, &mut out_scalar, 64, 8);

        let d = crate::nnue::simd::Dispatch::new();
        d.affine_propagate(&input, &weights, &biases, &mut out_dispatch, 64, 8);
        assert_eq!(
            out_scalar, out_dispatch,
            "affine_propagate dispatch must match scalar"
        );
    }

    #[test]
    fn test_find_nnz_neon_matches_scalar() {
        let mut input = [0u8; 64];
        input[1] = 5;
        input[8] = 1;
        input[11] = 2;
        input[24] = 100;
        input[33] = 50;

        let mut nnz_scalar = Vec::new();
        let mut nnz_dispatch = Vec::new();
        Scalar::find_nnz(&input, &mut nnz_scalar);

        let d = crate::nnue::simd::Dispatch::new();
        d.find_nnz(&input, &mut nnz_dispatch);
        assert_eq!(
            nnz_scalar, nnz_dispatch,
            "find_nnz dispatch must match scalar"
        );
    }

    #[test]
    fn test_affine_propagate_sparse_neon_matches_scalar() {
        let mut input = [0u8; 64];
        for (i, val) in input.iter_mut().enumerate() {
            *val = if i % 5 == 0 { (i * 3 % 128) as u8 } else { 0 };
        }
        let mut weights = vec![0i8; 64 * 4];
        for (i, w) in weights.iter_mut().enumerate() {
            *w = ((i * 11 + 3) % 256) as i8;
        }
        let biases = [100i32, 200, 300, 400];

        let mut nnz = Vec::new();
        Scalar::find_nnz(&input, &mut nnz);

        let mut out_scalar = [0i32; 4];
        let mut out_dispatch = [0i32; 4];
        Scalar::affine_propagate_sparse(&input, &weights, &biases, &mut out_scalar, 4, &nnz);

        let d = crate::nnue::simd::Dispatch::new();
        d.affine_propagate_sparse(&input, &weights, &biases, &mut out_dispatch, 4, &nnz);
        assert_eq!(
            out_scalar, out_dispatch,
            "affine_propagate_sparse dispatch must match scalar"
        );
    }

    // ---------------------------------------------------------------
    // Realistic-size affine_propagate test (FC0: 1024→32)
    // ---------------------------------------------------------------

    #[test]
    fn test_affine_propagate_fc0_size_matches_scalar() {
        let mut input = [0u8; 1024];
        for (i, val) in input.iter_mut().enumerate() {
            *val = ((i * 7 + 3) % 128) as u8;
        }
        let mut weights = vec![0i8; 1024 * 32];
        for (i, w) in weights.iter_mut().enumerate() {
            *w = ((i * 13 + 7) % 256) as i8;
        }
        let biases = [0i32; 32];

        let mut out_scalar = [0i32; 32];
        let mut out_dispatch = [0i32; 32];
        Scalar::affine_propagate(&input, &weights, &biases, &mut out_scalar, 1024, 32);

        let d = crate::nnue::simd::Dispatch::new();
        d.affine_propagate(&input, &weights, &biases, &mut out_dispatch, 1024, 32);
        assert_eq!(
            out_scalar, out_dispatch,
            "FC0-size affine_propagate dispatch must match scalar"
        );
    }

    #[test]
    fn test_affine_propagate_with_real_model_weights() {
        use crate::nnue::model::{L2_BIG, WEIGHT_SCALE_BITS};
        use crate::nnue::simd::Dispatch;
        use crate::nnue::simd::SimdBackend;
        use crate::types::Color;

        let Some(net) = load_network() else {
            return;
        };
        let fen = "4k4/9/9/9/9/9/9/9/4r4/4K4 w - - 0 1";
        let pos = Position::from_fen(fen).expect("parse fen");
        let mut psq_acc = Accumulator::new();
        let mut threat_acc = Accumulator::new();
        refresh_psq_accumulator(net.model(), &pos, &mut psq_acc);
        refresh_threat_accumulator(net.model(), &pos, &mut threat_acc);

        let scalar_d = Dispatch::with_backend(SimdBackend::Scalar);
        let neon_d = Dispatch::new();

        let mut transformed_scalar = [0u8; 1024];
        let mut transformed_neon = [0u8; 1024];

        let perspectives = [Color::White, Color::Black];
        for (p, &perspective) in perspectives.iter().enumerate() {
            let offset = p * 512;
            let c = perspective as usize;
            scalar_d.transform_features(
                &psq_acc.accumulation[c],
                &threat_acc.accumulation[c],
                &mut transformed_scalar[offset..offset + 512],
            );
            neon_d.transform_features(
                &psq_acc.accumulation[c],
                &threat_acc.accumulation[c],
                &mut transformed_neon[offset..offset + 512],
            );
        }
        assert_eq!(
            transformed_scalar[..],
            transformed_neon[..],
            "transform_features must match for endgame position"
        );

        let bucket =
            crate::nnue::network::make_layer_stack_bucket(&pos.piece_count, pos.side_to_move());
        let ls = &net.model().layer_stacks[bucket];

        let mut fc0_scalar = [0i32; 32];
        let mut fc0_neon = [0i32; 32];
        scalar_d.affine_propagate(
            &transformed_scalar,
            &ls.fc0_weights,
            ls.fc0_biases.as_slice(),
            &mut fc0_scalar,
            1024,
            32,
        );
        neon_d.affine_propagate(
            &transformed_neon,
            &ls.fc0_weights,
            ls.fc0_biases.as_slice(),
            &mut fc0_neon,
            1024,
            32,
        );

        for i in 0..32 {
            assert_eq!(
                fc0_scalar[i], fc0_neon[i],
                "FC0 output[{i}] mismatch: scalar={}, neon={}",
                fc0_scalar[i], fc0_neon[i]
            );
        }

        let mut sqr_scalar = [0u8; L2_BIG];
        let mut sqr_neon = [0u8; L2_BIG];
        scalar_d.sqr_clipped_relu(&fc0_scalar[..L2_BIG], &mut sqr_scalar, WEIGHT_SCALE_BITS);
        neon_d.sqr_clipped_relu(&fc0_neon[..L2_BIG], &mut sqr_neon, WEIGHT_SCALE_BITS);
        assert_eq!(
            sqr_scalar[..],
            sqr_neon[..],
            "sqr_clipped_relu must match for endgame FC0 output"
        );

        let mut relu_scalar = [0u8; L2_BIG];
        let mut relu_neon = [0u8; L2_BIG];
        scalar_d.clipped_relu(&fc0_scalar[..L2_BIG], &mut relu_scalar, WEIGHT_SCALE_BITS);
        neon_d.clipped_relu(&fc0_neon[..L2_BIG], &mut relu_neon, WEIGHT_SCALE_BITS);
        assert_eq!(
            relu_scalar[..],
            relu_neon[..],
            "clipped_relu must match for endgame FC0 output"
        );
    }

    // ---------------------------------------------------------------
    // Search tactical test: known best move
    // ---------------------------------------------------------------

    #[test]
    fn test_search_finds_move_at_depth() {
        use crate::engine::{Engine, SearchLimits};
        use crate::types::Move;

        let fen = "4k4/4a4/9/9/9/9/9/4r4/4A4/4K4 b - - 0 1";

        let mut engine = Engine::new().expect("engine init");
        engine.set_position(fen, &[]).expect("set position");

        let limits = SearchLimits {
            depth: Some(5),
            ..SearchLimits::default()
        };
        let result = engine.go(&limits);
        assert_ne!(
            result.best_move,
            Move::NONE,
            "search should find a move at depth 5"
        );
        assert!(result.depth >= 5, "should reach depth 5");
    }
}
