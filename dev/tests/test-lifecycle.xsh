use targets as target_policy

pure context_source(root: Path, host_os: target_policy.HostOs = target_policy.Linux) -> Str {
  let host = if host_os == target_policy.Linux { "target_policy.Linux" } else { "target_policy.Darwin" }
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

test test_build_failure_stops_at_the_cargo_boundary { |ctx|
  let root = test.temp_dir(ctx, name: "build-failure")?
  let tools = fp"{root}/tools"
  tools.mkdir()
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
    },
  )?
  assert cargo_marker.exists()?
}

test test_check_lint_runs_only_the_read_only_performance_gate { |ctx|
  let root = test.temp_dir(ctx, name: "check-lint")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let cargo_marker = fp"{root}/cargo-argv"
  write_fake_tool(
    fp"{tools}/cargo",
    xsh,
    f"""p"{cargo_marker}".write(args.join("|"))?
exit 23""",
  )
  test.expect(
    ctx,
    f"""
use build
use context
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_lint(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["StageError.Failed"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  assert cargo_marker.read_text()? == "test|--release|--target|x86_64-unknown-linux-musl|-p|xsht|--test|integration|lint_performance::|--|--test-threads=1|--nocapture"
}

test test_check_compat_runs_every_ratchet_and_stops_at_the_first_failure { |ctx|
  let root = test.temp_dir(ctx, name: "check-compat")?
  let tools = fp"{root}/tools"
  tools.mkdir()
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let log = fp"{root}/python-argv"
  write_fake_tool(
    fp"{tools}/python3",
    xsh,
    f"""let previous = if p"{log}".exists()? {{ p"{log}".read_text()? }} else {{ "" }}
p"{log}".write(previous + args.join("|") + "\\n")?
if "parity.py" in args.join("|") {{
  exit 7
}}""",
  )
  test.expect(
    ctx,
    f"""
use build
use context
use targets as target_policy

let ctx: context.Context = {context_source(root)}
match build.check_compat(ctx) {{
  Ok(_) => exit 1
  Err(error) => print ${{error.message}}
}}
""",
    status: 0,
    stdout: ["StageError.Failed"],
    args: [],
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
  )?
  assert log.read_text()? == "dev/compat/check_ignored_options.py\ndev/compat/check_kernel_reads.py\ndev/compat/check_exclusions.py\ndev/compat/parity.py|--check\n"
}

test test_lint_fix_rebuilds_the_debug_xsh_binary { |ctx|
  let root = test.temp_dir(ctx, name: "lint-build-xsh")?
  let tools = fp"{root}/tools"
  tools.mkdir()
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
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let docker_marker = fp"{root}/docker-marker"
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

let ctx: context.Context = {context_source(root)}
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
    env: {PATH: tools, XSH_MODULE_PATH: fp"{repository}/dev"},
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
  assert "build" in docker_marker.read_text()?
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
    run make --no-print-directory -n build
  }
  assert process.run(default)?.exited_with(0)
  assert output.read_text()? == """cargo dev build
"""

  let lint_check = process.command {
    cwd = root
    stdout = output
    run make --no-print-directory -n check
  }
  assert process.run(lint_check)?.exited_with(0)
  assert output.read_text()? == """cargo dev check lint
"""

  let override = process.command {
    cwd = root
    stdout = output
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
