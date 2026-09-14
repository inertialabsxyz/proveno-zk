//! Proving-layer concerns for proveno.
//!
//! Three modules, all of which the core runtime deliberately does not know
//! about:
//!
//! - [`policy`] — `OraclePolicy`, what counts as an acceptable execution, plus
//!   the two host wrappers that enforce it (host-side and in-guest).
//! - [`zkvm`] — `PublicInputs`, `GuestInput`, `DryRunResult`, and the
//!   commitment helpers in both the Poseidon2 and SHA-256 schemes.
//! - [`noir`] — the fixed-size bytecode ABI the Noir circuit expects.
//!   `MAX_BYTECODE` must match `global MAX_BYTECODE` in `noir/src/main.nr`.
#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

pub mod noir;
pub mod policy;

#[cfg(feature = "zkvm")]
pub mod zkvm;
