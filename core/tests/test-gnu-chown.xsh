use support.uu

# origin: gnu chown/deref.log
test test_gnu_chown_deref_log { |ctx|
  let s = uu.scene(ctx)?
  let owner = user.current()?.name
  uu.symlink(s, "no-such", "dangle")?
  let dangling = uu.invoke(s, "chown", ["--dereference", owner, "dangle"])?
  uu.fails(dangling)
  let strip = regex.compile(":[^:\n]*\n")?
  assert strip.replace(dangling.stderr.utf8()?, with: "\n") == "chown: cannot dereference 'dangle'\n"
  uu.mkdir(s, "cyc/b/c")?
  uu.symlink(s, uu.at(s, "cyc").display(), "cyc/b/c/d")?
  let cycle = uu.invoke(s, "chown", ["-vRL", owner, "cyc"], timeout: 10s)?
  uu.succeeds(cycle)
  let output = bytes.concat([cycle.stdout, cycle.stderr]).utf8()?
  assert "'cyc/b/c/d'" in output
  assert ! ("'cyc/b/c/d/b'" in output)
}

# origin: gnu chown/preserve-root.log
test test_gnu_chown_preserve_root_log { |ctx|
  let s = uu.scene(ctx)?
  let identity = unix.id()?
  if identity.euid == 0 { test.skip("requires an unprivileged owner"); return }
  uu.mkdir(s, "d")?
  uu.symlink(s, "/", "d/slink-to-root")?
  for item in [
    {util: "chown", args: ["-R", "--preserve-root", "0", "/"]},
    {util: "chgrp", args: ["-R", "--preserve-root", "0", "/"]},
    {util: "chmod", args: ["-R", "--preserve-root", "u+r", "/"]},
  ] {
    let r = uu.invoke(s, item.util, item.args, timeout: 10s)?
    uu.fails(r)
    assert bytes.concat([r.stdout, r.stderr]) == bytes.from_text(f"{item.util}: it is dangerous to operate recursively on '/'\n{item.util}: use --no-preserve-root to override this failsafe\n")
  }
  for item in [{util: "chown", id: identity.euid}, {util: "chgrp", id: identity.egid}] {
    let physical = uu.invoke(s, item.util, ["-RHh", "--preserve-root", f"{item.id}", "d"], timeout: 10s)?
    uu.succeeds(physical)
    uu.no_output(physical)
    let logical = uu.invoke(s, item.util, ["-RLh", "--preserve-root", f"{item.id}", "d"], timeout: 10s)?
    uu.fails(logical)
    assert bytes.concat([logical.stdout, logical.stderr]) == bytes.from_text(f"{item.util}: it is dangerous to operate recursively on 'd/slink-to-root' (same as '/')\n{item.util}: use --no-preserve-root to override this failsafe\n")
  }
}

# origin: gnu chown/separator.log
test test_gnu_chown_separator_log { |ctx|
  let s = uu.scene(ctx)?
  let account = user.current()?
  let membership = group.current()?
  let out = uu.at(s, ".group-records")
  let err = uu.at(s, ".group-errors")
  let listing = process.command_argv(p"/usr/bin/getent", [p"/usr/bin/getent", p"group"], s.root, {LC_ALL: "C"}, b"", out, err, timeout: 10s)
  assert process.run(listing)?.shell_code()? == 0, err.read_text()?
  let count = [line for line in out.read_text()?.split("\n") if line.starts_with(membership.name + ":")].len()
  if count != 1 { test.skip(f"group {membership.name} is not unique"); return }
  uu.succeeds(uu.invoke(s, "chown", ["", "."])?)
  for owner in [f"{account.uid}", account.name, ""] {
    for member in [f"{membership.gid}", membership.name, ""] {
      let separators = if "." in owner + member { [":"] } else { [":", "."] }
      for separator in separators {
        let spec = owner + separator + member
        let r = uu.invoke(s, "chown", [spec, "."])?
        if member == "" and rx"^[0-9]".matches(owner) { uu.fails_with_code(r, 1) } else { uu.succeeds(r) }
      }
    }
  }
}
