use fixtures
use test_workflows
use unix

test test_linux_root_native_tests_use_the_host_identity_and_home { |ctx|
  if system.uname()?.sysname != "Linux" or unix.id()?.euid != 0 {
    test.skip("requires a root Linux test process")
    return
  }

  let fixture_root = test.temp_dir(ctx, name: "native-root-identity")?
  fixture_root.chmod(0o777)
  let target_dir = fp"{fixture_root}/target"
  target_dir.mkdir()
  let product_dir = fp"{target_dir}/x86_64-unknown-linux-musl/verification"
  product_dir.parent().mkdir()
  product_dir.mkdir()
  let log = fp"{fixture_root}/observed"
  fp"{product_dir}/xsht".write(
    f"""#!{ctx.xsh_bin}
use unix
let home = env.get("HOME")?
fp"{{home}}/probe".write("writable")?
p"{log}".write(f"{{unix.id()?.euid}}|{{unix.id()?.egid}}|{{home}}|{{env.get("TMPDIR")?}}|{{args.join("|")}}")?
""",
    mode: 0o755,
  )
  let workflow_context = {
    ...fixtures.linux_context(fixture_root, "verification")?,
    target_dir: target_dir,
  }
  let host_uid = env.get("HOST_UID")?.trim().parse_int()?
  let host_gid = env.get("HOST_GID")?.trim().parse_int()?
  let home = fp"{fixture_root}/.work/tmp/native-home-{host_uid}-{host_gid}"
  let padded_uid = f"00{host_uid}"
  let padded_gid = f"0{host_gid}"

  env HOST_UID=$padded_uid HOST_GID=$padded_gid {
    test_workflows.native(workflow_context, test_workflows.Verification,
      ["--exact", "native-workflow-contract"])?
  }

  assert log.read_text()? == f"{host_uid}|{host_gid}|{home}|{fixture_root}/.work/tmp|test|--exact|native-workflow-contract"
  assert fp"{home}/probe".read_text()? == "writable"
}

test test_linux_root_native_tests_fail_when_setpriv_is_unavailable_or_identity_is_missing { |ctx|
  if system.uname()?.sysname != "Linux" or unix.id()?.euid != 0 {
    test.skip("requires a root Linux test process")
    return
  }

  let fixture_root = test.temp_dir(ctx, name: "native-root-errors")?
  let workflow_context = fixtures.linux_context(fixture_root, "verification")?
  let host_uid = env.get("HOST_UID")?.trim()
  let host_gid = env.get("HOST_GID")?.trim()
  env HOST_UID="" HOST_GID="" {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted a missing host identity")
      Err(_) => {}
    }
  }

  env HOST_UID="0" HOST_GID=$host_gid {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted the root UID")
      Err(_) => {}
    }
  }

  env HOST_UID="00" HOST_GID=$host_gid {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted a padded root UID")
      Err(_) => {}
    }
  }

  env HOST_UID="root" HOST_GID=$host_gid {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted a nonnumeric UID")
      Err(_) => {}
    }
  }

  env HOST_UID=$host_uid HOST_GID="group" {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted a nonnumeric GID")
      Err(_) => {}
    }
  }

  env HOST_UID="-1" HOST_GID=$host_gid {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted a negative UID")
      Err(_) => {}
    }
  }

  env HOST_UID=$host_uid HOST_GID="-1" {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests accepted a negative GID")
      Err(_) => {}
    }
  }

  env HOST_UID=$host_uid HOST_GID=$host_gid PATH="" {
    match test_workflows.native(workflow_context, test_workflows.Verification, []) {
      Ok(_) => test.fail("root native tests ignored an unavailable setpriv")
      Err(_) => {}
    }
  }
}

test test_linux_nonroot_native_tests_keep_the_callers_environment { |ctx|
  if system.uname()?.sysname != "Linux" or unix.id()?.euid == 0 {
    test.skip("requires a non-root Linux test process")
    return
  }

  let fixture_root = test.temp_dir(ctx, name: "native-nonroot-identity")?
  let target_dir = fp"{fixture_root}/target"
  target_dir.mkdir()
  let product_dir = fp"{target_dir}/x86_64-unknown-linux-musl/verification"
  product_dir.parent().mkdir()
  product_dir.mkdir()
  let log = fp"{fixture_root}/observed"
  fp"{product_dir}/xsht".write(
    f"""#!{ctx.xsh_bin}
use unix
p"{log}".write(f"{{unix.id()?.euid}}|{{env.get("HOME")?}}|{{env.get("TMPDIR")?}}|{{args.join("|")}}")?
""",
    mode: 0o755,
  )
  let workflow_context = {
    ...fixtures.linux_context(fixture_root, "verification")?,
    target_dir: target_dir,
  }
  let identity = unix.id()?
  let home = fp"{fixture_root}/home"
  let temp = fp"{fixture_root}/temp"
  let home_value = home.display()
  let temp_value = temp.display()
  home.mkdir()
  temp.mkdir()

  env HOST_UID="" HOST_GID="" HOME=$home_value TMPDIR=$temp_value PATH="" {
    test_workflows.native(workflow_context, test_workflows.Verification,
      ["--exact", "native-workflow-contract"])?
  }

  assert log.read_text()? == f"{identity.euid}|{home}|{temp}|test|--exact|native-workflow-contract"
}

test test_nonlinux_native_tests_keep_the_direct_command { |ctx|
  let fixture_root = test.temp_dir(ctx, name: "native-nonlinux-direct")?
  let target_dir = fp"{fixture_root}/target"
  target_dir.mkdir()
  let product_dir = fp"{target_dir}/aarch64-apple-darwin/verification"
  product_dir.parent().mkdir()
  product_dir.mkdir()
  let log = fp"{fixture_root}/observed"
  fp"{product_dir}/xsht".write(
    f"""#!{ctx.xsh_bin}
use unix
p"{log}".write(f"{{unix.id()?.euid}}|{{env.get("HOME")?}}|{{env.get("TMPDIR")?}}|{{args.join("|")}}")?
""",
    mode: 0o755,
  )
  let workflow_context = {
    ...fixtures.darwin_context(fixture_root, "verification")?,
    target_dir: target_dir,
  }
  let identity = unix.id()?
  let home = fp"{fixture_root}/home"
  let temp = fp"{fixture_root}/temp"
  let home_value = home.display()
  let temp_value = temp.display()
  home.mkdir()
  temp.mkdir()

  env HOST_UID="" HOST_GID="" HOME=$home_value TMPDIR=$temp_value PATH="" {
    test_workflows.native(workflow_context, test_workflows.Verification,
      ["--exact", "native-workflow-contract"])?
  }

  assert log.read_text()? == f"{identity.euid}|{home}|{temp}|test|--exact|native-workflow-contract"
}
