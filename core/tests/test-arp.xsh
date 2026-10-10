use core.lib.nettools as nettools

# Expected rows were produced by the reference tool for the same entries.

pure entry(address: Str, flags: Int, mac: Bytes, iface: Str) -> nettools.Neighbour {
  {address: address, hwtype: 1, hwaddr: mac, flags: flags, iface: iface, ifindex: 2, proxy: flags == 12}
}

test test_arp_rows_follow_the_reference_columns {
  assert nettools.ARP_HEADER == "Address                  HWtype  HWaddress           Flags Mask            Iface"
  let permanent = entry("10.1.0.9", 6, b"\x02\x00\x00\x00\x00\x99", "d1")
  assert nettools.arp_row(permanent, "10.1.0.9") == "10.1.0.9                 ether   02:00:00:00:00:99   CM                    d1"
  let resolved = entry("192.0.2.61", 2, b"\x02\x00\x00\x00\x00\x61", "d0")
  assert nettools.arp_row(resolved, "192.0.2.61") == "192.0.2.61               ether   02:00:00:00:00:61   C                     d0"
  assert nettools.arp_row(resolved, "router") == "router                   ether   02:00:00:00:00:61   C                     d0"
  let pending = entry("192.0.2.60", 0, b"", "d0")
  assert nettools.arp_row(pending, "192.0.2.60") == "192.0.2.60                       (incomplete)                              d0"
  let proxy = entry("192.0.2.54", 12, b"", "d0")
  assert nettools.arp_row(proxy, "192.0.2.54") == "192.0.2.54               *       <from_interface>    MP                    d0"
}

test test_arp_bsd_rows_follow_the_reference_wording {
  let permanent = entry("10.1.0.9", 6, b"\x02\x00\x00\x00\x00\x99", "d1")
  assert nettools.arp_bsd_row(permanent, "?") == "? (10.1.0.9) at 02:00:00:00:00:99 [ether] PERM on d1"
  let resolved = entry("192.0.2.61", 2, b"\x02\x00\x00\x00\x00\x61", "d0")
  assert nettools.arp_bsd_row(resolved, "router") == "router (192.0.2.61) at 02:00:00:00:00:61 [ether] on d0"
  assert nettools.arp_bsd_row(entry("192.0.2.60", 0, b"", "d0"), "?") == "? (192.0.2.60) at <incomplete> on d0"
  assert nettools.arp_bsd_row(entry("192.0.2.54", 12, b"", "d0"), "?") == "? (192.0.2.54) at <from_interface> PERM PUB on d0"
}

pure sorted_lines(text: Str) -> List[Str] {
  var sorted: List[Str] = []
  for line in text.lines() {
    var at = sorted.len()
    for index in range(sorted.len()) {
      if sorted[index] > line {
        at = index
        break
      }
    }
    sorted = sorted[0..at] + [line] + sorted[at..]
  }
  sorted
}

# A private network namespace with two dummy interfaces and a few neighbours,
# entered through a child program that sets the fixture up and then runs arp
# once per command.
proc session(ctx: TestContext, commands: List[List[Str]]) [fs, process, error] -> Result[Str] {
  let root = test.temp_dir(ctx, name: "arp-ns")?
  let setup = [
    "fixture.link_add_dummy(\"d0\", \"02:00:00:00:00:01\")",
    "fixture.link_up(\"d0\")",
    "fixture.address_add4(\"d0\", \"192.0.2.1\", 24, \"192.0.2.255\", \"\")",
    "fixture.link_add_dummy(\"d1\", \"02:00:00:00:00:02\")",
    "fixture.link_up(\"d1\")",
    "fixture.address_add4(\"d1\", \"10.1.0.1\", 16, \"10.1.255.255\", \"\")",
    "fixture.neighbour_add(\"d0\", \"192.0.2.61\", \"02:00:00:00:00:61\", 2)",
    "fixture.neighbour_add(\"d0\", \"192.0.2.62\", \"02:00:00:00:00:62\", 4)",
    "fixture.neighbour_add(\"d0\", \"192.0.2.60\", \"\", 1)",
    "fixture.neighbour_add(\"d0\", \"192.0.2.64\", \"02:00:00:00:00:64\", 64)",
    "fixture.neighbour_add(\"d1\", \"10.1.0.9\", \"02:00:00:00:00:99\", 128)",
  ]
  let source = f"""use lib.nettools_fixture as fixture

proc main(...argv: List[Str]) [fs, process, io, error] {{
{setup.join("\n")}
  fixture.run_commands(argv)
}}
"""
  let child = test.temp_file(ctx, name: "session.xsh", contents: bytes.from_text(source))?
  var argv = [ctx.xsh_bin.display(), child.display(), ctx.xsh_bin.display(), fp"{ctx.core_dir}/arp.xsh".display()]
  for command in commands {
    argv = argv.extend(command).push("--")
  }
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let child_env = {LC_ALL: "C", XSH_EXECUTION_PHRASE: "", XSH_MODULE_PATH: ctx.core_dir.display()}
  let plan = process.command_argv(ctx.xsh_bin, argv, root, child_env, b"", out, err, timeout: 60s)
  match linux.run_in_namespaces(plan, unshare: ["user", "net"], map_root_user: true) {
    Ok(status) => {
      if status.shell_code()? != 0 {
        test.skip(f"cannot build the neighbour fixture in a private network namespace: {err.read_text()?.trim()}")
        return ""
      }
      Ok(out.read_text()?)
    }
    Err(failure) => {
      test.skip(f"the kernel refuses a private user and network namespace: {failure.message}")
      Ok("")
    }
  }
}

test test_arp_lists_resolved_pending_and_permanent_entries { |ctx|
  let transcript = session(ctx, [["-n"], ["-an"], ["-i", "d1"], ["-ai", "d0", "-n"], ["-v", "-n"], ["-n", "192.0.2.61"], ["-an", "192.0.2.61"]])?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  # NOARP entries are not listed; the table order is the kernel's, so rows are sorted.
  let table = sorted_lines(blocks[1])
  assert table == [
    "-n",
    "10.1.0.9                 ether   02:00:00:00:00:99   CM                    d1",
    "192.0.2.60                       (incomplete)                              d0",
    "192.0.2.61               ether   02:00:00:00:00:61   C                     d0",
    "192.0.2.62               ether   02:00:00:00:00:62   C                     d0",
    "Address                  HWtype  HWaddress           Flags Mask            Iface",
    "rc=0",
  ], blocks[1]
  assert sorted_lines(blocks[2]) == [
    "-an",
    "? (10.1.0.9) at 02:00:00:00:00:99 [ether] PERM on d1",
    "? (192.0.2.60) at <incomplete> on d0",
    "? (192.0.2.61) at 02:00:00:00:00:61 [ether] on d0",
    "? (192.0.2.62) at 02:00:00:00:00:62 [ether] on d0",
    "rc=0",
  ], blocks[2]
  assert blocks[3] == "-i d1\nAddress                  HWtype  HWaddress           Flags Mask            Iface\n10.1.0.9                 ether   02:00:00:00:00:99   CM                    d1\nrc=0\n"
  assert sorted_lines(blocks[4]).len() == 5
  assert blocks[5].find("Entries: 4\tSkipped: 0\tFound: 4\n") != null
  assert blocks[6] == "-n 192.0.2.61\nAddress                  HWtype  HWaddress           Flags Mask            Iface\n192.0.2.61               ether   02:00:00:00:00:61   C                     d0\nrc=0\n"
  assert blocks[7] == "-an 192.0.2.61\n? (192.0.2.61) at 02:00:00:00:00:61 [ether] on d0\nrc=0\n"
}

test test_arp_reports_hosts_and_filters_with_no_match { |ctx|
  let transcript = session(ctx, [["-n", "192.0.2.99"], ["-an", "192.0.2.99"], ["-i", "nosuch0"], ["-H", "ax25", "-n"], ["-H", "netrom", "-n"], ["-A", "inet6"], ["-v", "-i", "d1", "-n"]])?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[1] == "-n 192.0.2.99\n192.0.2.99 (192.0.2.99) -- no entry\nrc=0\n"
  assert blocks[2] == "-an 192.0.2.99\narp: in 4 entries no match found.\nrc=0\n"
  assert blocks[3] == "-i nosuch0\narp: in 4 entries no match found.\nrc=0\n"
  assert blocks[4] == "-H ax25 -n\n2> arp: ax25: unknown hardware type.\nrc=255\n"
  assert blocks[5] == "-H netrom -n\narp: in 4 entries no match found.\nrc=0\n"
  assert blocks[6] == "-A inet6\n2> arp: inet6: kernel only supports 'inet'.\nrc=255\n"
  assert blocks[7].find("Entries: 4\tSkipped: 3\tFound: 1\n") != null
}

test test_arp_sets_and_deletes_entries { |ctx|
  let transcript = session(
    ctx,
    [
      ["-s", "192.0.2.50", "02:00:00:00:00:50"],
      ["-s", "192.0.2.51", "02:00:00:00:00:51", "temp"],
      ["-s", "10.1.0.8", "02:00:00:00:00:98", "-i", "d1"],
      ["-s", "192.0.2.55", "02:00:00:00:00:55", "pub", "-i", "d0"],
      ["-Ds", "192.0.2.54", "d0", "pub"],
      ["-s", "192.0.2.40", "d0", "-D"],
      ["-n"],
      ["-d", "192.0.2.50"],
      ["-i", "d0", "-d", "192.0.2.54", "pub"],
      ["-d", "192.0.2.99"],
      ["-d", "192.0.2.5", "-i", "nosuch0"],
      ["-n"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  for index in range(1, 7) {
    assert blocks[index].ends_with("rc=0\n"), blocks[index]
  }
  let table = blocks[7]
  assert table.find("192.0.2.50               ether   02:00:00:00:00:50   CM                    d0\n") != null
  assert table.find("192.0.2.51               ether   02:00:00:00:00:51   C                     d0\n") != null
  assert table.find("10.1.0.8                 ether   02:00:00:00:00:98   CM                    d1\n") != null
  assert table.find("192.0.2.55               *       <from_interface>    MP                    d0\n") != null
  assert table.find("192.0.2.54               *       <from_interface>    MP                    d0\n") != null
  # -D copies the hardware address of the named interface (d0).
  assert table.find("192.0.2.40               ether   02:00:00:00:00:01   CM                    d0\n") != null
  assert blocks[8] == "-d 192.0.2.50\nrc=0\n"
  assert blocks[9] == "-i d0 -d 192.0.2.54 pub\nrc=0\n"
  assert blocks[10] == "-d 192.0.2.99\n2> No ARP entry for 192.0.2.99\nrc=255\n"
  assert blocks[11] == "-d 192.0.2.5 -i nosuch0\n2> SIOCDARP(dontpub): No such device\nrc=255\n"
  assert blocks[12].find("192.0.2.50 ") == null and blocks[12].find("192.0.2.54 ") == null
}

test test_arp_rejects_bad_set_and_delete_forms { |ctx|
  let transcript = session(
    ctx,
    [
      ["-s", "192.0.2.70"],
      ["-s", "192.0.2.70", "zz:zz"],
      ["-s", "notahost.invalid", "02:00:00:00:00:70"],
      ["-s", "192.0.2.70", "02:00:00:00:00:70", "bogus"],
      ["-s"],
      ["-d"],
      ["-s", "192.0.2.5", "02:00:00:00:00:05", "-i", "nosuch0"],
      ["-Ds", "192.0.2.41", "nosuch0"],
      ["-z"],
      ["-f", "/nonexistent-etherfile"],
      ["-V"],
    ],
  )?
  if transcript == "" { return }
  let blocks = transcript.split("$ ")
  assert blocks[1] == "-s 192.0.2.70\n2> arp: need hardware address\nrc=255\n"
  assert blocks[2] == "-s 192.0.2.70 zz:zz\n2> arp: invalid hardware address\nrc=255\n"
  assert blocks[3] == "-s notahost.invalid 02:00:00:00:00:70\n2> notahost.invalid: Unknown host\nrc=255\n"
  assert blocks[4].starts_with("-s 192.0.2.70 02:00:00:00:00:70 bogus\n2> Usage:\n") and blocks[4].ends_with("rc=3\n")
  assert blocks[5] == "-s\n2> arp: need host name\nrc=255\n"
  assert blocks[6] == "-d\n2> arp: need host name\nrc=255\n"
  assert blocks[7] == "-s 192.0.2.5 02:00:00:00:00:05 -i nosuch0\n2> SIOCSARP: No such device\nrc=255\n"
  assert blocks[8] == "-Ds 192.0.2.41 nosuch0\n2> arp: cant get HW-Address for `nosuch0': No such device.\nrc=255\n"
  assert blocks[9].starts_with("-z\n2> arp: unrecognized option: z\n2> Usage:\n") and blocks[9].ends_with("rc=3\n")
  assert blocks[10] == "-f /nonexistent-etherfile\n2> arp: cannot open etherfile /nonexistent-etherfile !\nrc=255\n"
  assert blocks[11].find("arp (XSH core)") != null
}
