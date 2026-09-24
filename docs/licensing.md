# Licensing

PikaRust's engine and the programs that link it are licensed under
`GPL-3.0-or-later`: GNU GPL version 3 or, at the recipient's option, a later
version. The complete GPL text is in [LICENSE](../LICENSE). Independently
implemented components listed below retain the [MIT License](../LICENSE-MIT).
The NNUE model has separate terms.

## Component scope

| Component | License | Scope |
| --- | --- | --- |
| `crates/pikarust-core` | `GPL-3.0-or-later` | Positions, rules, NNUE implementation, search, and engine API |
| `crates/pikarust-app` | `GPL-3.0-or-later` | UCI engine and HTTP/WebSocket server linked to core |
| `crates/pikarust-bench` | `GPL-3.0-or-later` | Engine benchmarks linked to core |
| `e2e_platform` | `GPL-3.0-or-later` | Validation tools, including the referee linked to core |
| `crates/uci-rs` | `MIT` | Independent UCI parser and response types |
| `pikarust-web/bridge` | `MIT` | Independent adapter to an engine subprocess |
| `pikarust-web/frontend` | `MIT` | Independent browser interface |
| `models/pikafish.nnue` | [NNUE-License](../models/LICENSE-NNUE) | Model weights, not the Rust NNUE implementation |

The root GPL license is the repository default outside explicit exceptions.
Third-party dependencies and retained upstream notices keep their own terms.
Package metadata and source notices must agree with this scope; the presence
of `LICENSE-MIT` does not dual-license the engine under MIT.

## Provenance and modifications

The engine is translated and adapted from Pikafish and its Stockfish ancestry.
[NOTICE.md](../NOTICE.md) records that provenance and the reference revision.
The preserved [upstream author list](../notices/upstream/Pikafish-AUTHORS) and
[copyright notice](../notices/upstream/Pikafish-COPYRIGHT) accompany distributions.
The algorithm reference and model digest are pinned in
[`scripts/reference.lock`](../scripts/reference.lock).

Keep existing copyright and license notices when importing or translating
source. Record the upstream repository, revision, affected paths, and the nature
and dates of modifications. Maintain prominent modification notices alongside
the Git history; a language rewrite does not remove upstream licensing terms.
An algorithm reference pin identifies the comparison target, not the complete
copyright history of every file.

## Embedding and separate applications

An application that links `pikarust-core` must satisfy the GPL requirements
applicable to distributing that combined work. Future C, Python, Node.js, or
other bindings that link the core must use `GPL-3.0-or-later` in this project.
Putting such a binding in another crate or changing the programming language
does not create a license exception. The same applies to a WASM build of core.

The MIT protocol crate, bridge, and frontend may be used independently under
their own license. The bridge communicates with the native engine through a
subprocess and UCI messages; it does not link the core library. Bundling it with
the GPL engine does not remove the engine's source-distribution requirements.
A process boundary alone is not an assurance that any integration can remain
closed source: the actual code reuse, communication, and combined distribution
must be considered when changing this architecture.

## Binary and source distribution

Distribute the applicable license texts, copyright notices, and corresponding
source with GPL binaries. The source must match the binary revision and include
the code and scripts needed to build, install, run, and modify it, subject to the
GPL's stated exceptions. Preserve source and license notices for linked
dependencies as well as PikaRust's own files.

PikaRust release archives pair binaries with a source archive for the same
commit, recorded in their build metadata. The source archive includes the
workspace lockfile, build configuration and scripts, vendored Cargo dependency
sources and licenses, and Cargo configuration for using those sources offline.
Toolchains and system libraries remain external prerequisites. A link to the
latest repository branch is not a substitute for the source of a particular
distributed binary. See [releases](releases.md) for artifact checks and commands.

The local web distribution includes the engine, MIT components, license notices,
and matching source. Its builder requires a clean committed checkout and checks
that the source revision remains stable during packaging. Ordinary development
with `cargo` and the frontend dev server does not require a clean worktree;
see [contributing](../CONTRIBUTING.md#local-web-development).

## Model terms

The original [Pikafish Networks terms](../models/LICENSE-NNUE) continue to govern
`pikafish.nnue`, including the requirement to obtain permission for commercial
use. Neither GPL nor MIT grants that permission for the weights. Native release
archives exclude weights; the local web bundle includes the selected model and
its separate notice.
Keep these notices when distributing a bundle. The
[model documentation](../models/README.md) records its source and fixed digest.

## Contributions and historical revisions

Contributions must be compatible with the license of the component they change.
Before adding core code or a core dependency to an MIT component, review and
update its license scope, notices, metadata, and distribution contents together.
Do not remove attribution or rewrite history to conceal where code originated.

Earlier MIT declarations remain part of the recorded project history. They do
not retroactively remove restrictions on upstream-derived material. This policy
does not revoke valid earlier permissions for independently authored code or
certify every historical distribution. Consult the notices and provenance for
the specific revision being used; a new license file does not rewrite that
history.

Registry publishing remains disabled (`publish = false`). Enabling it requires
an explicit packaging and license review. Preparing a release draft, changing
metadata, or passing artifact checks does not itself grant additional rights.
