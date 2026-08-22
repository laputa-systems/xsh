use dist as distributions
use verify

pure verification_context(root: Path, profile: Str = "dist") -> Record {
  return {
    root: root,
    target_dir: fp"${root}/target",
    coverage_dir: fp"${root}/target/cov",
    artifact_dir: fp"${root}/dist",
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
    profile: profile,
    darwin_deployment_target: "26.0",
  }
}

pure verification_context_source(root: Path) -> Str {
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

proc write_fake_tool(tool_path: Path, xsh: Path, body: Str) [fs, error] {
  tool_path.write(f"""#!${xsh.display()}
${body}
""")?
  fs.chmod(tool_path, 0o755)?
}

proc test_binary_verification_rejects_missing_and_non_elf_products(ctx: TestContext) [fs, process, error, io] {
  let root = test.temp_dir(ctx, name: "verify-binary")?
  let verify_ctx = verification_context(root)

  match verify.binary(verify_ctx, "xsh", false) {
    Ok(_) => test.fail("missing product passed verification")?
    Err(error) => test.contains(error.message, "ContextError.StageFailed", error.message)?
  }

  let product = fp"${root}/target/x86_64-unknown-linux-musl/dist/xsh"
  product.parent().mkdir()?
  let padding = (["x"]
    |> repeat(1024)
    |> collect()).join("")
  product.write(padding)?
  fs.chmod(product, 0o755)?

  match verify.binary(verify_ctx, "xsh", false) {
    Ok(_) => test.fail("non-ELF product passed verification")?
    Err(error) => test.contains(error.message, "ContextError.StageFailed", error.message)?
  }
}

proc test_distribution_product_paths_are_stable() [error] {
  let target_dir = /repo/target
  test.eq(
    distributions.profile_product_path(target_dir, "x86_64-unknown-linux-musl", "release", "xsh").display(),
    "/repo/target/x86_64-unknown-linux-musl/release/xsh",
  )?
  test.eq(
    distributions.distribution_product_path(target_dir, "aarch64-unknown-linux-musl", "xsht").display(),
    "/repo/target/aarch64-unknown-linux-musl/dist/xsht",
  )?
}

proc test_linux_verification_rejects_wrong_machine_and_dynamic_binaries(ctx: TestContext) [fs, error] {
  let root = test.temp_dir(ctx, name: "verify-linux")?
  let product = fp"${root}/target/x86_64-unknown-linux-musl/dist/xsh"
  product.parent().mkdir()?
  let padding = (["x"]
    |> repeat(1020)
    |> collect()).join("")
  product.write("\u{7f}ELF" + padding)?
  fs.chmod(product, 0o755)?
  let tools = fp"${root}/tools"
  tools.mkdir()?
  let repository = fs.cwd()?
  let xsh = fp"${repository}/target/debug/xsh"
  let module_path = fp"${repository}/dev".display()

  write_fake_tool(
    fp"${tools}/readelf",
    xsh,
    """if "-h" in ARGV {
  print "Machine: AArch64"
} else {
  print ""
}""",
  )?
  let wrong_machine = test.run_script(
    ctx,
    f"""
use verify

proc main() [fs, process, error, io] -> Result[Unit] {
  let ctx = ${verification_context_source(root)}
  match verify.binary(ctx, "xsh", false) {
    Ok(_) => abort(1)
    Err(error) => print \${error.message}
  }
}

main()?
""",
    [],
    {PATH: tools.display(), XSH_MODULE_PATH: module_path},
  )?
  test.ok(wrong_machine.success, wrong_machine.stderr)?
  test.contains(wrong_machine.stdout, "ContextError.StageFailed", wrong_machine.stdout)?

  write_fake_tool(
    fp"${tools}/readelf",
    xsh,
    """if "-h" in ARGV {
  print "Machine: Advanced Micro Devices X86-64"
} else {
  print "NEEDED"
}""",
  )?
  let dynamic = test.run_script(
    ctx,
    f"""
use verify

proc main() [fs, process, error, io] -> Result[Unit] {
  let ctx = ${verification_context_source(root)}
  match verify.binary(ctx, "xsh", false) {
    Ok(_) => abort(1)
    Err(error) => print \${error.message}
  }
}

main()?
""",
    [],
    {PATH: tools.display(), XSH_MODULE_PATH: module_path},
  )?
  test.ok(dynamic.success, dynamic.stderr)?
  test.contains(dynamic.stdout, "ContextError.StageFailed", dynamic.stdout)?
}
