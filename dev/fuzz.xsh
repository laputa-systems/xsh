use fuzz_paths

cli main(duration: UInt) {
  let messages = run.text cargo build --release -p xsh-fuzz --bin xsh-fuzz --message-format=json-render-diagnostics
  let binary = fuzz_paths.built_binary(messages)?
  let out = fuzz_paths.failure_dir(binary)?
  run $binary all --duration $duration --out $out
}
