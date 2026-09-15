# proveno-zk

The proving layer for [proveno](https://github.com/inertialabsxyz/proveno):
execution policy, public-input commitments, two proving backends, and the
on-chain verifier.

Depends on [proveno-core](https://github.com/inertialabsxyz/proveno-core) by
git tag. Core is the runtime; this repository is everything that turns a run
into a proof a verifier will accept.

## Backends

| Backend | Commitments | Use |
|---|---|---|
| Noir / UltraHonk | Poseidon2 | the canonical path; small proofs, on-chain verification |
| OpenVM | SHA-256 | a RISC-V zkVM alternative; `app` and aggregated `stark` levels |

The two are **not interchangeable**. `program_hash`,
`tool_responses_hash` and `attestation_hash` are backend-specific, and a
verifier must recompute with the scheme the prover used.

## Pipeline

```bash
# 1. Lua -> verified bytecode
cargo run -p proveno-witness --bin proveno-compile -- source.lua compiled.json

# 2. Dry run with a live host -> oracle tape + public inputs
cargo run -p proveno-witness -- compiled.json dry_result.json --policy <spec>

# 3a. Noir proof
cargo run -p proveno-noir -- compiled.json dry_result.json --prove

# 3b. or the OpenVM backend
cargo run -p proveno-openvm-host -- compiled.json dry_result.json --prove [--stark]
```

Or in one shot:

```bash
make prove-openvm                # examples/simple.lua, compile -> prove -> verify
./prove-openvm.sh myscript.lua --policy policies/test-policy.json
```

## Execution policy

A policy constrains what an execution may do — domain allowlist, HTTP method
restriction, call and payload limits, response schemas — and its hash is a
public input. The guest parses the enforceable fields out of the same bytes it
hashes, so the policy enforced and the policy committed are one document by
construction.

A run that violates the policy cannot be replayed, so no proof of it exists.
This holds even if the host skipped enforcement during the dry run.

What a proof still cannot tell you is whether a response genuinely came from the
domain the program requested. That is provenance, and it is delegated to an
attestation provider; the circuit binds attestation blobs, it does not
authenticate them.

## Development

```bash
make check       # the gate: fmt, clippy -D warnings, tests
make test-prove  # pre-PR: the full nargo + bb pipeline, with timings
make help
```

`make test-prove` needs `nargo` and `bb` on `PATH`; `make build-openvm` and
`make prove-openvm` need `cargo-openvm`.

See [CLAUDE.md](CLAUDE.md) for the layout, the commitment schemes, the policy
enforcement model, and what a proof does not bind.

## Licence

See [LICENSE](LICENSE).
