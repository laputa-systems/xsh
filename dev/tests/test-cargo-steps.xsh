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

proc write_fake_toolchain(tools: Path, log: Path) [fs, error] {
  tools.mkdir()
  fp"{tools}/rustc".write(
    r"""#!/bin/sh
printf '%s\n' 'host: x86_64-unknown-linux-musl'
""",
    mode: 0o755,
  )
  fp"{tools}/cargo".write(
    f"""#!/bin/sh
set -eu
release_flags=
case " $* " in
  *' --release '*)
    release_flags=" rustflags=${{CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS-<unset>}} linker=${{CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER-<unset>}}"
    ;;
esac
printf '%s%s preload=%s\\n' "$*" "$release_flags" "${{LD_PRELOAD-}}" >> "{log}"
case " $* " in
  *' test '* )
    case " $* " in
      *' --no-run '*) exit 0 ;;
    esac
    runner=${{CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER-}}
    if [ -n "$runner" ]; then
      exec "$runner" "${{CARGO_TEST_HARNESS:-/bin/true}}" --cargo-runner-argument -- --test-threads=1
    fi
    ;;
esac
""",
    mode: 0o755,
  )
}

proc install_cargo_test_runner(root: Path, repository: Path) [fs, error] {
  let dev = fp"{root}/dev"
  dev.mkdir()
  fp"{dev}/cargo_test_runner.sh".write(
    fp"{repository}/dev/cargo_test_runner.sh".read_text()?,
    mode: 0o755,
  )
}

test test_linux_test_requires_one_harness_and_matching_target { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("requires Linux cargo target selection")
    return
  }

  let root = test.temp_dir(ctx, name: "cargo-test-targets")?
  let repository = fs.cwd()?
  let source = f"""
use cargo_steps
use stage as stages

let root = p"{root}"
for argv in [
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "--doc"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "--all-targets"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--no-run", "--test", "integration"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "--lib"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--lib", "--test", "integration"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration", "--test", "ambient_fs_policy"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "-p", "xsht", "--test", "integration"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "--target", "aarch64-unknown-linux-musl", "-p", "xsh", "--test", "integration"],
  ["cargo", "test", "--target", "aarch64-unknown-linux-musl", "-p", "xsh", "--test", "integration"],
  ["cargo", "test", "-p", "xsh", "--test", "integration", "--target"],
  ["cargo", "test", "--target=x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--package=xsh-fuzz", "--lib"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "-pxsh-fuzz", "--lib"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration", "--test=ambient_fs_policy"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "--target=aarch64-unknown-linux-musl", "-p", "xsh", "--test", "integration"],
] {{
  let spec = stages.command("test-rust", "x86_64-unknown-linux-musl", "cargo", argv, root, {{}})
  match cargo_steps.run_test(spec) {{
    Err(error) => assert error.message == "StageError.Failed"
    Ok(_) => test.fail(f"invalid target selection was accepted: {{argv.join(" ")}}")
  }}
}}

for argv in [
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "--package", "xsh", "--test", "integration"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--lib"],
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration", "--", "--target", "x86_64-unknown-linux-musl"],
] {{
  let spec = stages.command("test-rust", "x86_64-unknown-linux-musl", "cargo", argv, root, {{}})
  match cargo_steps.run_test(spec) {{
    Err(error) => assert error.message == "StageError.MissingTool"
    Ok(_) => test.fail("a valid target selection passed without the Cargo test runner")
  }}
}}

let reserved = stages.command(
  "test-rust",
  "x86_64-unknown-linux-musl",
  "cargo",
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration"],
  root,
  {{CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER: "/custom/runner"}},
)
match cargo_steps.run_test(reserved) {{
  Err(error) => assert error.message == "StageError.Failed"
  Ok(_) => test.fail("a caller-provided Cargo runner was silently overwritten")
}}
"""
  test.expect(
    ctx,
    source,
    status: 0,
    env: {
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
    },
  )?
}

test test_cargo_test_runner_hands_off_lock_and_preserves_cargo_environment { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("requires Linux /proc and flock")
    return
  }

  let root = test.temp_dir(ctx, name: "cargo-test-runner")?
  let repository = fs.cwd()?
  let runner = fp"{repository}/dev/cargo_test_runner.sh"
  let lock_file = fp"{root}/cargo.lock"
  let harness = fp"{root}/harness.sh"
  let log = fp"{root}/harness.log"
  let error_output = fp"{root}/runner.stderr"
  lock_file.write("")
  harness.write(
    r"""#!/bin/sh
set -eu
printf '%s\n' "${LD_PRELOAD-<unset>}" "${CARGO_MANIFEST_DIR-<unset>}" "${CARGO_PKG_NAME-<unset>}" "${CARGO_BIN_EXE_xsh-<unset>}" "$#" "$@" "${XSH_CARGO_TEST_LOCK_FILE-<unset>}" "${CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER-<unset>}" "${CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER-<unset>}" > "$CARGO_TEST_RUNNER_LOG"
""",
    mode: 0o755,
  )
  let command = process.command_argv(
    "/usr/bin/flock",
    [
      "/usr/bin/flock", "-x", lock_file, runner, harness,
      "--exact", "tests::preserves_argv", "--test-threads=1",
    ],
    env: {
      XSH_CARGO_TEST_LOCK_FILE: lock_file,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER: runner,
      CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_RUNNER: runner,
      CARGO_MANIFEST_DIR: "/workspace/xsh",
      CARGO_PKG_NAME: "xsh",
      CARGO_BIN_EXE_xsh: "/workspace/xsh/target/xsh",
      CARGO_TEST_RUNNER_LOG: log,
      LD_PRELOAD: "/compiler-preload.so:/inherited-runtime-preload.so",
    },
    stderr: error_output,
  )
  assert process.run(command)?.exited_with(0)
  assert log.read_lines()? == [
    "/compiler-preload.so:/inherited-runtime-preload.so", "/workspace/xsh", "xsh", "/workspace/xsh/target/xsh", "3",
    "--exact", "tests::preserves_argv", "--test-threads=1", "<unset>", "<unset>", "<unset>",
  ]

  let missing_lock = process.command_argv(
    runner,
    [runner, harness],
    env: {
      XSH_CARGO_TEST_LOCK_FILE: lock_file,
      CARGO_TEST_RUNNER_LOG: log,
    },
    stderr: error_output,
  )
  assert process.run(missing_lock)?.exited_with(125)
  assert "expected one inherited Cargo lock descriptor, found 0" in error_output.read_text()?

  let ambiguous_lock = process.command_argv(
    "/bin/sh",
    [
      "/bin/sh", "-c", "exec 3<\"$1\"; exec 4<\"$1\"; shift; exec \"$@\"",
      "sh", lock_file, runner, harness,
    ],
    env: {
      XSH_CARGO_TEST_LOCK_FILE: lock_file,
      CARGO_TEST_RUNNER_LOG: log,
    },
    stderr: error_output,
  )
  assert process.run(ambiguous_lock)?.exited_with(125)
  assert "expected one inherited Cargo lock descriptor, found 2" in error_output.read_text()?
}

test test_build_steps_run_cargo_under_the_preload_and_test_runs_do_not { |ctx|
  let root = test.temp_dir(ctx, name: "preload-steps")?
  let tools = fp"{root}/tools"
  let log = fp"{root}/cargo.log"
  let library = fp"{root}/libjemalloc.so.2"
  let inherited_linker = env.get_or("CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER", "<unset>")?
  let inherited_path = env.get_or("PATH", "")?
  library.write("")
  write_fake_toolchain(tools, log)
  let repository = fs.cwd()?
  install_cargo_test_runner(root, repository)
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
  var expected_stdout = [
    "[docs-build target=x86_64-unknown-linux-musl] cargo build --release",
  ]
  for _ in ["integration", "ambient_fs_policy", "symbol_plateau", "core_compat_boundaries", "core_compat_extra"] {
    expected_stdout += [
      "[test-rust-build target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
      "[test-rust target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
    ]
  }
  expected_stdout += [
    "[test-debug-assertions-build target=x86_64-unknown-linux-musl] cargo test --locked",
    "[test-debug-assertions target=x86_64-unknown-linux-musl] cargo test --locked",
    "[test-xsht-build target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
    "[test-xsht target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
    "[test-soundness-build target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
    "[test-soundness target=x86_64-unknown-linux-musl] cargo test --locked --profile verification",
  ]
  test.expect(
    ctx,
    script,
    status: 0,
    stdout: expected_stdout,
    args: [],
    env: {
      PATH: f"{tools}:{inherited_path}",
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: library,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  let preloaded = f"preload={library}"
  var expected_log = [
    f"build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol linker={inherited_linker} {preloaded}",
    f"build --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht -p xshi --bin xshi {preloaded}",
  ]
  for target in ["integration", "ambient_fs_policy", "symbol_plateau", "core_compat_boundaries", "core_compat_extra"] {
    expected_log += [
      f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test {target} --no-run {preloaded}",
      f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test {target} -- --skip split:: --skip runtime::coverage:: preload=",
    ]
  }
  expected_log += [
    f"test --locked --target x86_64-unknown-linux-musl -p xsh --lib --no-run {preloaded}",
    "test --locked --target x86_64-unknown-linux-musl -p xsh --lib preload=",
    f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsht --test integration --no-run {preloaded}",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsht --test integration -- --skip syntax::formatter_is_idempotent_on_laputa_corpus --skip lint_format_invariance::formatting_preserves_lints_on_laputa_corpus --skip examples::example_corpus_is_formatted --skip examples::example_corpus_lints_without_warnings --skip lint_performance::repository_lint_is_clean_within_wall_budget preload=",
    f"test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh-fuzz --test soundness --no-run {preloaded}",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh-fuzz --test soundness preload=",
  ]
  assert log.read_lines()? == expected_log

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
      PATH: f"{tools}:{inherited_path}",
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
      PATH: f"{tools}:{inherited_path}",
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    f"build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol linker={inherited_linker} preload=",
  ]

  log.remove(missing_ok: false)
  test.expect(
    ctx,
    script,
    status: 0,
    args: [],
    env: {
      PATH: f"{tools}:{inherited_path}",
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    f"build --release --target x86_64-unknown-linux-musl -p xsh --bin xsh -p xsht --bin xsht rustflags=-C target-feature=+crt-static -C link-arg=--defsym=__isoc23_sscanf=sscanf -C link-arg=--defsym=__isoc23_strtol=strtol linker={inherited_linker} preload=",
    "build --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --bins -p xsht --bin xsht -p xshi --bin xshi preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test integration -- --skip split:: --skip runtime::coverage:: preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test ambient_fs_policy -- --skip split:: --skip runtime::coverage:: preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test symbol_plateau -- --skip split:: --skip runtime::coverage:: preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test core_compat_boundaries -- --skip split:: --skip runtime::coverage:: preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh --test core_compat_extra -- --skip split:: --skip runtime::coverage:: preload=",
    "test --locked --target x86_64-unknown-linux-musl -p xsh --lib preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsht --test integration -- --skip syntax::formatter_is_idempotent_on_laputa_corpus --skip lint_format_invariance::formatting_preserves_lints_on_laputa_corpus --skip examples::example_corpus_is_formatted --skip examples::example_corpus_lints_without_warnings --skip lint_performance::repository_lint_is_clean_within_wall_budget preload=",
    "test --locked --profile verification --target x86_64-unknown-linux-musl -p xsh-fuzz --test soundness preload=",
  ]
}

test test_cargo_test_unlocks_only_when_the_harness_starts { |ctx|
  guard system.uname()?.sysname == "Linux" else {
    test.skip("requires Linux flock")
    return
  }

  let root = test.temp_dir(ctx, name: "cargo-shared-lock")?
  let tools = fp"{root}/tools"
  let library = fp"{root}/libjemalloc.so.2"
  let repository = fs.cwd()?
  let lock_file = fp"{root}/.work/consolidation/infra/cargo.lock"
  let first_script = fp"{root}/first.xsh"
  let second_script = fp"{root}/second.xsh"
  let xsh = ctx.xsh_bin

  tools.mkdir()
  library.write("")
  fp"{tools}/rustc".write(
    "#!/bin/sh\nprintf '%s\\n' 'host: x86_64-unknown-linux-musl'\n",
    mode: 0o755,
  )
  install_cargo_test_runner(root, repository)
  fp"{tools}/flock".write(
    """#!/bin/sh
if [ "$1" = "-u" ]; then
  exec /usr/bin/flock "$@"
fi
: > "$LOCK_TEST_DIR/$LOCK_TEST_ROLE.flock"
if [ "$LOCK_TEST_ROLE" = "second" ]; then
  if /usr/bin/flock -n "$LOCK_TEST_LOCK" /bin/true; then
    : > "$LOCK_TEST_DIR/second-unlocked"
  else
    : > "$LOCK_TEST_DIR/second-locked"
  fi
fi
exec /usr/bin/flock "$@"
""",
    mode: 0o755,
  )
  fp"{tools}/cargo".write(
    """#!/bin/sh
set -eu
case " $* " in
  *' --no-run '*)
    : > "$LOCK_TEST_DIR/first-build-cargo"
    if /usr/bin/flock -n "$LOCK_TEST_LOCK" /bin/true; then
      : > "$LOCK_TEST_DIR/first-build-unlocked"
    else
      : > "$LOCK_TEST_DIR/first-build-locked"
    fi
    ;;
  *' test '*)
    : > "$LOCK_TEST_DIR/first-runtime-cargo"
    if /usr/bin/flock -n "$LOCK_TEST_LOCK" /bin/true; then
      : > "$LOCK_TEST_DIR/first-runtime-unlocked"
    else
      : > "$LOCK_TEST_DIR/first-runtime-locked"
    fi
    exec "$CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUNNER" "$LOCK_TEST_HARNESS"
    ;;
  *)
    : > "$LOCK_TEST_DIR/second.cargo"
    ;;
esac
""",
    mode: 0o755,
  )
  let harness = fp"{root}/harness.sh"
  harness.write(
    r"""#!/bin/sh
set -eu
: > "$LOCK_TEST_DIR/harness-entered"
while [ ! -e "$LOCK_TEST_DIR/first.gate" ]; do sleep 0.01; done
""",
    mode: 0o755,
  )

  let first_source = f"""
use cargo_steps
use stage as stages

let root = p"{root}"
let spec = stages.command("lock-first", "x86_64-unknown-linux-musl", "cargo",
  ["cargo", "test", "--target", "x86_64-unknown-linux-musl", "-p", "xsh", "--test", "integration", "--target-dir", "target-a"], root,
  {{LOCK_TEST_DIR: "{root}", LOCK_TEST_LOCK: "{lock_file}", LOCK_TEST_ROLE: "first", LOCK_TEST_HARNESS: "{harness}"}})
cargo_steps.run_test(spec)?
"""
  let second_source = f"""
use cargo_steps
use stage as stages

let root = p"{root}"
let spec = stages.command("lock-second", "x86_64-unknown-linux-musl", "cargo", ["cargo", "build", "--target", "x86_64-unknown-linux-musl", "--target-dir", "target-b"], root,
  {{LOCK_TEST_DIR: "{root}", LOCK_TEST_LOCK: "{lock_file}", LOCK_TEST_ROLE: "second"}})
cargo_steps.run_build(spec)?
"""
  first_script.write(f"#!{xsh}\n{first_source}", mode: 0o755)
  second_script.write(f"#!{xsh}\n{second_source}", mode: 0o755)
  let orchestration = fp"{root}/run-concurrent.sh"
  orchestration.write(
    f"""#!/bin/sh
set -eu
export PATH="{tools}:$PATH"
export XSH_MODULE_PATH="{repository}/dev"
export XSH_DEV_BUILD_PRELOAD="{library}"
"{first_script}" &
first_pid=$!
attempt=0
while [ ! -f "{root}/harness-entered" ]; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 3000 ]; then
    : > "{root}/first.gate"
    wait "$first_pid" || true
    exit 1
  fi
  sleep 0.01
done
"{second_script}" &
second_pid=$!
attempt=0
while [ ! -f "{root}/second-locked" ] && [ ! -f "{root}/second-unlocked" ]; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 3000 ]; then
    : > "{root}/first.gate"
    wait "$first_pid" || true
    wait "$second_pid" || true
    exit 1
  fi
  sleep 0.01
done
if [ ! -f "{root}/second-unlocked" ] || [ -f "{root}/second-locked" ]; then
  : > "{root}/first.gate"
  wait "$first_pid" || true
  wait "$second_pid" || true
  exit 1
fi
attempt=0
while [ ! -f "{root}/second.cargo" ]; do
  attempt=$((attempt + 1))
  if [ "$attempt" -ge 3000 ]; then
    : > "{root}/first.gate"
    wait "$first_pid" || true
    wait "$second_pid" || true
    exit 1
  fi
  sleep 0.01
done
: > "{root}/second-entered-while-harness-held"
: > "{root}/first.gate"
wait "$first_pid"
wait "$second_pid"
""",
    mode: 0o755,
  )

  run /bin/sh $orchestration
  assert fp"{root}/first-build-cargo".exists()?
  assert fp"{root}/first-build-locked".exists()?
  assert ! fp"{root}/first-build-unlocked".exists()?
  assert fp"{root}/first-runtime-cargo".exists()?
  assert fp"{root}/first-runtime-locked".exists()?
  assert ! fp"{root}/first-runtime-unlocked".exists()?
  assert fp"{root}/harness-entered".exists()?
  assert fp"{root}/second.cargo".exists()?
  assert fp"{root}/second-unlocked".exists()?
  assert ! fp"{root}/second-locked".exists()?
  assert fp"{root}/second-entered-while-harness-held".exists()?
}
