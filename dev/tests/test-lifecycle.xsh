use targets as target_policy

pure context_source(root: Path, host_os: target_policy.HostOs = target_policy.Linux) -> Str {
  let host = if host_os == .Linux { "target_policy.Linux" } else { "target_policy.Darwin" }
  f"""{{
    root: p"{root}",
    target_dir: p"{root}/target",
    coverage_dir: p"{root}/target/cov",
    artifact_dir: p"{root}/dist",
    host_os: {host},
    host_arch: target_policy.X86_64,
    target: target_policy.resolve("x86_64-unknown-linux-musl")?,
    profile: "dist",
    darwin_deployment_target: "26.0",
  }}"""
}

pure darwin_context_source(root: Path) -> Str {
  f"""{{
    root: p"{root}",
    target_dir: p"{root}/target",
    coverage_dir: p"{root}/target/cov",
    artifact_dir: p"{root}/dist",
    host_os: target_policy.Darwin,
    host_arch: target_policy.Aarch64,
    target: target_policy.resolve("aarch64-apple-darwin")?,
    profile: "dist",
    darwin_deployment_target: "26.0",
  }}"""
}

proc write_fake_tool(tool_path: Path, xsh: Path, body: Str) [fs, error] {
  tool_path.write(
    f"""#!{xsh}
{body}
""",
    mode: 0o755,
  )
}

proc write_fake_flock(tools: Path) [fs, error] {
  fp"{tools}/flock".write(
    "#!/bin/sh\nexec /usr/bin/flock \"$@\"\n",
    mode: 0o755,
  )
}

proc write_fake_lint_tools(
  tools: Path,
  xsh: Path,
  log: Path,
  library: Path,
  lint_exit: Int,
  ready_before_exit: Int,
  ready_after_exit: Int,
) [fs, error] {
  tools.mkdir()
  write_fake_flock(tools)
  library.write("")
  write_fake_tool(
    fp"{tools}/rustc",
    xsh,
    "print \"host: x86_64-unknown-linux-musl\"",
  )
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
let step = if "--no-run" in args {{ "build" }} else {{ "test" }}
let preload = env.get_or("LD_PRELOAD", "")?
log.write(previous + f"cargo-{{step}}|{{args.join("|")}}|preload={{preload}}\\n")?
if "--no-run" in args {{ exit 0 }}
exit {lint_exit}""",
  )
  let guard_seen = fp"{tools}/ready-seen"
  write_fake_tool(
    fp"{tools}/perf-gate-ready",
    xsh,
    f"""let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
let already_seen = p"{guard_seen}".exists()?
let label = if already_seen {{ "ready-after" }} else {{ "ready-before" }}
let status = if already_seen {{ {ready_after_exit} }} else {{ {ready_before_exit} }}
log.write(previous + f"{{label}}|{{args.join("|")}}\\n")?
p"{guard_seen}".write("seen")?
exit status""",
  )
}

test test_build_failure_stops_at_the_cargo_boundary { |ctx|
  let root = test.temp_dir(ctx, name: "build-failure")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  write_fake_flock(tools)
  let cargo_marker = fp"{root}/cargo-marker"
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""p"{cargo_marker}".write("cargo")?
exit 23""",
  )
  let inherited_path = env.get_or("PATH", "")?
  test.expect(
    ctx,
    f"""
use build
use context
use targets as target_policy

let ctx: context.Context = {context_source(root, host_os: target_policy.Darwin)}
match build.build(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["[build target=x86_64-unknown-linux-musl] cargo build", "StageError.Failed"],
    args: [],
    env: {
      PATH: f"{tools}:{inherited_path}",
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: "",
    },
  )?
  assert cargo_marker.exists()?
}

test test_check_lint_guards_the_exact_budget_test_before_later_checks { |ctx|
  let root = test.temp_dir(ctx, name: "check-lint-order")?
  let tools = fp"{root}/tools"
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let log = fp"{root}/commands.log"
  let library = fp"{root}/libjemalloc.so.2"
  write_fake_lint_tools(tools, xsh, log, library, 0, 0, 0)
  let release = fp"{root}/target/x86_64-unknown-linux-musl/release"
  release.mkdir()
  write_fake_tool(
    fp"{release}/xsht",
    xsh,
    f"""let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
log.write(previous + "core-check\\n")?
exit 7""",
  )
  test.expect(
    ctx,
    f"""
use build
use context
use stage as stages
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_lint(ctx) {{
  Ok(_) => exit 1
  Err(stages.StageError.Failed {{stage, ..}}) => print ${{stage}}
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["check-core-contracts"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: library,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    f"cargo-build|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--no-run|preload={library}",
    "ready-before|",
    "cargo-test|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--|--exact|--test-threads=1|--nocapture|preload=",
    "ready-after|-1",
    "core-check",
  ]
}

test test_check_lint_skips_the_test_when_the_host_is_busy { |ctx|
  let root = test.temp_dir(ctx, name: "check-lint-busy")?
  let tools = fp"{root}/tools"
  let repository = fs.cwd()?
  let log = fp"{root}/commands.log"
  let library = fp"{root}/libjemalloc.so.2"
  write_fake_lint_tools(tools, ctx.xsh_bin, log, library, 0, 1, 0)
  test.expect(
    ctx,
    f"""
use build
use context
use stage as stages
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_lint(ctx) {{
  Ok(_) => exit 1
  Err(stages.StageError.Failed {{stage, ..}}) => print ${{stage}}
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["check-lint-ready"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: library,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    f"cargo-build|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--no-run|preload={library}",
    "ready-before|",
  ]
}

test test_check_lint_keeps_the_test_error_when_the_postcheck_fails { |ctx|
  let root = test.temp_dir(ctx, name: "check-lint-postcheck")?
  let tools = fp"{root}/tools"
  let repository = fs.cwd()?
  let log = fp"{root}/commands.log"
  let library = fp"{root}/libjemalloc.so.2"
  write_fake_lint_tools(tools, ctx.xsh_bin, log, library, 23, 0, 1)
  let result = test.expect(
    ctx,
    f"""
use build
use context
use stage as stages
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_lint(ctx) {{
  Ok(_) => exit 1
  Err(stages.StageError.Failed {{stage, ..}}) => print ${{stage}}
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["check-lint"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: library,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    f"cargo-build|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--no-run|preload={library}",
    "ready-before|",
    "cargo-test|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--|--exact|--test-threads=1|--nocapture|preload=",
    "ready-after|-1",
  ]
  assert "post-readiness check failed after lint test failure" in result.stderr
}

test test_check_lint_rejects_a_successful_test_when_the_postcheck_fails { |ctx|
  let root = test.temp_dir(ctx, name: "check-lint-postcheck-busy")?
  let tools = fp"{root}/tools"
  let repository = fs.cwd()?
  let log = fp"{root}/commands.log"
  let library = fp"{root}/libjemalloc.so.2"
  write_fake_lint_tools(tools, ctx.xsh_bin, log, library, 0, 0, 1)
  let release = fp"{root}/target/x86_64-unknown-linux-musl/release"
  release.mkdir()
  write_fake_tool(
    fp"{release}/xsht",
    ctx.xsh_bin,
    f"""let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
log.write(previous + "core-check\\n")?
exit 0""",
  )
  test.expect(
    ctx,
    f"""
use build
use context
use stage as stages
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_lint(ctx) {{
  Ok(_) => exit 1
  Err(stages.StageError.Failed {{stage, ..}}) => print ${{stage}}
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["check-lint-ready-after"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      XSH_DEV_BUILD_PRELOAD: library,
      CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_RUSTFLAGS: "",
    },
  )?
  assert log.read_lines()? == [
    f"cargo-build|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--no-run|preload={library}",
    "ready-before|",
    "cargo-test|test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::repository_lint_is_clean_within_wall_budget|--|--exact|--test-threads=1|--nocapture|preload=",
    "ready-after|-1",
  ]
}

test test_check_core_contracts_propagates_native_runner_failure { |ctx|
  let root = test.temp_dir(ctx, name: "check-core-contracts")?
  let release = fp"{root}/target/x86_64-unknown-linux-musl/release"
  release.mkdir()
  let repository = fs.cwd()?
  let log = fp"{root}/xsht-argv"
  write_fake_tool(
    fp"{release}/xsht",
    ctx.xsh_bin,
    f"""p"{log}".write(args.join("|"))?
exit 7""",
  )
  test.expect(
    ctx,
    f"""
use build
use context
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_core_contracts(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["StageError.Failed"],
    args: [],
    env: {XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  assert log.read_text()? == "test|-j|1|dev/tests/test-repository-contracts.xsh"
}

test test_lint_fix_rebuilds_the_debug_xsh_binary { |ctx|
  let root = test.temp_dir(ctx, name: "lint-build-xsh")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  write_fake_flock(tools)
  let target = fp"{root}/target"
  target.mkdir()
  let debug = fp"{target}/debug"
  debug.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let xsh_marker = fp"{root}/xsh-build-marker"
  let xsht = fp"{debug}/xsht"

  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""if "--bin" in args and "xsh" in args {{
  p"{xsh_marker}".write(args.join("|"))?
}}
""",
  )
  write_fake_tool(xsht, xsh, "")

  let result = test.run_script(
    ctx,
    f"""
use build
use context
use targets as target_policy

let ctx: context.Context = {context_source(root)}
build.lint_fix(ctx)?
""",
    [],
    {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  assert result.success, f"""{result.stdout}
{result.stderr}"""
  assert xsh_marker.exists()?, "lint --fix did not rebuild the debug xsh binary"
  let xsh_arguments = xsh_marker.read_text()?
  let xsh_diagnostic = xsh_marker.read_text()?
  assert "--bin|xsh" in xsh_arguments, xsh_diagnostic
}

test test_docker_container_failure_runs_target_ownership_cleanup { |ctx|
  let root = test.temp_dir(ctx, name: "container-cleanup")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  write_fake_flock(tools)
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let cargo_marker = fp"{root}/cargo-marker"
  let cleanup_marker = fp"{root}/cleanup-marker"
  write_fake_tool(fp"{tools}/git", xsh, "let configured = true")
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""p"{cargo_marker}".write("cargo")?
exit 23""",
  )
  write_fake_tool(fp"{tools}/chown", xsh, f"""p"{cleanup_marker}".write("cleanup")?""")
  test.expect(
    ctx,
    f"""
use context
use internal
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match internal.linux_ci_test(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["[test-products target=x86_64-unknown-linux-musl] cargo build --locked --profile verification --target x86_64-unknown-linux-musl", "StageError.Failed"],
    args: [],
    env: {
      PATH: tools,
      XSH_MODULE_PATH: fp"{repository}/dev",
      DIST_PROFILE: "verification",
      HOST_UID: "501",
      HOST_GID: "20",
    },
  )?
  assert cargo_marker.exists()?
  assert cleanup_marker.exists()?
}

test test_docker_image_and_container_failures_are_staged { |ctx|
  let root = test.temp_dir(ctx, name: "docker-failures")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let cargo_home = fp"{root}/cargo"
  let cargo_bin = fp"{cargo_home}/bin"
  cargo_home.mkdir()
  cargo_bin.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let docker_marker = fp"{root}/docker-marker"
  write_fake_tool(fp"{cargo_bin}/kache", xsh, "exit 0")
  write_fake_tool(
    fp"{tools}/docker",
    xsh,
    f"""p"{docker_marker}".write(args.join("|"))?
if "run" in args {{
  exit 24
}}
""",
  )
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(repository)}
match docker.run_internal(ctx, "dist", false, []) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: [
      "[docker-image-build target=x86_64-unknown-linux-musl] docker build",
      "[docker-dist target=x86_64-unknown-linux-musl] docker run",
      "StageError.Failed",
    ],
    args: [],
    env: {
      PATH: tools,
      CARGO_HOME: cargo_home,
      XSH_MODULE_PATH: fp"{repository}/dev",
    },
  )?
  assert "run" in docker_marker.read_text()?
}

test test_docker_image_build_failure_prevents_the_container_stage { |ctx|
  let root = test.temp_dir(ctx, name: "docker-image-failure")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let docker_marker = fp"{root}/docker-marker"
  write_fake_tool(
    fp"{tools}/docker",
    xsh,
    f"""p"{docker_marker}".write(args.join("|"))?
exit 24""",
  )
  let result = test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match docker.run_internal(ctx, "dist", false, []) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["[docker-image-build target=x86_64-unknown-linux-musl] docker build"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  assert "[docker-dist" not in result.stdout, result.stdout
  assert docker_marker.read_text()?.starts_with("build|--provenance=false|"), docker_marker.read_text()?
}

test test_trace_keeps_process_status_for_a_failed_child { |ctx|
  let traced = test.run_xsht_trace(
    ctx,
    """
run.status false
""",
    ["--trace", "--raw"],
  )?
  assert traced.success, traced.stderr
  assert "kind=run.start" in traced.stderr
  assert "kind=run.end" in traced.stderr
  assert "status={kind:exit success:false code:1}" in traced.stderr
}

test test_make_facade_only_delegates_to_the_development_entrypoint {
  let facade = p"Makefile".read_text()?

  for command in [
    "$(XSH_DEV) dev/main.xsh --",
    "cargo dev",
    "$(DEV) check lint",
    "$(DEV) docs",
    "$(DEV) docs check",
    "$(DEV) lint --fix",
    "$(DEV) test linux --ci",
    "$(DEV) coverage --backend docker",
    "$(DEV) bench --syscalls",
    "$(DEV) dist --docker always",
  ] {
    assert command in facade, facade
  }

  for forbidden in ["cargo build", "cargo test", "sh -c", "bash -c", "docker run"] {
    assert forbidden not in facade, facade
  }
}

test test_make_facade_bootstraps_by_default_and_honors_an_explicit_binary { |ctx|
  match process.which("make") {
    Err(_) => {
      test.skip("requires make executable")
      return
    }
    Ok(_) => {}
  }

  let root = test.temp_dir(ctx, name: "make-facade")?
  fp"{root}/Makefile".write(p"Makefile".read_text()?)
  let stale_dir = fp"{root}/target/debug"
  stale_dir.mkdir()
  fp"{stale_dir}/xsh".write("stale binary")
  let output = fp"{root}/make-output.txt"

  let default = process.command {
    cwd = root
    stdout = output
    env = {XSH_DEV: ""}
    run make --no-print-directory -n build
  }
  assert process.run(default)?.exited_with(0)
  assert output.read_text()? == """cargo dev build
"""

  let lint_check = process.command {
    cwd = root
    stdout = output
    env = {XSH_DEV: ""}
    run make --no-print-directory -n check
  }
  assert process.run(lint_check)?.exited_with(0)
  assert output.read_text()? == """cargo dev check lint
"""

  let override = process.command {
    cwd = root
    stdout = output
    env = {XSH_DEV: ""}
    run make --no-print-directory -n build "XSH_DEV=/missing/xsh"
  }
  assert process.run(override)?.exited_with(0)
  assert output.read_text()? == """/missing/xsh dev/main.xsh -- build
"""
}

test test_codesign_failure_stops_darwin_installation { |ctx|
  let root = test.temp_dir(ctx, name: "codesign-failure")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  write_fake_flock(tools)
  let release_dir = fp"{root}/target/aarch64-apple-darwin/release"
  release_dir.mkdir()
  fp"{release_dir}/xsh".write("binary", mode: 0o755)
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let codesign_marker = fp"{root}/codesign-marker"
  let cargo_marker = fp"{root}/cargo-argv"
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""p"{cargo_marker}".write(args.join("|"))?
""",
  )
  write_fake_tool(
    fp"{tools}/codesign",
    xsh,
    f"""p"{codesign_marker}".write("codesign")?
exit 25""",
  )
  test.expect(
    ctx,
    f"""
use context
use install
use targets as target_policy

let ctx: context.Context = {darwin_context_source(root)}
match install.darwin(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["[install-darwin-codesign target=aarch64-apple-darwin] codesign", "StageError.Failed"],
    args: [],
    env: {
      PATH: tools,
      HOME: fp"{root}/home",
      XSH_MODULE_PATH: fp"{repository}/dev",
    },
  )?
  assert codesign_marker.exists()?
  let cargo_arguments = cargo_marker.read_text()?
  let cargo_diagnostic = cargo_marker.read_text()?
  assert "build-std" in cargo_arguments, cargo_diagnostic
}

test test_darwin_install_rejects_linux_target_before_building { |ctx|
  let root = test.temp_dir(ctx, name: "darwin-rejects-linux")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let cargo_marker = fp"{root}/cargo-marker"
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""p"{cargo_marker}".write("cargo")?
""",
  )
  test.expect(
    ctx,
    f"""
use context
use install
use targets as target_policy

let ctx: context.Context = {{
  root: p"{root}",
  target_dir: p"{root}/target",
  coverage_dir: p"{root}/target/cov",
  artifact_dir: p"{root}/dist",
  host_os: target_policy.Darwin,
  host_arch: target_policy.Aarch64,
  target: target_policy.resolve("x86_64-unknown-linux-musl")?,
  profile: "dist",
  darwin_deployment_target: "26.0",
}}
match install.darwin(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["StageError.Failed"],
    args: [],
    env: {
      PATH: tools,
      HOME: fp"{root}/home",
      XSH_MODULE_PATH: fp"{repository}/dev",
    },
  )?
  assert ! cargo_marker.exists()?, "darwin install with a Linux target must fail before cargo"
}

test test_linux_install_requires_native_musl_target { |ctx|
  let root = test.temp_dir(ctx, name: "linux-requires-native")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let cargo_marker = fp"{root}/cargo-marker"
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""p"{cargo_marker}".write("cargo")?
""",
  )
  test.expect(
    ctx,
    f"""
use context
use install
use targets as target_policy

let ctx: context.Context = {{
  root: p"{root}",
  target_dir: p"{root}/target",
  coverage_dir: p"{root}/target/cov",
  artifact_dir: p"{root}/dist",
  host_os: target_policy.Linux,
  host_arch: target_policy.X86_64,
  target: target_policy.resolve("aarch64-unknown-linux-musl")?,
  profile: "dist",
  darwin_deployment_target: "26.0",
}}
match install.linux_install(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["StageError.Failed"],
    args: [],
    env: {
      PATH: tools,
      HOME: fp"{root}/home",
      XSH_MODULE_PATH: fp"{repository}/dev",
    },
  )?
  assert ! cargo_marker.exists()?, "cross-arch Linux install must fail before cargo"

  test.expect(
    ctx,
    f"""
use context
use install
use targets as target_policy

let ctx: context.Context = {{
  root: p"{root}",
  target_dir: p"{root}/target",
  coverage_dir: p"{root}/target/cov",
  artifact_dir: p"{root}/dist",
  host_os: target_policy.Linux,
  host_arch: target_policy.X86_64,
  target: target_policy.resolve("aarch64-apple-darwin")?,
  profile: "dist",
  darwin_deployment_target: "26.0",
}}
match install.linux_install(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["StageError.Failed"],
    args: [],
    env: {
      PATH: tools,
      HOME: fp"{root}/home",
      XSH_MODULE_PATH: fp"{repository}/dev",
    },
  )?
}

test test_a_gate_build_resolves_the_image_from_the_build_not_the_tag { |ctx|
  let root = test.temp_dir(ctx, name: "docker-iidfile")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let built = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  let retagged = "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  write_fake_tool(
    fp"{tools}/docker",
    ctx.xsh_bin,
    f"""if args[0] == "build" {{
  var index = 0
  while index < args.len() {{
    if args[index] == "--iidfile" {{ Path(args[index + 1]).write("{built}")? }}
    index += 1
  }}
  exit 0
}}
if args[0] == "image" {{
  print "{retagged}"
  exit 0
}}
exit 1
""",
  )
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(repository)}
print docker.resolve_image(ctx)?
""",
    status: 0,
    stdout: [built],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev", DOCKER_PLATFORM: "linux/amd64"},
  )?
}

test test_an_owned_container_that_outlives_its_client_is_removed_by_id_and_verified { |ctx|
  let root = test.temp_dir(ctx, name: "docker-cidfile")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let log = fp"{root}/calls"
  let cidfile = fp"{root}/container.cid"
  cidfile.write(id)
  write_container_stub(fp"{tools}/docker", ctx.xsh_bin, root, log)?
  let source = f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(repository)}
match docker.remove_owned_container(ctx, p"{cidfile}") {{
  Ok(_) => print "removed"
  Err(error) => print ${{error.message}}
}}
"""
  test.expect(ctx, source, status: 0, stdout: ["removed"], args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"})?
  assert f"rm|-f|{id}" in log.read_lines()?
  fp"{root}/gone-{id}".remove()?
  log.remove()?
  test.expect(ctx, source, status: 0, stdout: ["ContainerError.Survived"], args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev", KEEP: "1"})?
}

proc write_container_stub(tool: Path, xsh: Path, root: Path, log: Path) [fs, error] {
  write_fake_tool(
    tool,
    xsh,
    f"""let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
log.write(previous + args.join("|") + "\\n")?
let marker = Path("{root}/gone-" + args[args.len() - 1])
if args[0] == "container" and args[1] == "inspect" {{
  if env.get_or("UNREACHABLE", "")? != "" {{
    eprint "Cannot connect to the Docker daemon at unix:///var/run/docker.sock. Is the docker daemon running?"
    exit 1
  }}
  if marker.exists()? {{
    eprint "Error response from daemon: No such container: " + args[args.len() - 1]
    exit 1
  }}
  exit 0
}}
if args[0] == "rm" {{
  marker.write("x")? when env.get_or("KEEP", "")? == ""
  if env.get_or("RM_RACES", "")? != "" {{
    eprint "Error response from daemon: removal of container is already in progress"
    exit 1
  }}
}}
exit 0
""",
  )
}

test test_every_container_a_run_started_is_removed_by_id_from_its_registry { |ctx|
  let root = test.temp_dir(ctx, name: "docker-registry")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let first = "1ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let second = "2ccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let log = fp"{root}/calls"
  write_container_stub(fp"{tools}/docker", ctx.xsh_bin, root, log)?
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(root)}
let registry = docker.owned_container_dir(ctx)?
registry.mkdir(parents: true)?
fp"{{registry}}/one.cid".write("{first}")?
fp"{{registry}}/two.cid".write("{second}")?
docker.remove_owned_containers(ctx)?
assert ! registry.exists()?
print "removed"
""",
    status: 0,
    stdout: ["removed"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  let calls = log.read_lines()?
  assert f"rm|-f|{first}" in calls
  assert f"rm|-f|{second}" in calls
}

test test_a_cancelled_gate_removes_its_containers_before_the_runtime_kills_the_client { |ctx|
  let root = test.temp_dir(ctx, name: "docker-cancel")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "eccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let log = fp"{root}/calls"
  let ready = fp"{root}/ready"
  write_container_stub(fp"{tools}/docker", ctx.xsh_bin, root, log)?
  let script = fp"{root}/run.xsh"
  script.write(f"""
use context
use docker
use gates
use targets as target_policy

let ctx: context.Context = {context_source(root)}

on TERM --pre-cancel=20s [fs, process, env, time, error, io] {{
  gates.cancel_owned_work(ctx, "TERM")
  exit 130
}}

proc main() [fs, process, env, time, error, io] {{
  let registry = docker.owned_container_dir(ctx)?
  registry.mkdir(parents: true)?
  fp"{{registry}}/gate-lane-1.cid".write("{id}")?
  p"{ready}".write("x")?
  run sleep 30
}}
""")
  let search_path = f"{tools}:{env.get_or("PATH", "")?}"
  let module_path = fp"{repository}/dev".display()
  let xsh = ctx.xsh_bin
  env PATH=$search_path XSH_MODULE_PATH=$module_path {
    let child = spawn run $xsh $script ?
    var waited = 0
    while ! ready.exists()? and waited < 100 {
      time.sleep(100ms)
      waited += 1
    }
    assert ready.exists()?
    process.kill(child.pid, signal: "TERM")
    let status = wait child?
    assert status.exited_with(130)
  }
  assert f"rm|-f|{id}" in log.read_lines()?
  assert fp"{root}/gone-{id}".exists()?
}

test test_a_cancelled_gate_gives_its_adapters_the_time_their_cleanup_needs { |ctx|
  let root = test.temp_dir(ctx, name: "adapter-cancel")?
  let repository = fs.cwd()?
  let ready = fp"{root}/ready"
  let cleaned = fp"{root}/cleaned"
  let adapter = fp"{root}/adapter.sh"
  adapter.write(f"""#!/bin/sh
trap 'sleep 1; echo cleaned > {cleaned}; exit 143' TERM
: > {ready}
sleep 30 &
wait
""", mode: 0o755)
  let script = fp"{root}/run.xsh"
  script.write(f"""
use context
use gates
use targets as target_policy

let ctx: context.Context = {context_source(repository)}

on TERM --pre-cancel=20s [fs, process, env, time, error, io] {{
  gates.cancel_owned_work(ctx, "TERM")
  exit 130
}}

proc main() [fs, process, env, time, error, io] {{
  run {adapter}
}}
""")
  let search_path = env.get_or("PATH", "")?
  let module_path = fp"{repository}/dev".display()
  let xsh = ctx.xsh_bin
  env PATH=$search_path XSH_MODULE_PATH=$module_path {
    let child = spawn run $xsh $script ?
    var waited = 0
    while ! ready.exists()? and waited < 100 {
      time.sleep(100ms)
      waited += 1
    }
    assert ready.exists()?
    process.kill(child.pid, signal: "TERM")
    let status = wait child?
    assert status.exited_with(130)
  }
  assert cleaned.exists()?
}

test test_container_removal_is_verified_against_the_daemon_not_assumed { |ctx|
  let root = test.temp_dir(ctx, name: "docker-verify")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "dccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let cidfile = fp"{root}/container.cid"
  cidfile.write(id)
  write_container_stub(fp"{tools}/docker", ctx.xsh_bin, root, fp"{root}/calls")?
  let source = f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(repository)}
match docker.remove_owned_container(ctx, p"{cidfile}") {{
  Ok(_) => print "removed"
  Err(error) => print ${{error.message}}
}}
"""
  test.expect(ctx, source, status: 0, stdout: ["ContainerError.Unverified"], args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev", UNREACHABLE: "1"})?
  test.expect(ctx, source, status: 0, stdout: ["removed"], args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev", RM_RACES: "1"})?
}

test test_a_gate_that_fails_before_launching_leaves_no_container_registry { |ctx|
  let root = test.temp_dir(ctx, name: "docker-no-launch")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  write_fake_tool(fp"{tools}/docker", ctx.xsh_bin, "exit 1", )?
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(root)}
let registry = docker.owned_container_dir(ctx)?
match docker.run_internal_status(ctx, "gate-lane", false, []) {{
  Ok(_) => print "launched"
  Err(_) => print f"registry={{registry.exists()?}}"
}}
""",
    status: 0,
    stdout: ["registry=false"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev", XSH_TEST_IMAGE_BUILD: "0"},
  )?
}

test test_a_cancelled_gate_removes_a_container_only_after_its_children_had_their_grace { |ctx|
  let root = test.temp_dir(ctx, name: "docker-order")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "accccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let log = fp"{root}/calls"
  let ready = fp"{root}/ready"
  let cleaned = fp"{root}/cleaned"
  write_fake_tool(
    fp"{tools}/docker",
    ctx.xsh_bin,
    f"""let log = p"{log}"
let previous = if log.exists()? {{ log.read_text()? }} else {{ "" }}
let state = if p"{cleaned}".exists()? {{ "after-cleanup" }} else {{ "before-cleanup" }}
log.write(previous + args.join("|") + "|" + state + "\\n")?
let marker = Path("{root}/gone-" + args[args.len() - 1])
if args[0] == "container" and args[1] == "inspect" {{
  if marker.exists()? {{
    eprint "Error response from daemon: No such container: " + args[args.len() - 1]
    exit 1
  }}
  exit 0
}}
if args[0] == "rm" {{ marker.write("x")? }}
exit 0
""",
  )
  fp"{tools}/chown".write(f"#!/bin/sh\nstate=before-cleanup; [ -e {cleaned} ] && state=after-cleanup\nprintf 'chown|%s\\n' \"$state\" >> {log}\n", mode: 0o755)
  fp"{tools}/find".write("#!/bin/sh\nexit 0\n", mode: 0o755)
  let adapter = fp"{root}/adapter.sh"
  adapter.write(f"""#!/bin/sh
trap 'sleep 1; echo cleaned > {cleaned}; exit 143' TERM
: > {ready}
sleep 30 &
wait
""", mode: 0o755)
  let script = fp"{root}/run.xsh"
  script.write(f"""
use context
use docker
use gates
use targets as target_policy

let ctx: context.Context = {context_source(root)}

on TERM --pre-cancel=40s [fs, process, env, time, error, io] {{
  gates.cancel_owned_work(ctx, "TERM", 20000)
  exit 130
}}

proc main() [fs, process, env, time, error, io] {{
  let registry = docker.owned_container_dir(ctx)?
  registry.mkdir(parents: true)?
  fp"{{registry}}/gate-lane-1.cid".write("{id}")?
  run {adapter}
}}
""")
  let search_path = f"{tools}:{env.get_or("PATH", "")?}"
  let module_path = fp"{repository}/dev".display()
  let xsh = ctx.xsh_bin
  env PATH=$search_path XSH_MODULE_PATH=$module_path HOST_UID=1000 HOST_GID=1000 {
    let child = spawn run $xsh $script ?
    var waited = 0
    while ! ready.exists()? and waited < 100 {
      time.sleep(100ms)
      waited += 1
    }
    assert ready.exists()?
    process.kill(child.pid, signal: "TERM")
    let status = wait child?
    assert status.exited_with(130)
  }
  assert cleaned.exists()?
  let calls = log.read_lines()?
  assert f"rm|-f|{id}|after-cleanup" in calls
  assert f"rm|-f|{id}|before-cleanup" not in calls
  assert "chown|after-cleanup" in calls
  var chown_at = -1
  var rm_at = -1
  var position = 0
  for line in calls {
    chown_at = position when line.starts_with("chown|")
    rm_at = position when line.starts_with("rm|")
    position += 1
  }
  assert chown_at >= 0 and rm_at > chown_at
}

test test_an_unverifiable_removal_is_reported_with_the_id_and_the_manual_command { |ctx|
  let root = test.temp_dir(ctx, name: "docker-unverified")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "bccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  write_container_stub(fp"{tools}/docker", ctx.xsh_bin, root, fp"{root}/calls")?
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(root)}
let registry = docker.owned_container_dir(ctx)?
registry.mkdir(parents: true)?
fp"{{registry}}/gate-lane-1.cid".write("{id}")?
match docker.remove_owned_containers(ctx) {{
  Ok(_) => print "removed"
  Err(_) => print "unverified"
}}
""",
    status: 0,
    stdout: [f"container {id} removal is unverified", f"docker rm -f {id}", "unverified"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev", UNREACHABLE: "1"},
  )?
}

test test_a_hung_daemon_cannot_hold_the_cleanup_past_its_query_timeout { |ctx|
  let root = test.temp_dir(ctx, name: "docker-hung")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "dccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  fp"{root}/container.cid".write(id)
  write_fake_tool(fp"{tools}/docker", ctx.xsh_bin, "time.sleep(60s)?")?
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(repository)}
match docker.remove_owned_container(ctx, p"{root}/container.cid", query_timeout_ms: 500) {{
  Ok(_) => print "removed"
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["ContainerError.Unverified"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
}

test test_a_registry_of_a_dead_run_is_swept_and_a_live_one_is_kept { |ctx|
  let root = test.temp_dir(ctx, name: "docker-sweep")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let id = "fccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
  let log = fp"{root}/calls"
  write_container_stub(fp"{tools}/docker", ctx.xsh_bin, root, log)?
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(root)}
let tmp = fp"{{ctx.root}}/.work/tmp"
let host = p"/proc/sys/kernel/hostname".read_text()?.trim()
let dead = fp"{{tmp}}/gate-containers-{{host}}-999999-1"
dead.mkdir(parents: true)?
fp"{{dead}}/gate-lane-1.cid".write("{id}")?
let foreign = fp"{{tmp}}/gate-containers-another-host-999999-1"
foreign.mkdir(parents: true)?
fp"{{foreign}}/gate-lane-1.cid".write("{id}")?
let own = docker.owned_container_dir(ctx)?
own.mkdir(parents: true)?
docker.sweep_stale_containers(ctx)?
print f"dead={{dead.exists()?}} own={{own.exists()?}} foreign={{foreign.exists()?}}"
""",
    status: 0,
    stdout: ["dead=false own=true foreign=true"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  assert f"rm|-f|{id}" in log.read_lines()?
}

test test_an_empty_cidfile_means_no_container_was_created_yet { |ctx|
  let root = test.temp_dir(ctx, name: "docker-empty-cid")?
  let repository = fs.cwd()?
  fp"{root}/container.cid".write("")
  test.expect(
    ctx,
    f"""
use context
use docker
use targets as target_policy

let ctx: context.Context = {context_source(repository)}
match docker.remove_owned_container(ctx, p"{root}/container.cid") {{
  Ok(_) => print "nothing to remove"
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["nothing to remove"],
    args: [],
    env: {PATH: "/nonexistent", XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
}
