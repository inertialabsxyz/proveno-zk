//! Rebuild the OpenVM proving inputs from a stored proveno-gateway trace.
//!
//! A gateway trace store is a directory holding `traces/<trace_id>.json` and
//! `programs/<program_hash>.lua`. The trace records the VM limits the run used
//! and one `ToolCallRecord` per call, which is everything a proof needs: the
//! program is recompiled from the stored source, the tape is rebuilt from the
//! records, and the run is replayed offline, the same reconstruction
//! proveno-gateway's `replay` performs.
//!
//! The trace signature is **not** checked here: the signing key belongs to the
//! gateway, not to the prover. Run `proveno-gateway replay` for that.
//!
//! Two policy documents, two commitments: the header's `gateway_policy_hash`
//! names the gateway's rules file, which has no `OraclePolicy` form. The proof's
//! `policy_hash` is the [`OraclePolicy`] built by [`policy_for`] from the VM
//! limits in the header.

use std::{fs, path::Path};

use proveno::{
    OracleTape, TapeHost, ToolCallRecord, Vm, VmConfig,
    bytecode::verify,
    compiler::{compile, program_hash::compute_program_hash_sha256, proto::CompiledProgram},
    host::canonicalize::canonical_serialize,
    parser::parse,
    types::value::LuaValue,
};
use proveno_zk::{
    policy::{OraclePolicy, TlsRequirement},
    zkvm::{dry_run_result::DryRunResult, guest_input::GuestInput},
};
use serde::Deserialize;

/// The parts of a gateway trace the reconstruction reads. Everything else in
/// the file (decisions, provenance labels, the signature) is ignored.
#[derive(Deserialize)]
struct Trace {
    header: Header,
    entries: Vec<Entry>,
    footer: Footer,
}

#[derive(Deserialize)]
struct Header {
    program_hash: String,
    vm_config: VmLimits,
}

/// proveno-gateway's `VmSettings`: `VmConfig` without `record_trace`.
#[derive(Deserialize)]
struct VmLimits {
    gas_limit: u64,
    memory_limit_bytes: u64,
    max_call_depth: usize,
    max_tool_calls: usize,
    max_tool_bytes_in: usize,
    max_tool_bytes_out: usize,
    max_output_bytes: usize,
}

#[derive(Deserialize)]
struct Entry {
    record: ToolCallRecord,
}

#[derive(Deserialize)]
struct Footer {
    output: Option<String>,
    gas_used: u64,
    memory_used: u64,
}

/// Everything `proveno-openvm-host` needs to prove one gateway trace.
pub struct Reconstruction {
    pub program: CompiledProgram,
    /// Its `public_inputs` use the SHA-256 scheme, as the OpenVM guest does,
    /// and are exactly what the guest will reveal a digest of.
    pub dry_run: DryRunResult,
    pub vm_config: VmConfig,
    pub policy: OraclePolicy,
}

/// The `OraclePolicy` a gateway run was held to, from its recorded VM limits.
///
/// Gateway tools are not HTTP tools, so the domain and method lists stay empty
/// (unrestricted) and the gateway's own rules are not represented here. The
/// response cap is `max_tool_bytes_in`, the limit on bytes flowing into the VM.
/// Gateway calls carry `unsigned` provenance, so attestation is not required.
pub fn policy_for(config: &VmConfig) -> OraclePolicy {
    OraclePolicy {
        allowed_domains: Vec::new(),
        allowed_http_methods: Vec::new(),
        max_tool_calls: config.max_tool_calls,
        max_payload_bytes_per_call: config.max_tool_bytes_in,
        tls_requirement: TlsRequirement::UnattestedPermitted,
        required_output_schema: None,
        schema_versions: Default::default(),
    }
}

/// Store keys are hex digests and UUIDs; refusing anything else keeps a key
/// from naming a path outside the store.
fn store_path(dir: &Path, key: &str, ext: &str) -> Result<std::path::PathBuf, String> {
    let valid = !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b) || b == b'-');
    if !valid {
        return Err(format!("invalid store key '{key}'"));
    }
    Ok(dir.join(format!("{key}.{ext}")))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Reconstruct the proving inputs for `trace_id` from the store at `store`.
///
/// Fails when the stored program does not compile to the header's
/// `program_hash`, when the replay departs from the recording, or when the run
/// did not complete: the guest proves only an execution that returns.
pub fn reconstruct(store: &Path, trace_id: &str) -> Result<Reconstruction, String> {
    let trace_path = store_path(&store.join("traces"), trace_id, "json")?;
    let trace: Trace = serde_json::from_slice(
        &fs::read(&trace_path).map_err(|e| format!("reading {}: {e}", trace_path.display()))?,
    )
    .map_err(|e| format!("parsing {}: {e}", trace_path.display()))?;
    let header = &trace.header;

    let program_path = store_path(&store.join("programs"), &header.program_hash, "lua")?;
    let source = fs::read_to_string(&program_path)
        .map_err(|e| format!("reading {}: {e}", program_path.display()))?;
    let block = parse(&source).map_err(|e| {
        format!(
            "program {}: line {}: {}",
            header.program_hash,
            e.span().line,
            e.message()
        )
    })?;
    let program =
        compile(&block).map_err(|e| format!("program {}: {}", header.program_hash, e.message()))?;
    verify(&program)
        .map_err(|e| format!("program {}: verification: {e:?}", header.program_hash))?;
    let program_hash = hex(&compute_program_hash_sha256(&program.prototypes));
    if program_hash != header.program_hash {
        return Err(format!(
            "program_hash: header has {}, stored source compiles to {program_hash}",
            header.program_hash
        ));
    }

    let l = &header.vm_config;
    let vm_config = VmConfig {
        gas_limit: l.gas_limit,
        memory_limit_bytes: l.memory_limit_bytes,
        max_call_depth: l.max_call_depth,
        max_tool_calls: l.max_tool_calls,
        max_tool_bytes_in: l.max_tool_bytes_in,
        max_tool_bytes_out: l.max_tool_bytes_out,
        max_output_bytes: l.max_output_bytes,
        record_trace: false,
    };

    let records: Vec<ToolCallRecord> = trace.entries.into_iter().map(|e| e.record).collect();
    let tape = OracleTape::from_records(&records);

    // Strict replay, so a program that makes different calls from the ones
    // recorded is caught here rather than proved over the wrong tape.
    let mut vm = Vm::new(vm_config.clone(), TapeHost::strict(tape.clone()));
    let output = vm
        .execute(&program, LuaValue::Nil)
        .map_err(|e| format!("trace {trace_id}: replay did not complete: {e:?}"))?;
    let host = vm.host();
    let mut mismatches = Vec::new();
    if let Some(d) = host.divergence() {
        mismatches.push(format!("replay diverged at call {}", d.seq));
    }
    if host.remaining() > 0 {
        mismatches.push(format!(
            "{} recorded calls left unconsumed",
            host.remaining()
        ));
    }
    let replayed = canonical_serialize(&output.return_value)
        .ok()
        .and_then(|b| String::from_utf8(b).ok());
    if replayed != trace.footer.output {
        mismatches.push(format!(
            "output: recorded {:?}, replayed {replayed:?}",
            trace.footer.output
        ));
    }
    if output.gas_used != trace.footer.gas_used {
        mismatches.push(format!(
            "gas_used: recorded {}, replayed {}",
            trace.footer.gas_used, output.gas_used
        ));
    }
    if output.memory_used != trace.footer.memory_used {
        mismatches.push(format!(
            "memory_used: recorded {}, replayed {}",
            trace.footer.memory_used, output.memory_used
        ));
    }
    if !mismatches.is_empty() {
        return Err(format!(
            "trace {trace_id}: replay does not match the recording: {}",
            mismatches.join("; ")
        ));
    }

    // The public inputs come from the guest's own replay function, so they are
    // what the proof will reveal, policy enforcement included.
    let policy = policy_for(&vm_config);
    let guest_input = GuestInput::new(
        program,
        LuaValue::Nil,
        tape.clone(),
        vm_config.clone(),
        Vec::new(),
    )
    .with_policy_canonical(policy.canonical_bytes());
    let (_, public_inputs) = guest_input
        .replay_public_inputs()
        .map_err(|e| format!("trace {trace_id}: guest replay failed: {e:?}"))?;

    let program = guest_input.program;

    let attestations = records.iter().map(|r| r.attestation.clone()).collect();
    Ok(Reconstruction {
        program,
        dry_run: DryRunResult {
            output,
            oracle_tape: tape,
            attestations,
            public_inputs,
        },
        vm_config,
        policy,
    })
}
