//! Pins the reconstruction of OpenVM proving inputs from a proveno-gateway
//! trace store.
//!
//! The fixtures are two traces from proveno-gateway's `demo/run.sh`, recorded
//! under proveno-core v0.4.0: step 1, a rebalance with four allowed calls, and
//! step 3, the same program with the transfer refused by the gateway's policy.
//! They hold public Anvil test addresses and no credential.

use std::{fs, path::PathBuf};

use proveno::{OracleTape, TapeEntry, ToolCallRecord, host::canonicalize::canonical_serialize};
use proveno_witness::gateway_trace::{policy_for, reconstruct};

const REBALANCE: &str = "01a0abde-0c46-7481-a087-2d71dd6f7656";
const REFUSED: &str = "01a0abde-1054-74ff-91e8-86c5b95891ea";

fn store() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gateway-store")
}

fn trace_json(id: &str) -> serde_json::Value {
    let path = store().join(format!("traces/{id}.json"));
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The tape commitment computed straight from the trace file, independently of
/// the reconstruction.
fn tool_responses_hash_of(trace: &serde_json::Value) -> String {
    let records: Vec<ToolCallRecord> = trace["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| serde_json::from_value(e["record"].clone()).unwrap())
        .collect();
    OracleTape::from_records(&records).commitment_hash_sha256_hex()
}

/// A writable copy of the fixture store, with `edit` applied to one trace.
fn edited_store(name: &str, id: &str, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gateway_trace_{name}_{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    for sub in ["traces", "programs"] {
        fs::create_dir_all(dir.join(sub)).unwrap();
        for f in fs::read_dir(store().join(sub)).unwrap() {
            let f = f.unwrap();
            fs::copy(f.path(), dir.join(sub).join(f.file_name())).unwrap();
        }
    }
    let mut trace = trace_json(id);
    edit(&mut trace);
    fs::write(
        dir.join(format!("traces/{id}.json")),
        serde_json::to_vec(&trace).unwrap(),
    )
    .unwrap();
    dir
}

#[test]
fn gateway_trace_reconstruction_matches_the_trace() {
    let trace = trace_json(REBALANCE);
    let r = reconstruct(&store(), REBALANCE).unwrap();
    let pi = &r.dry_run.public_inputs;

    assert_eq!(hex(&pi.program_hash), trace["header"]["program_hash"]);
    assert_eq!(hex(&pi.tool_responses_hash), tool_responses_hash_of(&trace));
    // Pinned, so a change to the tape encoding or the record serde shows up.
    assert_eq!(
        hex(&pi.program_hash),
        "842dd6769164063a8d97a2a602e2afe6e899712f38e5e5e899348df58522229e"
    );
    assert_eq!(
        hex(&pi.tool_responses_hash),
        "cbd6b8d14517565c041ec361a9078ce7b7b88ca13ae9e8a3c2b20aa63be41eb2"
    );

    // The proof's policy is the OraclePolicy from the recorded VM limits, not
    // the gateway's rules file.
    let limits = &trace["header"]["vm_config"];
    assert_eq!(r.vm_config.gas_limit, limits["gas_limit"].as_u64().unwrap());
    assert_eq!(
        r.vm_config.max_tool_calls as u64,
        limits["max_tool_calls"].as_u64().unwrap()
    );
    assert_eq!(pi.policy_hash, policy_for(&r.vm_config).policy_hash());
    assert_ne!(hex(&pi.policy_hash), trace["header"]["gateway_policy_hash"]);

    assert_eq!(r.dry_run.oracle_tape.entries.len(), 4);
    assert_eq!(r.dry_run.attestations, vec![Vec::<u8>::new(); 4]);
    let output = canonical_serialize(&r.dry_run.output.return_value).unwrap();
    assert_eq!(
        String::from_utf8(output).unwrap(),
        trace["footer"]["output"].as_str().unwrap()
    );
}

/// A refusal is a failed call on the tape, so the proof binds it.
#[test]
fn gateway_trace_refusal_is_bound_in_the_tape() {
    let trace = trace_json(REFUSED);
    let r = reconstruct(&store(), REFUSED).unwrap();
    let pi = &r.dry_run.public_inputs;

    assert_eq!(hex(&pi.program_hash), trace["header"]["program_hash"]);
    assert_eq!(hex(&pi.tool_responses_hash), tool_responses_hash_of(&trace));
    assert_eq!(
        hex(&pi.tool_responses_hash),
        "0c6b7a26594038afdddcfedf8d56b1bb1b3790c41ee80f5903f9da54db88fe0e"
    );
    assert_eq!(
        r.dry_run.oracle_tape.entries[3],
        TapeEntry::Err("policy: wallet.transfer: amount 20 exceeds amount_max 10".into())
    );

    let allowed = reconstruct(&store(), REBALANCE).unwrap();
    assert_eq!(pi.program_hash, allowed.dry_run.public_inputs.program_hash);
    assert_ne!(
        pi.tool_responses_hash,
        allowed.dry_run.public_inputs.tool_responses_hash
    );
}

#[test]
fn gateway_trace_with_a_mismatched_program_hash_is_refused() {
    let other = "00".repeat(32);
    let dir = edited_store("program_hash", REBALANCE, |t| {
        t["header"]["program_hash"] = other.clone().into();
    });
    fs::copy(
        store()
            .join("programs/842dd6769164063a8d97a2a602e2afe6e899712f38e5e5e899348df58522229e.lua"),
        dir.join(format!("programs/{other}.lua")),
    )
    .unwrap();

    let err = reconstruct(&dir, REBALANCE).err().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(err.contains("program_hash"), "got: {err}");
}

/// A response edited after the fact no longer replays to the recorded output,
/// so it cannot be carried into a proof as this trace.
#[test]
fn gateway_trace_with_an_edited_response_is_refused() {
    let dir = edited_store("response", REBALANCE, |t| {
        t["entries"][1]["record"]["response_canonical"] =
            r#"{"address":"0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266","eth_milli":700}"#.into();
    });

    let err = reconstruct(&dir, REBALANCE).err().unwrap();
    fs::remove_dir_all(&dir).unwrap();
    assert!(err.contains("does not match the recording"), "got: {err}");
}

#[test]
fn gateway_trace_key_outside_the_store_is_refused() {
    let err = reconstruct(&store(), "../../Cargo").err().unwrap();
    assert!(err.contains("invalid store key"), "got: {err}");
}
