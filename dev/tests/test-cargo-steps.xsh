use cargo_steps
use stage as stages

const musl_host = "x86_64-unknown-linux-musl"

const context_source = """{
    root: root,
    target_dir: fp"{root}/target",
    coverage_dir: fp"{root}/target/cov",
    artifact_dir: fp"{root}/dist",
    host_os: target_policy.Linux,
    host_arch: target_policy.X86_64,
    target: target_policy.resolve("x86_64-unknown-linux-musl")?,
    profile: "dist",
    darwin_deployment_target: "26.0",
  }"""

test test_host_triple_is_read_from_the_compiler_report {
  let report = "rustc 1.90.0-nightly\nbinary: rustc\nhost: aarch64-unknown-linux-musl\nrelease: 1.90.0-nightly\n"
  assert cargo_steps.host_triple(report)? == "aarch64-unknown-linux-musl"
  match cargo_steps.host_triple("rustc 1.90.0\n") {
    Ok(_) => test.fail("a report without a host line was accepted")
    Err(error) => assert error.message == "ToolchainError.MissingHost"
  }
}

test test_musl_host_with_the_library_preloads_it { |ctx|
  let root = test.temp_dir(ctx, name: "preload-present")?
  let library = fp"{root}/libjemalloc.so.2"
  library.write("")
  assert cargo_steps.preload_for(musl_host, null, library)? == library
  assert cargo_steps.preload_for("aarch64-unknown-linux-musl", null, library)? == library
}

test test_musl_host_without_the_library_preloads_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "preload-missing")?
  assert cargo_steps.preload_for(musl_host, null, fp"{root}/libjemalloc.so.2")? == null
}

test test_non_musl_host_preloads_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "preload-other-host")?
  let library = fp"{root}/libjemalloc.so.2"
  library.write("")
  assert cargo_steps.preload_for("x86_64-unknown-linux-gnu", null, library)? == null
  assert cargo_steps.preload_for("aarch64-apple-darwin", null, library)? == null
  assert cargo_steps.preload_for("aarch64-apple-darwin", library.display(), library)? == null
}

test test_override_replaces_the_library_and_a_missing_one_preloads_nothing { |ctx|
  let root = test.temp_dir(ctx, name: "preload-override")?
  let library = fp"{root}/libjemalloc.so.2"
  let other = fp"{root}/libmimalloc.so"
  library.write("")
  other.write("")
  assert cargo_steps.preload_for(musl_host, other.display(), library)? == other
  assert cargo_steps.preload_for(musl_host, fp"{root}/absent.so".display(), library)? == null
}

test test_empty_override_disables_the_preload { |ctx|
  let root = test.temp_dir(ctx, name: "preload-disabled")?
  let library = fp"{root}/libjemalloc.so.2"
  library.write("")
  assert cargo_steps.preload_for(musl_host, "", library)? == null
}

test test_a_test_step_builds_under_the_preload_and_runs_without_it {
  let library = /usr/lib/libjemalloc.so.2
  let spec = stages.command(
    "test-rust",
    musl_host,
    "cargo",
    ["cargo", "test", "--release", "--test", "integration", "syntax::", "--", "--test-threads=1"],
    /repo,
    {XSH_OS_STRESS_REPEAT: "25"},
  )
  assert cargo_steps.test_steps(spec, null, "") == [cargo_steps.Step(spec:, preload: null)]
  assert cargo_steps.build_step(spec, null, "") == cargo_steps.Step(spec:, preload: null)
  assert cargo_steps.build_step(spec, library, "").preload == "/usr/lib/libjemalloc.so.2"

  let steps = cargo_steps.test_steps(spec, library, "")
  assert steps.len() == 2
  assert steps[0].spec.stage == "test-rust-build"
  assert steps[0].spec.argv == ["cargo", "test", "--release", "--test", "integration", "syntax::", "--no-run"]
  assert steps[0].spec.environment == spec.environment
  assert steps[0].preload == "/usr/lib/libjemalloc.so.2"
  assert steps[1] == cargo_steps.Step(spec:, preload: null)

  assert cargo_steps.test_build_argv(["cargo", "test", "--lib"]) == ["cargo", "test", "--lib", "--no-run"]
  assert cargo_steps.preload_value(library, " /opt/libtrace.so ") == "/usr/lib/libjemalloc.so.2:/opt/libtrace.so"
}

# The fake `rustc` reports a musl host and the fake `cargo` appends one line
# per invocation with its arguments and the preload it inherited.
proc write_fake_toolchain(tools: Path, xsh: Path, log: Path) [fs, error] {
  tools.mkdir()
  fp"{tools}/rustc".write(
    f"""#!{xsh}
print "host: x86_64-unknown-linux-musl"
""",
    mode: 0o755,
  )
  fp"{tools}/cargo".write(
    f"""#!{xsh}
let log = p"{log}"
let earlier = if log.exists()? {{ log.read_text()? }} else {{ "" }}
let release_flags = if "--release" in args {{
  let rustflags = env.get_or("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS", "<unset>")?
  let linker = env.get_or("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", "<unset>")?
  f" rustflags={{rustflags}} linker={{linker}}"
}} else {{ "" }}
log.write(earlier + f"{{args.join(" ")}}{{release_flags}} preload={{env.get_or("LD_PRELOAD", "")?}}\\n")?
""",
    mode: 0o755,
  )
}

test test_build_steps_run_cargo_under_the_preload_and_test_runs_do_not { |ctx|
  let root = test.temp_dir(ctx, name: "preload-steps")?
  let tools = fp"{root}/tools"
  let log = fp"{root}/cargo.log"
  let library = fp"{root}/libjemalloc.so.2"
  library.write("")
  write_fake_toolchain(tools, ctx.xsh_bin, log)
  let repository = fs.cwd()?
  let script = f"""
use context
use docs
use targets as target_policy
use test_workflows

let root = p"{root}"
let ctx: context.Context = {context_source}
docs.build_release(ctx)?
test_workflows.run_commands(test_workflows.rust_plan(ctx, test_workflows.Verification)?)?
"""
  let docs_script = f"""
use context
use docs
use targets as target_policy

let root = p"{root}"
let ctx: context.Context = {context_source}
docs.build_release(ctx)?
"""
  test.expect(
    ctx,
    script,
    status: 0,
    stdout: [
      "[docs-build target=x86_64-unknown-linux-musl] cargo build --release",
      "[test-rust-build target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
      "[test-rust target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
    ],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: library,
    },
  )?
  let preloaded = f"preload={library}"
  assert log.read_lines()? == [
    f"build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol linker=<unset> {preloaded}",
    f"build --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht -p xshi --bin xshi {preloaded}",
    f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test integration --test ambient_fs_policy --test symbol_plateau --test core_compat_boundaries --test core_compat_extra --no-run {preloaded}",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test integration --test ambient_fs_policy --test symbol_plateau --test core_compat_boundaries --test core_compat_extra -- --skip split:: --skip runtime::coverage:: preload=",
    f"test --locked --target x86_64-unknown-linux-musl -p xsh --lib --no-run {preloaded}",
    "test --locked --target x86_64-unknown-linux-musl -p xsh --lib preload=",
    f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsht --test integration --no-run {preloaded}",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsht --test integration -- --skip syntax::formatter_is_idempotent_on_laputa_corpus --skip lint_format_invariance::formatting_preserves_lints_on_laputa_corpus --skip examples::example_corpus_is_formatted --skip examples::example_corpus_lints_without_warnings preload=",
    f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh-fuzz --test soundness --no-run {preloaded}",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh-fuzz --test soundness preload=",
  ]

  # An override replaces repository defaults, so retain static musl linking and
  # both C23 aliases while selecting the Clang/mold wrapper.
  let rustflags = [
    "-Zlocation-detail=none",
    "-Zunstable-options",
    "-Cpanic=immediate-abort",
    "-C target-cpu=x86-64-v3",
    "-C target-feature=+crt-static",
    "-C linker=clang",
    "-C link-arg=-fuse-ld=mold",
    "-C link-arg=-Wl,--defsym=__isoc23_sscanf=sscanf",
    "-C link-arg=-Wl,--defsym=__isoc23_strtol=strtol",
  ].join(" ")
  log.remove(missing_ok: false)
  test.expect(
    ctx,
    docs_script,
    status: 0,
    stdout: ["[docs-build target=x86_64-unknown-linux-musl] cargo build --release"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: rustflags,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER: "clang",
    },
  )?
  assert log.read_lines()? == [
    f"build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags={rustflags} linker=clang preload=",
  ]

  log.remove(missing_ok: false)
  test.expect(
    ctx,
    docs_script,
    status: 0,
    stdout: ["[docs-build target=x86_64-unknown-linux-musl] cargo build --release"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    "build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol linker=<unset> preload=",
  ]

  log.remove(missing_ok: false)
  test.expect(
    ctx,
    script,
    status: 0,
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
    },
  )?
  assert log.read_lines()? == [
    "build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol linker=<unset> preload=",
    "build --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht -p xshi --bin xshi preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test integration --test ambient_fs_policy --test symbol_plateau --test core_compat_boundaries --test core_compat_extra -- --skip split:: --skip runtime::coverage:: preload=",
    "test --locked --target x86_64-unknown-linux-musl -p xsh --lib preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsht --test integration -- --skip syntax::formatter_is_idempotent_on_laputa_corpus --skip lint_format_invariance::formatting_preserves_lints_on_laputa_corpus --skip examples::example_corpus_is_formatted --skip examples::example_corpus_lints_without_warnings preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh-fuzz --test soundness preload=",
  ]
}
