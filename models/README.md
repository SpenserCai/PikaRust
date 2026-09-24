# NNUE model

PikaRust uses the `pikafish.nnue` network from
[Pikafish Networks](https://github.com/official-pikafish/Networks). The model is
pinned for reproducible evaluation and reference tests. Git LFS stores the
weights; download the LFS object before running the engine with NNUE.

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

The upstream `master-net` release is a rolling download. Use the repository's LFS
object to obtain the model matching this digest, rather than substituting the
current release asset.

## Loading and distribution

Use `Engine::with_nnue_file(path)` when embedding the library, or
`pikarust --eval-file PATH` for the native UCI application. Those explicit paths
surface load failures instead of silently selecting material evaluation.

Release application archives and crate packages exclude the weights. The local
web bundle built by `scripts/build-web.sh` includes the selected network and a
copy of `LICENSE-NNUE` so that the application can run from that directory.

## License

The weights retain the original
[Pikafish Networks terms](https://github.com/official-pikafish/Networks#nnue-license),
reproduced in [LICENSE-NNUE](LICENSE-NNUE). They require lawful use and permission
for commercial use, and also apply to weights derived from Pikafish's network.
Keep these terms with any model distribution. Neither the engine's GPL license
nor the independent components' MIT license changes the model's terms or grants
commercial permission. See the
[component licensing policy](../docs/licensing.md) for code licenses.

## Updating the reference

1. Select an explicit official Pikafish commit and compatible model. Record their
   provenance and verify the applicable source and weight terms.
2. Update the LFS model and both pins in `scripts/reference.lock` together.
3. Update the digest and reference commit in this document.
4. Rebuild the reference and derive numerical fixtures from that independently
   verified build. Review every changed expectation.
5. Run core, scalar, model-backed, and reference E2E checks. Review search and
   strength changes separately; numerical agreement does not establish parity.
