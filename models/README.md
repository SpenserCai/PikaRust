# NNUE model

`pikafish.nnue` is the fixed network used by PikaRust's numerical tests and pinned
Pikafish reference. Git LFS stores the actual weights; a normal Git checkout may
contain only a pointer until LFS downloads the object.

| Property | Value |
| --- | --- |
| Model | `pikafish.nnue` |
| SHA-256 | `92b5fb5d333800654377a93ad8d28d0b4c8b34fb9a3d1cdaafd6ecdfb3459bb2` |
| Reference commit | `76239d0b06720bfa4588989fd4ac7573e9dbf887` |
| Original source | [Pikafish Networks](https://github.com/official-pikafish/Networks) |
| Terms | [LICENSE-NNUE](LICENSE-NNUE) |

The authoritative tooling pins are in
[`scripts/reference.lock`](../scripts/reference.lock). Retrieve and verify the
network from the repository root:

```sh
git lfs pull --include="models/pikafish.nnue"
scripts/setup-pikafish.sh --verify-model
```

The original `master-net` release URL is a rolling download. It is not a reliable
way to recover this exact historical network. Do not substitute its current
contents when this repository's LFS object is unavailable.

## Loading and distribution

Use `Engine::with_nnue_file(path)` when embedding the library, or
`pikarust --eval-file PATH` for the native UCI application. Those explicit paths
surface load failures instead of silently selecting material evaluation.

Weights are not included in release application archives or crate packages.
Distribute or obtain them separately under their own terms. In particular,
`LICENSE-NNUE` prohibits commercial use without permission; the repository's
source-code license does not replace these conditions.

## Updating the reference

1. Select an explicit official Pikafish commit and compatible model. Record their
   provenance and verify the applicable source and weight terms.
2. Update the LFS model and both pins in `scripts/reference.lock` together.
3. Update the digest and reference commit in this document.
4. Rebuild the reference and derive numerical fixtures from that independently
   verified build. Review every changed expectation.
5. Run core, scalar, model-backed, and reference E2E checks. Review search and
   strength changes separately; numerical agreement does not establish parity.

Keep temporary upstream checkouts and diagnostic exports outside tracked source.
