use context
use dist as distributions
use fixtures
use verify

pure verification_context(root: Path, profile: Str = "dist") -> Result[context.Context] {
  fixtures.linux_context(root, profile)
}

pure verification_context_source(root: Path) -> Str {
  f"""{{
  root: p"{root}",
  target_dir: p"{root}/target",
  coverage_dir: p"{root}/target/cov",
  artifact_dir: p"{root}/dist",
  host_os: target_policy.Linux,
  host_arch: target_policy.X86_64,
  target: target_policy.resolve("x86_64-unknown-linux-musl")?,
  profile: "dist",
  darwin_deployment_target: "26.0",
}}"""
}

proc write_fake_tool(tool_path: Path, xsh: Path, body: Str) [fs, error] {
  tool_path.write(f"""#!{xsh}
{body}
""")?
  fs.chmod(tool_path, 0o755)?
}

test test_binary_verification_rejects_missing_and_non_elf_products { |ctx|
  let root = test.temp_dir(ctx, name: "verify-binary")?
  let verify_ctx = verification_context(root)?

  match verify.binary(verify_ctx, "xsh", false) {
    Ok(_) => test.fail("missing product passed verification")?
    Err(error) => assert "StageError.Failed" in error.message, error.message
  }

  let product = fp"{root}/target/x86_64-unknown-linux-musl/dist/xsh"
  product.parent().mkdir()?
  let padding = (["x"]
    |> repeat(1024)
    |> collect()).join("")
  product.write(padding)?
  fs.chmod(product, 0o755)?

  match verify.binary(verify_ctx, "xsh", false) {
    Ok(_) => test.fail("non-ELF product passed verification")?
    Err(error) => assert "StageError.Failed" in error.message, error.message
  }
}

test test_distribution_product_paths_are_stable {
  let target_dir = /repo/target
  assert distributions.profile_product_path(target_dir, "x86_64-unknown-linux-musl", "release", "xsh") == "/repo/target/x86_64-unknown-linux-musl/release/xsh"
  assert distributions.distribution_product_path(target_dir, "aarch64-unknown-linux-musl", "xsht") == "/repo/target/aarch64-unknown-linux-musl/dist/xsht"
}

test test_linux_verification_rejects_wrong_machine_and_dynamic_binaries { |ctx|
  let root = test.temp_dir(ctx, name: "verify-linux")?
  let product = fp"{root}/target/x86_64-unknown-linux-musl/dist/xsh"
  product.parent().mkdir()?
  let padding = (["x"]
    |> repeat(1020)
    |> collect()).join("")
  product.write("\u{7f}ELF" + padding)?
  fs.chmod(product, 0o755)?
  let tools = fp"{root}/tools"
  tools.mkdir()?
  let repository = fs.cwd()?
  let xsh = ctx.xsh_bin
  let module_path = fp"{repository}/dev".display()

  write_fake_tool(
    fp"{tools}/readelf",
    xsh,
    """if "-h" in args {
  print "Machine: AArch64"
} else {
  print ""
}""",
  )?
  let wrong_machine = test.run_script(
    ctx,
    f"""
use context
use targets as target_policy
use verify

proc main() [fs, process, error, io] -> Result[Unit] {{
  let ctx: context.Context = {verification_context_source(root)}
  match verify.binary(ctx, "xsh", false) {{
    Ok(_) => abort(1)
    Err(error) => print ${{error.message}}
  }}
}}

main()?
""",
    [],
    {PATH: tools, XSH_MODULE_PATH: module_path},
  )?
  assert wrong_machine.success, wrong_machine.stderr
  assert "StageError.Failed" in wrong_machine.stdout, wrong_machine.stdout

  write_fake_tool(
    fp"{tools}/readelf",
    xsh,
    """if "-h" in args {
  print "Machine: Advanced Micro Devices X86-64"
} else {
  print "NEEDED"
}""",
  )?
  let dynamic = test.run_script(
    ctx,
    f"""
use context
use targets as target_policy
use verify

proc main() [fs, process, error, io] -> Result[Unit] {{
  let ctx: context.Context = {verification_context_source(root)}
  match verify.binary(ctx, "xsh", false) {{
    Ok(_) => abort(1)
    Err(error) => print ${{error.message}}
  }}
}}

main()?
""",
    [],
    {PATH: tools, XSH_MODULE_PATH: module_path},
  )?
  assert dynamic.success, dynamic.stderr
  assert "StageError.Failed" in dynamic.stdout, dynamic.stdout
}
