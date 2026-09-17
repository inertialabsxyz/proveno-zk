# Proveno Contracts

Solidity contracts for verifying Proveno Noir UltraHonk proofs on-chain and
consuming their outputs.

> **Status.** These contracts verify the Noir backend, which is in development.
> Its circuit constrains a run's control flow, not its computation, so a proof
> these contracts accept does not establish a program's result and does not
> enforce the execution policy: `ProvenoVerifier` checks that `policyHash`
> matches, but the circuit does not check the run against that policy. Do not
> rely on these contracts for results or policy.
> OpenVM is the canonical proving backend and has no on-chain verifier yet.

## Contracts

| Contract | Description |
|---|---|
| `src/HonkVerifier.sol` | UltraHonk verifier generated from the Noir VK by `bb write_solidity_verifier` |
| `src/Types.sol` | `PublicInputs` struct + `PublicInputsLib.pack` (wire-format expansion) |
| `src/ProvenoVerifier.sol` | Enforces `policyHash` and forwards the proof + packed public inputs to `HonkVerifier` |
| `src/ProvenoConsumer.sol` | Example consumer: verifies, then asserts `keccak256(outputPayload) == outputHash`, then decodes a single `int256` result |

## Regenerating `HonkVerifier.sol`

`HonkVerifier.sol` is generated from the Noir circuit's verification key — it
is committed to the repo for reproducibility but is not hand-written. Whenever
`noir/src/main.nr` changes, regenerate the verifier:

```bash
# 1. Compile + execute the circuit (writes noir/target/trace_verifier.{json,gz}):
nargo execute --program-dir noir

# 2. Re-derive the verification key targeting the EVM (keccak random oracle, ZK):
bb write_vk -b noir/target/trace_verifier.json -o noir/target -t evm

# 3. Emit the Solidity verifier:
bb write_solidity_verifier -k noir/target/vk -o contracts/src/HonkVerifier.sol -t evm
```

The generated contract exposes
`verify(bytes calldata proof, bytes32[] calldata publicInputs) external view returns (bool)`.
Public inputs are passed in the wire format produced by `bb prove -t evm`:
194 × 32-byte words, with each `[u8; 32]` field byte-expanded into 32
single-byte entries.

## PublicInputs ordering

`PublicInputs` (in `Types.sol`) holds the eight `pub` parameters of
`noir/src/main.nr` in **declaration order**:

```solidity
struct PublicInputs {
    uint32  numSteps;
    bytes32 programHash;
    int64   returnValue;
    bytes32 toolResponsesHash;
    bytes32 inputHash;
    bytes32 outputHash;
    bytes32 attestationHash;
    bytes32 policyHash;
}
```

`PublicInputsLib.pack` produces the corresponding 194-element `bytes32[]`:

```text
[0]            numSteps
[1 .. 33)      programHash bytes
[33]           returnValue (int64 -> uint64 two's-complement -> bytes32)
[34 .. 66)     toolResponsesHash bytes
[66 .. 98)     inputHash bytes
[98 .. 130)    outputHash bytes
[130 .. 162)   attestationHash bytes
[162 .. 194)   policyHash bytes
```

Reordering fields breaks verification.

### Invariant — keep these in sync

Any change to the `pub` declarations in `noir/src/main.nr` — adding a field,
removing one, reordering, or changing a width — invalidates the on-chain
verifier. After such a change you **must** do all three of the following:

1. Regenerate `HonkVerifier.sol`:
   ```bash
   nargo execute --program-dir noir
   bb write_vk -b noir/target/trace_verifier.json -o noir/target -t evm
   bb write_solidity_verifier -k noir/target/vk -o contracts/src/HonkVerifier.sol -t evm
   ```
2. Update `PublicInputs` in `src/Types.sol` to match the new declaration order
   (field-for-field) and bump `PROVENO_PUBLIC_INPUTS_LENGTH` if the wire-format
   total changed.
3. Update `PublicInputsLib.pack` so each scalar / `[u8; 32]` field is written
   into the `bytes32[]` at the same offset that `bb prove -t evm` produces.

Any Rust code that builds the `bytes32[]` for these contracts, and the test
fixtures under `contracts/test/fixtures/`, must then be regenerated to match.

## Output payload schema

`ProvenoConsumer.consumeResult` expects `outputPayload` to be:

```solidity
abi.encode(int256(return_value))
```

This is the preimage of `output_hash` as computed in `../src/zkvm/commitment.rs`.
The consumer asserts `keccak256(outputPayload) == inputs.outputHash`, then
decodes the payload as a single `int256`:

```solidity
int256 result = abi.decode(outputPayload, (int256));
```

## Usage

```bash
# Build
forge build --root contracts

# Test (loads the committed proof + public inputs from contracts/test/fixtures/)
forge test --root contracts

# Deploy (POLICY_HASH is the 0x-prefixed 32-byte policy commitment the
# verifier should accept):
POLICY_HASH=0x... forge script contracts/script/Deploy.s.sol \
  --rpc-url <RPC_URL> \
  --private-key <PRIVATE_KEY> \
  --broadcast
```
