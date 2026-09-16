//! `proveno-gateway-trace` — OpenVM proving inputs from a proveno-gateway trace.
//!
//! ```text
//! proveno-gateway-trace <store_dir> <trace_id> [out_dir]
//! proveno-openvm-host <out>/<id>.compiled.json <out>/<id>.dry.json \
//!     --policy <out>/<id>.policy.json --vm-config <out>/<id>.vm_config.json --prove [--stark]
//! ```

use std::{fs, path::Path, process};

use proveno_witness::gateway_trace::reconstruct;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn write_json(path: &Path, value: &impl serde::Serialize) {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|e| {
        eprintln!("error: serializing {}: {e}", path.display());
        process::exit(1);
    });
    fs::write(path, bytes).unwrap_or_else(|e| {
        eprintln!("error: writing {}: {e}", path.display());
        process::exit(1);
    });
    println!("  {}", path.display());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (store, trace_id, out_dir) = match args.as_slice() {
        [store, id] => (store, id, "target/openvm".to_owned()),
        [store, id, out] => (store, id, out.clone()),
        _ => {
            eprintln!("usage: proveno-gateway-trace <store_dir> <trace_id> [out_dir]");
            process::exit(2);
        }
    };

    let r = reconstruct(Path::new(store), trace_id).unwrap_or_else(|e| {
        eprintln!("error: {e}");
        process::exit(1);
    });

    let out = Path::new(&out_dir);
    fs::create_dir_all(out).unwrap_or_else(|e| {
        eprintln!("error: creating {out_dir}: {e}");
        process::exit(1);
    });
    println!("Wrote:");
    let path = |ext: &str| out.join(format!("{trace_id}.{ext}"));
    write_json(&path("compiled.json"), &r.program);
    write_json(&path("dry.json"), &r.dry_run);
    fs::write(path("policy.json"), r.policy.to_json()).unwrap_or_else(|e| {
        eprintln!("error: writing policy: {e}");
        process::exit(1);
    });
    println!("  {}", path("policy.json").display());
    write_json(&path("vm_config.json"), &r.vm_config);

    let pi = &r.dry_run.public_inputs;
    println!("\nPublic inputs (SHA-256 scheme):");
    println!("  program_hash        {}", hex(&pi.program_hash));
    println!("  input_hash          {}", hex(&pi.input_hash));
    println!("  tool_responses_hash {}", hex(&pi.tool_responses_hash));
    println!("  output_hash         {}", hex(&pi.output_hash));
    println!("  attestation_hash    {}", hex(&pi.attestation_hash));
    println!("  policy_hash         {}", hex(&pi.policy_hash));
    println!("  digest              {}", hex(&pi.digest_sha256()));
}
