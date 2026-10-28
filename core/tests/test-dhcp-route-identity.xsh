proc dump_routes(ctx: TestContext, setup: List[Str]) [fs, process, error] -> Result[Str?] {
  let root = test.temp_dir(ctx, name: "route-tos")?
  let source = f"""use lib.nettools_fixture as fixture

proc main() [fs, process, net, io, error] {{
  fixture.link_add_dummy("d0", "02:00:00:00:00:01")
  fixture.link_up("d0")
  fixture.address_add4("d0", "192.0.2.1", 24, "192.0.2.255", "")
{setup.join("\n")}
  for route in linux.network_dump()?.routes {{
    io.write_stdout(f"{{route.destination ?? "-"}}/{{route.destination_prefix_length}} tos={{route.tos}} table={{route.table}}\\n")
  }}
}}
"""
  let child = test.temp_file(ctx, name: "tos.xsh", contents: bytes.from_text(source))?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let child_env = {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", XSH_MODULE_PATH: ctx.core_dir.display()}
  let plan = process.command_argv(ctx.xsh_bin, [ctx.xsh_bin.display(), child.display()], root, child_env, b"", out, err, timeout: 60s)
  match linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) {
    Ok(status) => {
      guard status.shell_code()? == 0 else {
        test.skip(f"cannot build the route fixture in a private network namespace: {err.read_text()?.trim()}")
        return Ok(null)
      }
      Ok(out.read_text()?)
    }
    Err(failure) => {
      test.skip(f"the kernel refuses a private user and network namespace: {failure.message}")
      Ok(null)
    }
  }
}

test test_network_dump_distinguishes_routes_that_differ_only_in_tos { |ctx|
  let setup = [
    "  fixture.route_add_tos4(\"10.9.0.0\", 24, 0, \"d0\")",
    "  fixture.route_add_tos4(\"10.9.0.0\", 24, 8, \"d0\")",
    "  fixture.route_add_tos4(\"0.0.0.0\", 0, 16, \"d0\")",
  ]
  guard let listing = dump_routes(ctx, setup)? else { return }
  let lines = listing.lines()
  assert "10.9.0.0/24 tos=0 table=254" in lines, listing
  assert "10.9.0.0/24 tos=8 table=254" in lines, listing
  assert "-/0 tos=16 table=254" in lines, listing
  assert "192.0.2.0/24 tos=0 table=254" in lines, listing
}
