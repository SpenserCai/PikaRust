# NNUE models

PikaRust uses the `pikafish.nnue` network from
[Pikafish Networks](https://github.com/official-pikafish/Networks). The model is
pinned for reproducible evaluation and reference tests. Git LFS stores the
weights; download the LFS object before running the engine with NNUE.

| Property | Value |
| --- | --- |
| Model | `pikafish.nnue` |
| SHA-256 | `7d13d73569a9b571ba0eb20cf1596247bc2a42738967e61afef6482b231e900e` |
| Reference commit | `b562d6aeac5401879e973dc53ddb56053f07bb6a` |
| Size | 50,706,378 bytes |
| Original source | [Pikafish Networks master-net](https://github.com/official-pikafish/Networks/releases/download/master-net/pikafish.nnue), release asset `547002829` |
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

## Supported format

PikaRust follows the pinned upstream NNUE architecture: version `0x6A448AFA`
with matching feature-transformer and network hashes. Like the current official
loader, it rejects historical formats instead of selecting a compatibility
implementation. The previous `0x7AF32F20` network is not supported.

The repository tracks one reference model. Other structurally compatible models
can be loaded explicitly, but normal alignment and benchmark gates require the
exact digest above. Historical source revisions preserve the models and engine
implementations used by earlier releases.

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
2. Update the LFS model, `scripts/reference.lock`, and supported decoder
   architecture together. Remove obsolete format paths and document any change
   in model compatibility.
3. Update the digest, reference commit, and download provenance in this document.
4. Rebuild the reference and derive numerical fixtures from that independently
   verified build. Review every changed expectation.
5. Run core, scalar, model-backed, and reference E2E checks. Review search and
   strength changes separately; numerical agreement does not establish parity.
