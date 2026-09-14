//! `proveno-compile` — Lua source to verified bytecode JSON.
//!
//! The same job as `proveno-compiler` in the proveno-core repository, kept here
//! so the proving pipelines in this repository do not have to shell into
//! another repository's checkout. It is a thin wrapper over the core library:
//! parse, compile, verify, serialize.

use std::{
    env,
    fs::{self, File},
    process,
};

use proveno::{bytecode, compiler, parser};

fn main() {
    let mut args = env::args().skip(1);
    let Some(path) = args.next() else {
        eprintln!("usage: proveno-compile <source.lua> [output.json]");
        process::exit(2);
    };
    let out_path = args.next().unwrap_or_else(|| "compiled.json".to_owned());

    let source = fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("error reading {path}: {e}");
        process::exit(1);
    });

    let ast = parser::parse(&source).unwrap_or_else(|e| {
        eprintln!("parse error: {e:?}");
        process::exit(1);
    });
    let program = compiler::compile(&ast).unwrap_or_else(|e| {
        eprintln!("compile error: {e:?}");
        process::exit(1);
    });
    if let Err(e) = bytecode::verify(&program) {
        eprintln!("verification error: {e:?}");
        process::exit(1);
    }

    let out_file = File::create(&out_path).unwrap_or_else(|e| {
        eprintln!("error creating {out_path}: {e}");
        process::exit(1);
    });
    serde_json::to_writer(out_file, &program).unwrap_or_else(|e| {
        eprintln!("error writing {out_path}: {e}");
        process::exit(1);
    });
    println!("File written - {out_path}");
}
