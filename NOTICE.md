# Copyright and source notices

PikaRust is a Rust implementation of Pikafish. The engine and its directly linked
applications are distributed under **GPL-3.0-or-later**. The complete GPL version
3 text is in [LICENSE](LICENSE); the component boundaries and distribution
requirements are in [docs/licensing.md](docs/licensing.md).

Copyright (c) 2026 SpenserCai and PikaRust contributors.

The original PikaRust MIT notice is retained in [LICENSE-MIT](LICENSE-MIT).
It covers the independently licensed components identified in the licensing
policy and preserves the notice for original contributions previously offered
under MIT. It does not grant MIT permissions over Pikafish/Stockfish code or the
combined engine. Third-party notices remain applicable to their respective work.

## Pikafish and Stockfish

Upstream project: <https://github.com/official-pikafish/Pikafish>.

The algorithm reference is commit
[`76239d0b06720bfa4588989fd4ac7573e9dbf887`](https://github.com/official-pikafish/Pikafish/tree/76239d0b06720bfa4588989fd4ac7573e9dbf887),
also recorded in [scripts/reference.lock](scripts/reference.lock). This is the
alignment reference, not a claim that every historical port originated at that
revision. Git history records the Rust adaptations and subsequent changes.

The upstream source notices identify:

- Stockfish, a UCI chess playing engine derived from Glaurung 2.1.
  Copyright (C) 2004-2026 The Stockfish developers.
- Pikafish, a UCI chess variant playing engine derived from Stockfish.
  Copyright (C) 2018-2022 PikaCat++ (the magic tables in `src/magics.h`).

Both notices permit redistribution and modification under GPL version 3 or, at
the recipient's option, any later version, without warranty. Their original
headers are retained in
[Pikafish-COPYRIGHT](notices/upstream/Pikafish-COPYRIGHT). The pinned upstream
[AUTHORS](notices/upstream/Pikafish-AUTHORS) file is retained verbatim, including
its acknowledgments of the Stockfish and Fairy-Stockfish developers.

The source relationships below describe the engine implementation as a whole;
they do not assert that every line or mathematical constant is copyrightable or
copied. Paths in the right column are relative to upstream `src/`.

| PikaRust source | Upstream implementation and data |
| --- | --- |
| `crates/pikarust-core/src/types/` | `types.h` |
| `crates/pikarust-core/src/bitboard/` | `bitboard.cpp`, `bitboard.h`, `magics.h`; `magic_numbers.rs` contains converted upstream tables |
| `crates/pikarust-core/src/position/` | `position.cpp`, `position.h`, `movegen.cpp`, `movegen.h`, `perft.h`, and position/rule state |
| `crates/pikarust-core/src/search/` | `search.cpp`, `search.h`, `history.h`, `movepick.cpp`, `movepick.h`, `evaluate.cpp`, `score.cpp`, `thread.cpp`, `timeman.cpp`, `tt.cpp`, and corresponding headers |
| `crates/pikarust-core/src/nnue/` | `nnue/network.*`, `nnue/nnue_accumulator.*`, `nnue/nnue_feature_transformer.h`, `nnue/nnue_architecture.h`, `nnue/nnue_common.h`, `nnue/features/`, `nnue/layers/`, and `nnue/simd.h` |
| `crates/pikarust-bench/src/main.rs` | The 49 default benchmark positions from `benchmark.cpp` and the continuous search-state benchmark procedure |
| `crates/pikarust-core/tests/`, `e2e_platform/fixtures/`, and `e2e_platform/src/cases/` | Regression positions and comparisons against the pinned engine; individual oracle comments identify expected-output provenance |

PikaRust's changes include translation into Rust modules, ownership and error
handling, NNUE model loading and SIMD implementations, position and search
alignment fixes, a library API, applications, and validation tooling. These are
PikaRust adaptations, not unmodified upstream releases. This notice and the
explicit GPL/MIT component declarations were added on **2026-09-24**; source
history retains the dates and authors of earlier modifications.

## Independent MIT components

The UCI types/parser in `crates/uci-rs`, child-process transport in
`pikarust-web/bridge`, and browser interface in `pikarust-web/frontend` retain
MIT. Each directory includes its own `LICENSE`. Their manifests declare MIT;
they do not contain or link the search implementation. Distributing the native
engine with them still requires the engine's GPL notices and corresponding
source.

## Dependencies and models

Cargo and npm dependencies retain their own copyright and license notices.
Release source archives include locked Cargo dependency sources; generated
`notices/dependencies/` accompanies native application archives. This generated
material belongs in release artifacts rather than the source repository.
When a crate's published package omits license files, the version-specific
files in `notices/dependency-supplements/` retain the original texts from its
published source revision. Each supplement records that origin in `SOURCE.md`;
the vendored crate and its Cargo checksums remain unchanged.

NNUE weights are separate data, subject to [models/LICENSE-NNUE](models/LICENSE-NNUE).
Neither GPL nor MIT grants additional rights to those weights. See
[models/README.md](models/README.md) for model origin, digest, and setup.
