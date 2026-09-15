# CLAUDE.md

Guidance for Claude Code working in **proveno-zk**.

## What this repository is

Everything about proving a proveno execution: the execution policy, the
public-input commitments, the Noir circuit and its witness writer, the OpenVM
guest and host, and the Solidity verifier and consumer.

| Repository | Scope |
|---|---|
| [proveno-core](https://github.com/inertialabsxyz/proveno-core) | Runtime: parser, compiler, bytecode, vm, host, isa, record/replay |
| **proveno-zk** (here) | Policy, commitments, Noir circuit, OpenVM guest, contracts |
| [proveno-agent](https://github.com/inertialabsxyz/proveno-agent) | LLM orchestrator, demo server, TLS provenance |
| [proveno](https://github.com/inertialabsxyz/proveno) | Umbrella: project overview, architecture, trust model |

Core arrives as a git dependency pinned to a tag, with
`default-features = false`. The [architecture document](https://github.com/inertialabsxyz/proveno/blob/main/docs/architecture.md)
in the umbrella is the tie-breaker when documents disagree.

## Quality Gate

```bash
make check      # lint + test
```

Must pass before every commit.

```bash
make test-prove # pre-PR gate
```

Must pass before opening a PR, and is **not** part of `make check` because it
takes ~20 s and needs `nargo` and `bb` on `PATH`. It drives the full
`nargo execute → bb write_vk → bb prove → bb verify` pipeline and prints
prove/verify wall-time per test, so circuit-size and prove-time regressions are
visible from the output.

Run it for **any** change to the circuit, the witness writer, the oracle tape,
canonical serialization, or the program and trace encoders — including changes
that land in proveno-core, because the circuit recomputes what core produces.
The tampered-witness tests (`tampered_*`) bail out before `bb prove`, so their
times are much lower; only the success-path tests are useful as benchmarks.

## Layout

| Path | Role |
|---|---|
| `src/policy/` | `OraclePolicy` and two enforcing host wrappers |
| `src/zkvm/` | `PublicInputs`, `GuestInput`, `DryRunResult`, commitments |
| `src/noir/` | `encode_program`, the fixed-size bytecode ABI for the circuit |
| `noir/` | The Noir circuit itself (`src/main.nr`, `Nargo.toml`) |
| `proveno-witness/` | Dry-runs a program, produces the oracle tape and public inputs. Also ships `proveno-compile` |
| `proveno-noir/` | Noir witness writer and `nargo`/`bb` prover driver |
| `proveno-openvm/` | OpenVM zkVM guest (RISC-V) |
| `proveno-openvm-host/` | Host driver for the OpenVM backend |
| `verifier/` | `proveno-verifier`, plus the `policy-hash` helper binary |
| `contracts/` | `ProvenoVerifier`, `ProvenoConsumer`, `Bounty`, `Types.sol` |

## Common Commands

```bash
make test-prove                          # Noir prove/verify, with timings
make build-openvm                        # build + transpile the guest (needs cargo-openvm)
make prove-openvm                        # full OpenVM pipeline on examples/simple.lua
make prove-examples                      # every examples/*.lua through the pipeline

# The steps individually:
cargo run -p proveno-witness --bin proveno-compile -- source.lua compiled.json
cargo run -p proveno-witness -- compiled.json dry_result.json [--policy <spec>]
cargo run -p proveno-noir   -- compiled.json dry_result.json --prove
cargo run -p proveno-openvm-host -- compiled.json dry_result.json --prove [--stark]
```

`proveno-compile` mirrors proveno-core's `proveno-compiler` so the pipelines do
not have to shell into another repository's checkout. `proveno-witness` sets
`default-run`, so a bare `cargo run -p proveno-witness` still means the witness.

## Two commitment schemes

`program_hash`, `tool_responses_hash` and `attestation_hash` are
**backend-specific**. `input_hash` (SHA-256) and `output_hash` (keccak256) are
not — they are fixed by their consumers.

| Backend | Scheme | Constructor |
|---|---|---|
| Noir / UltraHonk | Poseidon2 | `compute_public_inputs` |
| zkVM (OpenVM) | SHA-256 | `compute_public_inputs_sha256` |

They are **not interchangeable**: a verifier must recompute with the scheme the
prover used.

**`poseidon` must be off for zkVM guest builds.** It pulls
`bn254_blackbox_solver` → wasmer → cranelift, whose build script hard-panics on
custom RISC-V target triples, and cargo runs it whether or not the code is
linked.

Public inputs, in circuit-declaration order: `num_steps`, `program_hash`,
`return_value`, `tool_responses_hash`, `input_hash`, `output_hash`,
`attestation_hash`, `policy_hash`. The Solidity `PublicInputs` struct in
`contracts/src/Types.sol` mirrors this ordering exactly; reordering breaks
verification.

## Execution policy

An `OraclePolicy` constrains what an execution may do: domain allowlist, HTTP
method restriction, call and payload limits, response schemas. Supply it as a
built-in profile name or a JSON file. `--policy` goes to **both** the dry run,
which enforces it, and the guest input, which commits its hash; the two CLIs
must be given the same value by hand, or `prove-openvm.sh`, which passes it to
both so they cannot drift.

Omitted list fields mean *unrestricted*, not *denied*, so a sparse policy file
is **wider** than a full one.

| Check | Where |
|---|---|
| HTTP method restriction | in-guest, proven |
| Domain allowlist | in-guest, proven |
| `max_tool_calls` | in-guest, proven (rejected calls count, so probing is not free) |
| `max_payload_bytes_per_call` | in-guest, checked against the tape before replay |
| `required_output_schema`, `schema_versions` | **host-side only**, bind-only |

Enforcement is a **host wrapper**, not a `ToolRegistry` feature:
`policy::OraclePolicyHost` host-side (adds schema checks, needs `serde_json`)
and `policy::guest::PolicyEnforcingHost` in-guest (`no_std`). A denial therefore
travels the ordinary tool-failure path — recorded in the transcript, raised as
`VmError::ToolError` — which `pcall` can catch. This is deliberate: enforcing
inside the registry rejected the call before the host was reached, leaving no
trace of the attempt in the artifact.

The guest parses the enforceable fields out of the same bytes it hashes
(`policy::canonical::PolicyView`), so the policy enforced and the policy
committed are one document by construction. Corrupt bytes are refused outright
rather than partially applied, since a partially applied policy is
indistinguishable from a weaker one.

## What a proof does not bind

- **`VmConfig`.** The prover chooses the gas and memory limits, which determine
  whether execution completes or aborts.
- **Provenance.** `attestation_hash` *binds* a per-call attestation blob to the
  response bytes it covers; it does not authenticate it. The circuit binds
  blobs. Keep this honest in prose.
- **Constants, on the Noir path.** See the known gap in
  [proveno-core's CLAUDE.md](https://github.com/inertialabsxyz/proveno-core/blob/main/CLAUDE.md).

## Testing

Determinism invariants are load-bearing for proof soundness. Any change to the
oracle tape, canonical serialization or the encoders must include a test pinning
the property it affects — identical hashes on replay, byte-identical canonical
JSON — and must be followed by `make test-prove`.
