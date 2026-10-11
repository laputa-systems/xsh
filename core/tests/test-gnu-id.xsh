use support.uu

proc lookup_status(r: uu.Ran, names: Bool) {
  assert r.status == 0 or (names and r.status == 1), f"id {r.args.join(" ")}: unexpected status {r.status}"
}

# origin: gnu id/uid.log
test test_gnu_id_uid_log { |ctx|
  let s = uu.scene(ctx)?
  let number = uu.invoke(s, "id", ["-u"])?
  let named = uu.invoke(s, "id", ["-nu"])?
  uu.succeeds(number)
  uu.succeeds(named)
  let uid = number.stdout.utf8()?.trim()
  let account = named.stdout.utf8()?.trim()
  uu.fails_with_code(uu.invoke(s, "id", [""])?, 1)
  let single = uu.invoke(s, "id", [account])?
  uu.succeeds(single)
  let continued = uu.invoke(s, "id", ["", account])?
  uu.fails_with_code(continued, 1)
  assert continued.stdout == single.stdout
  for mode in [[], ["-G"], ["-g"]] {
    let by_name = uu.invoke(s, "id", mode.extend([account]))?
    uu.succeeds(by_name)
    for operand in [uid, "+" + uid] {
      let by_number = uu.invoke(s, "id", mode.extend([operand]))?
      uu.succeeds(by_number)
      assert by_number.stdout == by_name.stdout
    }
  }
}

# origin: gnu id/zero.log
test test_gnu_id_zero_log { |ctx|
  let s = uu.scene(ctx)?
  let named = uu.invoke(s, "id", ["-nu"])?
  let account = named.stdout.utf8()?.trim()
  uu.succeeds(uu.invoke(s, "id", [])?)
  uu.succeeds(uu.invoke(s, "id", [account])?)
  let invalid = uu.invoke(s, "id", ["--zero"])?
  uu.fails(invalid)
  uu.stderr_only(invalid, "id: option --zero not permitted in default format\n")
  var accounts = [account]
  for candidate in ["root", "man", "postfix", "sshd", "nobody"] {
    let found = uu.invoke(s, "id", [candidate])?
    if found.status == 0 { accounts += [candidate] }
  }
  var failures: List[Str] = []
  for name in accounts + [""] {
    let operands = if name == "" { [] } else { [name] }
    for mode in ["g", "gr", "G", "Gr", "u", "ur"] {
      for suffix in ["", "n"] {
        let flag = "-" + mode + suffix
        let plain = uu.invoke(s, "id", [flag].extend(operands))?
        let zero = uu.invoke(s, "id", [flag + "z"].extend(operands))?
        lookup_status(plain, suffix == "n")
        lookup_status(zero, suffix == "n")
        assert zero.stdout.len() >= 1, "zero output must include a terminator"
        let restored = (zero.stdout[..-1].utf8()? + "\n").replace("\0", with: " ")
        if plain.stdout != bytes.from_text(restored) { failures += [f"{flag}[z] {name}: terminator or separator differs"] }
      }
    }
  }
  for mode in ["g", "gr", "u", "ur"] {
    for suffix in ["", "n"] {
      let flag = "-" + mode + suffix
      let plain = uu.invoke(s, "id", [flag].extend(accounts))?
      let zero = uu.invoke(s, "id", [flag + "z"].extend(accounts))?
      lookup_status(plain, suffix == "n")
      lookup_status(zero, suffix == "n")
      if plain.stdout != bytes.from_text(zero.stdout.utf8()?.replace("\0", with: "\n")) { failures += [f"{flag}[z] multiple accounts: delimiter differs"] }
    }
  }
  for mode in ["G", "Gr"] {
    for suffix in ["", "n"] {
      let flag = "-" + mode + suffix
      let plain = uu.invoke(s, "id", [flag].extend(accounts))?
      let zero = uu.invoke(s, "id", [flag + "z"].extend(accounts))?
      lookup_status(plain, suffix == "n")
      lookup_status(zero, suffix == "n")
      let restored = (zero.stdout.utf8()?.replace("\0", with: " ") + "\n").replace("  ", with: "\n")
      if bytes.concat([plain.stdout, b"\n"]) != bytes.from_text(restored) { failures += [f"{flag}[z] multiple accounts: group boundary differs"] }
    }
  }
  assert failures.is_empty(), failures.join("\n")
}
