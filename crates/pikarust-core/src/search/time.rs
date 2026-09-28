// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2004-2026 The Stockfish developers (see notices/upstream/Pikafish-AUTHORS)
// Copyright (c) 2026 SpenserCai and PikaRust contributors
// Rust adaptation and modifications, 2026; see NOTICE.md for upstream sources.
// Search/evaluation aligned with Pikafish b562d6ae, 2026-09-28.
// Distributed without warranty; see LICENSE and notices/upstream/Pikafish-COPYRIGHT.

use std::time::Instant;

use crate::types::{Color, Depth};

pub type TimePoint = u64;

#[derive(Clone)]
pub struct SearchLimits {
    pub time: [TimePoint; Color::NUM],
    pub inc: [TimePoint; Color::NUM],
    pub movestogo: i32,
    pub depth: Depth,
    pub mate: i32,
    pub perft: i32,
    pub infinite: bool,
    pub nodes: u64,
    pub movetime: TimePoint,
    pub move_overhead: TimePoint,
    pub ponder_mode: bool,
    pub search_moves: Vec<String>,
    pub start_time: Instant,
}

impl SearchLimits {
    pub fn new() -> Self {
        Self {
            time: [0; Color::NUM],
            inc: [0; Color::NUM],
            movestogo: 0,
            depth: 0,
            mate: 0,
            perft: 0,
            infinite: false,
            nodes: 0,
            movetime: 0,
            move_overhead: 10,
            ponder_mode: false,
            search_moves: Vec::new(),
            start_time: Instant::now(),
        }
    }

    pub const fn use_time_management(&self) -> bool {
        self.time[Color::White.index()] > 0 || self.time[Color::Black.index()] > 0
    }
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self::new()
    }
}

pub struct TimeManager {
    start_time: Instant,
    optimum_time: TimePoint,
    maximum_time: TimePoint,
    original_time_adjust: f64,
}

impl TimeManager {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            optimum_time: 0,
            maximum_time: 0,
            original_time_adjust: -1.0,
        }
    }

    pub fn init(
        &mut self,
        limits: &SearchLimits,
        us: Color,
        ply: i32,
        move_overhead: TimePoint,
        ponder: bool,
    ) {
        self.start_time = limits.start_time;

        if limits.movetime > 0 {
            self.optimum_time = limits.movetime;
            self.maximum_time = limits.movetime;
            return;
        }

        if !limits.use_time_management() {
            self.optimum_time = TimePoint::MAX / 2;
            self.maximum_time = TimePoint::MAX / 2;
            return;
        }

        let time_ms = limits.time[us.index()].max(1) as f64;
        let inc_ms = limits.inc[us.index()] as f64;
        let overhead = move_overhead as f64;

        let scaled_time = time_ms;

        let mtg = if scaled_time < 1000.0 && limits.movestogo == 0 {
            (scaled_time * 0.05) as i32
        } else if limits.movestogo > 0 {
            limits.movestogo.min(50)
        } else {
            50
        };

        let mtg_f64 = f64::from(mtg);

        let time_left = inc_ms
            .mul_add(mtg_f64 - 1.0, time_ms)
            .mul_add(1.0, -overhead * (2.0 + mtg_f64))
            .max(1.0);

        let (mut opt_scale, max_scale) = if limits.movestogo == 0 {
            if self.original_time_adjust < 0.0 {
                self.original_time_adjust = 0.3356f64.mul_add(time_left.log10(), -0.4903);
            }

            let log_time_in_sec = (scaled_time / 1000.0).log10();
            let opt_constant = 0.000_206_57f64
                .mul_add(log_time_in_sec, 0.003_401_3)
                .min(0.004_536);
            let max_constant = 2.8003f64.mul_add(log_time_in_sec, 3.7803).max(2.547);

            let ply_f64 = f64::from(ply);
            let os = ((ply_f64 + 2.711_11)
                .powf(0.434_33)
                .mul_add(opt_constant, 0.017_244))
            .min(0.205_77 * time_ms / time_left)
                * self.original_time_adjust;

            let ms = 7.002f64.min(ply_f64.mul_add(1.0 / 13.184, max_constant));
            (os, ms)
        } else {
            let ply_f64 = f64::from(ply);
            let os = (ply_f64.mul_add(1.0 / 116.4, 0.88) / mtg_f64).min(0.88 * time_ms / time_left);
            let ms = 0.11f64.mul_add(mtg_f64, 1.3);
            (os, ms)
        };

        if limits.movestogo != 1 {
            let opponent_time = limits.time[(!us).index()] as f64;
            let advantage = (time_ms - opponent_time) / (1.0 + time_ms + opponent_time);
            opt_scale *= 0.9f64.mul_add(advantage.min(0.0), 1.0);
        }

        self.optimum_time = (opt_scale * time_left).max(1.0) as TimePoint;
        self.maximum_time = (max_scale * self.optimum_time as f64)
            .min(0.8237f64.mul_add(time_ms, -overhead))
            .max(self.optimum_time as f64) as TimePoint;

        if ponder {
            self.optimum_time += self.optimum_time / 4;
        }
    }

    pub fn elapsed(&self) -> TimePoint {
        self.start_time.elapsed().as_millis() as TimePoint
    }

    pub fn elapsed_time(&self) -> TimePoint {
        self.elapsed()
    }

    #[inline]
    pub const fn optimum(&self) -> TimePoint {
        self.optimum_time
    }

    #[inline]
    pub const fn maximum(&self) -> TimePoint {
        self.maximum_time
    }

    pub const fn clear(&mut self) {
        self.optimum_time = 0;
        self.maximum_time = 0;
        self.original_time_adjust = -1.0;
    }
}

impl Default for TimeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_limits_default() {
        let limits = SearchLimits::new();
        assert_eq!(limits.time[0], 0);
        assert_eq!(limits.time[1], 0);
        assert!(!limits.use_time_management());
    }

    #[test]
    fn test_search_limits_use_time_management() {
        let mut limits = SearchLimits::new();
        limits.time[Color::White.index()] = 60000;
        assert!(limits.use_time_management());
    }

    #[test]
    fn test_time_manager_movetime() {
        let mut tm = TimeManager::new();
        let mut limits = SearchLimits::new();
        limits.movetime = 5000;
        tm.init(&limits, Color::White, 0, 50, false);
        assert_eq!(tm.optimum(), 5000);
        assert_eq!(tm.maximum(), 5000);
    }

    #[test]
    fn test_time_manager_infinite() {
        let mut tm = TimeManager::new();
        let limits = SearchLimits::new();
        tm.init(&limits, Color::White, 0, 50, false);
        assert!(tm.optimum() > 1_000_000);
    }

    #[test]
    fn test_time_manager_with_time() {
        let mut tm = TimeManager::new();
        let mut limits = SearchLimits::new();
        limits.time[Color::White.index()] = 60000;
        limits.inc[Color::White.index()] = 1000;
        tm.init(&limits, Color::White, 10, 50, false);
        assert!(tm.optimum() > 0);
        assert!(tm.maximum() >= tm.optimum());
        assert!(tm.optimum() < 60000);
    }

    #[test]
    fn test_time_manager_with_movestogo() {
        let mut tm = TimeManager::new();
        let mut limits = SearchLimits::new();
        limits.time[Color::White.index()] = 60000;
        limits.movestogo = 20;
        tm.init(&limits, Color::White, 10, 50, false);
        assert!(tm.optimum() > 0);
        assert!(tm.maximum() >= tm.optimum());
    }

    #[test]
    fn test_time_manager_clear() {
        let mut tm = TimeManager::new();
        let mut limits = SearchLimits::new();
        limits.time[Color::White.index()] = 60000;
        tm.init(&limits, Color::White, 10, 50, false);
        tm.clear();
        assert_eq!(tm.optimum(), 0);
        assert_eq!(tm.maximum(), 0);
    }

    #[test]
    fn test_time_manager_ponder() {
        let mut tm1 = TimeManager::new();
        let mut tm2 = TimeManager::new();
        let mut limits = SearchLimits::new();
        limits.time[Color::White.index()] = 60000;
        limits.inc[Color::White.index()] = 1000;

        tm1.init(&limits, Color::White, 10, 50, false);
        tm2.init(&limits, Color::White, 10, 50, true);

        assert!(tm2.optimum() > tm1.optimum());
    }

    #[test]
    fn time_disadvantage_reduces_budget_except_at_cycle_boundary() {
        let mut limits = SearchLimits::new();
        limits.time = [60_000, 60_000];
        let mut equal = TimeManager::new();
        equal.init(&limits, Color::White, 10, 10, false);
        limits.time[Color::Black.index()] = 180_000;
        let mut behind = TimeManager::new();
        behind.init(&limits, Color::White, 10, 10, false);
        assert!(behind.optimum() < equal.optimum());

        limits.movestogo = 1;
        behind.init(&limits, Color::White, 10, 10, false);
        limits.time[Color::Black.index()] = 60_000;
        equal.init(&limits, Color::White, 10, 10, false);
        assert_eq!(behind.optimum(), equal.optimum());
    }

    #[test]
    fn subsecond_cyclic_clock_keeps_supplied_move_horizon() {
        let mut limits = SearchLimits::new();
        limits.time = [500, 500];
        limits.movestogo = 40;
        let mut tm = TimeManager::new();
        tm.init(&limits, Color::White, 10, 0, false);
        // Upstream cyclic formula: floor(((0.88 + 10 / 116.4) / 40) * 500).
        assert_eq!(tm.optimum(), 12);
    }

    #[test]
    fn exhausted_clock_keeps_minimum_finite_time_budget() {
        let mut limits = SearchLimits::new();
        limits.time = [0, 60_000];
        let mut tm = TimeManager::new();
        tm.init(&limits, Color::White, 10, 10, false);
        assert_eq!(tm.optimum(), 1);
        assert_eq!(tm.maximum(), 1);
    }
}
