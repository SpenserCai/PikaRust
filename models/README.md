# NNUE Models

This directory contains NNUE model weight files tracked via Git LFS.

## pikafish.nnue

- Source: https://github.com/official-pikafish/Networks/releases/download/master-net/pikafish.nnue
- License: **NNUE-License** (see `LICENSE-NNUE`) — no commercial use without permission
- SHA256: `92b5fb5d333800654377a93ad8d28d0b4c8b34fb9a3d1cdaafd6ecdfb3459bb2`
- Downloaded: 2026-04-26
- Pikafish commit: `76239d0b06720bfa4588989fd4ac7573e9dbf887`

## Why checked in?

The `master-net` release is a rolling release — the file at the same URL gets updated periodically.
PikaRust's NNUE implementation must match a specific model version exactly (layer-by-layer numerical
comparison), so we pin the model file in the repository via Git LFS.

## Updating the model

When updating to a new Pikafish version:

1. Download the new model: `curl -L -o models/pikafish.nnue https://github.com/official-pikafish/Networks/releases/download/master-net/pikafish.nnue`
2. Update the SHA256 and date in this README
3. Update the Pikafish commit in `scripts/setup-pikafish.sh`
4. Re-export all NNUE test fixtures to match the new model
