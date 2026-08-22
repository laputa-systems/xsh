pure context_source(root: Path) -> Str {
  return f"""{
    root: p"${root}",
    target_dir: p"${root}/target",
    coverage_dir: p"${root}/target/cov",
    artifact_dir: p"${root}/dist",
    host_os: "linux",
    host_arch: "x86_64",
    target: {
      triple: "x86_64-unknown-linux-musl",
      os: "linux",
      arch: "x86_64",
      docker_platform: "linux/amd64",
      executable_format: "ELF",
      elf_machine: "Advanced Micro Devices X86-64",
      cpu_rustflags: [],
      cpu_cflags: [],
      static_musl: true,
    },
    profile: "dist",
    darwin_deployment_target: "26.0",
  }"""
}

pure darwin_context_source(root: Path) -> Str {
  return f"""{
    root: p"${root}",
    target_dir: p"${root}/target",
    coverage_dir: p"${root}/target/cov",
    artifact_dir: p"${root}/dist",
    host_os: "darwin",
    host_arch: "aarch64",
    target: {
      triple: "aarch64-apple-darwin",
      os: "darwin",
      arch: "aarch64",
      docker_platform: "linux/arm64",
      executable_format: "Mach-O",
      elf_machine: "",
      cpu_rustflags: [],
      cpu_cflags: [],
      static_musl: false,
    },
    profile: "dist",
    darwin_deployment_target: "26.0",
  }"""
}

proc write_fake_tool(tool_path: Path, xsh: Path, body: Str) [fs, error] {
  tool_path.write(f"""#!${xsh.display()}
${body}
""")?
  fs.chmod(tool_path, 0o755)?
}

proc test_build_failure_stops_at_the_cargo_boundary(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "build-failure")?
  let tools = fp"${root}/tools"
  tools.mkdir()?
  let cargo_marker = fp"${root}/cargo-marker"
  let repository = fs.cwd()?
  let xsh = fp"${repository}/target/debug/xsh"
  write_fake_tool(
    fp"${tools}/cargo",
    xsh,
    f"""p"${cargo_marker.display()}".write("cargo")?
abort(23)""",
  )?
  let result = test.run_script(
    ctx,
    f"""
use build

let ctx = ${context_source(root)}
match build.build(ctx) {
  Ok(_) => abort(1)
  Err(error) => print \${error.message}
}
""",
    [],
    {PATH: tools.display(), XSH_MODULE_PATH: fp"${repository}/dev".display()},
  )?
  test.ok(result.success, result.stderr)?
  test.contains(result.stdout, "[build target=x86_64-unknown-linux-musl] cargo build", result.stdout)?
  test.contains(result.stdout, "ContextError.StageFailed", result.stdout)?
  test.ok(cargo_marker.exists()?)?
}

proc test_docker_container_failure_runs_target_ownership_cleanup(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "container-cleanup")?
  let tools = fp"${root}/tools"
  tools.mkdir()?
  let repository = fs.cwd()?
  let xsh = fp"${repository}/target/debug/xsh"
  let cargo_marker = fp"${root}/cargo-marker"
  let cleanup_marker = fp"${root}/cleanup-marker"
  write_fake_tool(fp"${tools}/git", xsh, "let configured = true")?
  write_fake_tool(
    fp"${tools}/cargo",
    xsh,
    f"""p"${cargo_marker.display()}".write("cargo")?
abort(23)""",
  )?
  write_fake_tool(fp"${tools}/chown", xsh, f"""p"${cleanup_marker.display()}".write("cleanup")?""")?
  let result = test.run_script(
    ctx,
    f"""
use internal

let ctx = ${context_source(root)}
match internal.linux_ci_test(ctx) {
  Ok(_) => abort(1)
  Err(error) => print \${error.message}
}
""",
    [],
    {
      PATH: tools.display(),
      XSH_MODULE_PATH: fp"${repository}/dev".display(),
      HOST_UID: "501",
      HOST_GID: "20",
    },
  )?
  test.ok(result.success, result.stderr)?
  test.contains(result.stdout, "[linux-ci-tests target=x86_64-unknown-linux-musl] cargo test", result.stdout)?
  test.contains(result.stdout, "ContextError.StageFailed", result.stdout)?
  test.ok(cargo_marker.exists()?)?
  test.ok(cleanup_marker.exists()?)?
}

proc test_docker_image_and_container_failures_are_staged(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "docker-failures")?
  let tools = fp"${root}/tools"
  tools.mkdir()?
  let repository = fs.cwd()?
  let xsh = fp"${repository}/target/debug/xsh"
  let docker_marker = fp"${root}/docker-marker"
  write_fake_tool(
    fp"${tools}/docker",
    xsh,
    f"""p"${docker_marker.display()}".write(ARGV.join("|"))?
if "run" in ARGV {
  abort(24)
}
""",
  )?
  let result = test.run_script(
    ctx,
    f"""
use docker

let ctx = ${context_source(root)}
match docker.run_internal(ctx, "dist", false, []) {
  Ok(_) => abort(1)
  Err(error) => print \${error.message}
}
""",
    [],
    {PATH: tools.display(), XSH_MODULE_PATH: fp"${repository}/dev".display()},
  )?
  test.ok(result.success, result.stderr)?
  test.contains(result.stdout, "[docker-image-build target=x86_64-unknown-linux-musl] docker build", result.stdout)?
  test.contains(result.stdout, "[docker-dist target=x86_64-unknown-linux-musl] docker run", result.stdout)?
  test.contains(result.stdout, "ContextError.StageFailed", result.stdout)?
  test.contains(docker_marker.read_text()?, "run")?
}

proc test_docker_image_build_failure_prevents_the_container_stage(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "docker-image-failure")?
  let tools = fp"${root}/tools"
  tools.mkdir()?
  let repository = fs.cwd()?
  let xsh = fp"${repository}/target/debug/xsh"
  let docker_marker = fp"${root}/docker-marker"
  write_fake_tool(
    fp"${tools}/docker",
    xsh,
    f"""p"${docker_marker.display()}".write(ARGV.join("|"))?
abort(24)""",
  )?
  let result = test.run_script(
    ctx,
    f"""
use docker

let ctx = ${context_source(root)}
match docker.run_internal(ctx, "dist", false, []) {
  Ok(_) => abort(1)
  Err(error) => print \${error.message}
}
""",
    [],
    {PATH: tools.display(), XSH_MODULE_PATH: fp"${repository}/dev".display()},
  )?
  test.ok(result.success, result.stderr)?
  test.contains(result.stdout, "[docker-image-build target=x86_64-unknown-linux-musl] docker build", result.stdout)?
  test.ok("[docker-dist" not in result.stdout, result.stdout)?
  test.contains(docker_marker.read_text()?, "build")?
}

proc test_trace_keeps_process_status_for_a_failed_child(ctx: TestContext) [error] {
  let traced = test.run_xsht_trace(
    ctx,
    """
run.status false
""",
    ["--trace", "--raw"],
  )?
  test.ok(traced.success, traced.stderr)?
  test.contains(traced.stderr, "kind=run.start")?
  test.contains(traced.stderr, "kind=run.end")?
  test.contains(traced.stderr, "status={kind:exit success:false code:1}")?
}

proc test_make_facade_only_delegates_to_the_development_entrypoint() [fs, error] {
  let facade = p"Makefile".read_text()?

  for command in [
    "XSH_DEV ?= target/debug/xsh",
    "$(XSH_DEV) dev/main.xsh --",
    "cargo dev",
    "$(DEV) lint --fix",
    "$(DEV) test linux --ci",
    "$(DEV) coverage --backend docker",
    "$(DEV) bench --syscalls",
    "$(DEV) dist --docker always",
  ] {
    test.contains(facade, command, facade)?
  }

  for forbidden in ["cargo build", "cargo test", "sh -c", "bash -c", "docker run"] {
    test.ok(forbidden not in facade, facade)?
  }
}

proc test_codesign_failure_stops_darwin_installation(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "codesign-failure")?
  let tools = fp"${root}/tools"
  tools.mkdir()?
  let release_dir = fp"${root}/target/aarch64-apple-darwin/release"
  release_dir.mkdir()?
  fp"${release_dir}/xsh".write("binary")?
  fs.chmod(fp"${release_dir}/xsh", 0o755)?
  let repository = fs.cwd()?
  let xsh = fp"${repository}/target/debug/xsh"
  let codesign_marker = fp"${root}/codesign-marker"
  write_fake_tool(fp"${tools}/cargo", xsh, "let built = true")?
  write_fake_tool(
    fp"${tools}/codesign",
    xsh,
    f"""p"${codesign_marker.display()}".write("codesign")?
abort(25)""",
  )?
  let result = test.run_script(
    ctx,
    f"""
use install

let ctx = ${darwin_context_source(root)}
match install.darwin(ctx) {
  Ok(_) => abort(1)
  Err(error) => print \${error.message}
}
""",
    [],
    {
      PATH: tools.display(),
      HOME: fp"${root}/home".display(),
      XSH_MODULE_PATH: fp"${repository}/dev".display(),
    },
  )?
  test.ok(result.success, result.stderr)?
  test.contains(result.stdout, "[install-darwin-codesign target=aarch64-apple-darwin] codesign", result.stdout)?
  test.contains(result.stdout, "ContextError.StageFailed", result.stdout)?
  test.ok(codesign_marker.exists()?)?
}
