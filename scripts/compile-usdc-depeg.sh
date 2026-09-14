echo "Compiling following file:"
cat examples/usdc_depeg.lua
cargo run -p proveno-witness --bin proveno-compile -- examples/usdc_depeg.lua /tmp/compiled.json
