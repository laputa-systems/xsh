use support.uu

pure reversed(items: List[Str]) -> List[Str] { [items[items.len() - index - 1] for index in range(items.len())] }

proc shell_sort(s: uu.Scene, args: List[Str], setup: Str, stdin = b"", vars: Record = {}, timeout = 30s) [fs, process, env, error] -> Result[uu.Ran, Error] {
  let out = uu.at(s, ".wrapper-out")
  let err = uu.at(s, ".wrapper-err")
  let words = [p"/bin/sh", p"-c", fp"{setup}", p"sort-wrapper"].extend(uu.argv(s, "sort", [fp"{word}" for word in args], vars)?)
  let plan = process.command_argv(p"/bin/sh", words, s.root, vars, stdin, out, err, timeout:)
  let status = process.run(plan)?.shell_code()?
  Ok({util: "sort", args: args, status: status, stdout: out.read_bytes()?, stderr: err.read_bytes()?})
}

proc bash_path_precedes(s: uu.Scene, left: Path, right: Path, vars: Record) [fs, process, error] -> Result[Bool, Error] {
  let stdout = uu.at(s, ".collation-order-out")
  let stderr = uu.at(s, ".collation-order-err")
  let argv = [p"/bin/bash", p"-c", p"[[ $1 < $2 ]]", p"sort-collation", left, right]
  let result = process.run(process.command_argv(p"/bin/bash", argv, s.root, vars, b"", stdout, stderr))?
  let status = result.shell_code()?
  assert status == 0 or status == 1, stderr.read_text()?
  Ok(status == 0)
}

pure pair_bytes(left: Bytes, right: Bytes, left_first: Bool) -> Bytes {
  if left_first { bytes.concat([left, b"\n", right, b"\n"]) } else { bytes.concat([right, b"\n", left, b"\n"]) }
}

pure byte_values(raw: Bytes) -> Str {
  "[" + [f"{raw.byte_at(index) ?? 0}" for index in range(raw.len())].join(", ") + "]"
}

pure padded(number: Int, width: Int) -> Str {
  ["0" for _ in range(width - f"{number}".byte_len())].join("") + f"{number}"
}

pure sequence(first: Int, last: Int, width = 0) -> Str {
  [if width == 0 { f"{number}\n" } else { padded(number, width) + "\n" } for number in range(first, last + 1)].join("")
}

# origin: gnu sort/sort-NaN-infloop.log
test test_gnu_sort_sort_NaN_infloop_log { |ctx|
  let s = uu.scene(ctx)?
  test.timeout(ctx, 150s)
  uu.write(s, "F", "nan\n")
  let merged = uu.invoke(s, "sort", ["-g", "-m", "F", "F"], timeout: 10s)?
  uu.succeeds(merged)
  uu.stdout_is(merged, "nan\nnan\n")
  uu.write(s, "many", ["nan\n" for _ in range(128095)].join(""))
  uu.succeeds(uu.invoke_from_path(s, "sort", ["-g"], uu.at(s, "many"), stdout: p"/dev/null", timeout: 120s)?)
}

# origin: gnu sort/sort-benchmark-random.log
test test_gnu_sort_sort_benchmark_random_log { |ctx|
  let s = uu.scene(ctx)?
  test.timeout(ctx, 1800s)
  let r = shell_sort(s, ["in"], r"""perl -e 'srand 1701; for (1..500000) { print pack("C*", map {32+int(rand 94)} 1..100), "\n" }' >in || exit; perl -e 'open my $f,"<","in" or die $!; my @rows=<$f>; print $_ for sort @rows' >exp || exit; exec "$@";""", timeout: 1800s)?
  uu.succeeds(r)
  assert r.stdout == uu.read(s, "exp")?
}

# origin: gnu sort/sort-buffer-size.log
test test_gnu_sort_sort_buffer_size_log { |ctx|
  let s = uu.scene(ctx)?
  test.timeout(ctx, 1800s)
  if system.memory()?.available <= 120MiB { test.skip("requires more than 120 MiB available memory"); return }
  let limit = process.rlimit("as")?
  if limit.soft != null and limit.soft <= 120MiB { test.skip("requires more than 120 MiB virtual memory"); return }
  let expected = sequence(1, 200000, 6)
  let reverse_input = [padded(200001 - number, 6) + "\n" for number in range(1, 200001)].join("")
  let directory = uu.at(s, "no-temp").display()
  let plain = uu.invoke(s, "sort", ["--parallel=1"], stdin: bytes.from_text(reverse_input), vars: {TMPDIR: directory}, timeout: 600s)?
  uu.succeeds(plain)
  uu.stdout_is(plain, expected)
  let keyed = uu.invoke(s, "sort", ["--parallel=1", "-k2,2"], stdin: bytes.from_text(["k " + padded(200001 - number, 6) + "\n" for number in range(1, 200001)].join("")), vars: {TMPDIR: directory}, timeout: 600s)?
  uu.succeeds(keyed)
  uu.stdout_is(keyed, ["k " + padded(number, 6) + "\n" for number in range(1, 200001)].join(""))
  let limited = uu.invoke(s, "sort", ["--parallel=1", "-S", "1M"], stdin: bytes.from_text(reverse_input), vars: {TMPDIR: directory}, timeout: 600s)?
  uu.fails_with_code(limited, 2)
}

# origin: gnu sort/sort-compress-hang.log
test test_gnu_sort_sort_compress_hang_log { |ctx|
  let s = uu.scene(ctx)?
  test.timeout(ctx, 1800s)
  uu.at(s, "compress").write("#!/bin/sh\nif tr 41 14; then : >ok; else exit \"$?\"; fi\n", mode: 0o755)
  let expected = sequence(1, 200000, 6)
  uu.write(s, "in", [padded(200001 - number, 6) + "\n" for number in range(1, 200001)].join(""))
  let r = uu.invoke(s, "sort", ["--compress-program=./compress", "-S", "1k", "in"], timeout: 1700s)?
  uu.succeeds(r)
  uu.stdout_is(r, expected)
  assert uu.exists(s, "ok")?
}

# origin: gnu sort/sort-continue.log
test test_gnu_sort_sort_continue_log { |ctx|
  let s = uu.scene(ctx)?
  let probe = shell_sort(s, [], r"""ulimit -n 7 || exit; exec 3<&- 4<&- 5<&- 6<&-; exec "$@";""")?
  if probe.status != 0 { test.skip("sort cannot start under a seven-descriptor limit"); return }
  let files = [f"__test.{number}" for number in range(1, 32)] |> sort
  for number in range(1, 32) { uu.write(s, f"__test.{number}", f"{number}\n") }
  let merged = shell_sort(s, ["-n", "-m"].extend(files), r"""ulimit -n 7 || exit; exec 3<&- 4<&- 5<&- 6<&-; exec "$@";""")?
  uu.succeeds(merged)
  uu.stdout_is(merged, sequence(1, 31))
  let stdin = shell_sort(s, ["-n", "-m", @files, "-"], r"""ulimit -n 7 || exit; exec 3<&- 4<&- 5<&- 6<&-; exec "$@";""", b"32\n")?
  uu.succeeds(stdin)
  uu.stdout_is(stdin, sequence(1, 32))
}

# origin: gnu sort/sort-debug-keys.log
test test_gnu_sort_sort_debug_keys_log { |ctx|
  let s = uu.scene(ctx)?
  let failures = collect {
    for item in [
      {
        args: [
          "-s",
          "-k2n",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "1\n ^ no match for key\n\n^ no match for key\n44\n  ^ no match for key\n33\n  ^ no match for key\n2\n ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1.3n",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "1\n ^ no match for key\n\n^ no match for key\n44\n  ^ no match for key\n33\n  ^ no match for key\n2\n ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1n",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "\n^ no match for key\n1\n_\n2\n_\n33\n__\n44\n__\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2g",
          "--debug",
        ],
        input: "1\t2\n2\t\n3\t1\n",
        expected: "2>\n  ^ no match for key\n3>1\n  _\n1>2\n  _\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2h",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "1\n ^ no match for key\n\n^ no match for key\n44\n  ^ no match for key\n33\n  ^ no match for key\n2\n ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1.3h",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "1\n ^ no match for key\n\n^ no match for key\n44\n  ^ no match for key\n33\n  ^ no match for key\n2\n ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1h",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "\n^ no match for key\n1\n_\n2\n_\n33\n__\n44\n__\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2g",
          "--debug",
        ],
        input: "1\t2\n2\t\n3\t1\n",
        expected: "2>\n  ^ no match for key\n3>1\n  _\n1>2\n  _\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2g",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "1\n ^ no match for key\n\n^ no match for key\n44\n  ^ no match for key\n33\n  ^ no match for key\n2\n ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1.3g",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "1\n ^ no match for key\n\n^ no match for key\n44\n  ^ no match for key\n33\n  ^ no match for key\n2\n ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1g",
          "--debug",
        ],
        input: "1\n\n44\n33\n2\n",
        expected: "\n^ no match for key\n1\n_\n2\n_\n33\n__\n44\n__\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2g",
          "--debug",
        ],
        input: "1\t2\n2\t\n3\t1\n",
        expected: "2>\n  ^ no match for key\n3>1\n  _\n1>2\n  _\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1M",
          "--debug",
        ],
        input: "FEB\n\nJAN\n",
        expected: "\n^ no match for key\nJAN\n___\nFEB\n___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2,2M",
          "--debug",
        ],
        input: "FEB\n\nJAN\n",
        expected: "FEB\n   ^ no match for key\n\n^ no match for key\nJAN\n   ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1M",
          "--debug",
        ],
        input: "FEB\nJAZZ\n\nJAN\n",
        expected: "JAZZ\n^ no match for key\n\n^ no match for key\nJAN\n___\nFEB\n___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2,2M",
          "--debug",
        ],
        input: "1\tFEB\n2\tJAZZ\n3\t\n4\tJAN\n",
        expected: "2>JAZZ\n  ^ no match for key\n3>\n  ^ no match for key\n4>JAN\n  ___\n1>FEB\n  ___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1M",
          "--debug",
        ],
        input: "FEB\nJANZ\n\nJAN\n",
        expected: "\n^ no match for key\nJANZ\n___\nJAN\n___\nFEB\n___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2,2M",
          "--debug",
        ],
        input: "1\tFEB\n2\tJANZ\n3\t\n4\tJAN\n",
        expected: "3>\n  ^ no match for key\n2>JANZ\n  ___\n4>JAN\n  ___\n1>FEB\n  ___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-g",
          "--debug",
        ],
        input: " 1.2ignore\n 1.1e4ignore\n",
        expected: " 1.2ignore\n ___\n 1.1e4ignore\n _____\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-d",
          "--debug",
        ],
        input: "\tb\n\t\ta\n",
        expected: ">>a\n___\n>b\n__\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2,2",
          "--debug",
        ],
        input: "a\n\n",
        expected: "a\n ^ no match for key\n\n^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1",
          "--debug",
        ],
        input: "b\na\n",
        expected: "a\n_\nb\n_\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "--debug",
          "-k1,1h",
        ],
        input: "-0\n1\n-2\n--Mi-1\n-3\n-0\n",
        expected: "-3\n__\n-2\n__\n-0\n__\n--Mi-1\n^ no match for key\n-0\n__\n1\n_\n",
        strip_nul: false,
      },
      {
        args: [
          "-b",
          "--debug",
        ],
        input: " 1\n1\n",
        expected: " 1\n _\n__\n1\n_\n_\n",
        strip_nul: false,
      },
      {
        args: [
          "-sb",
          "--debug",
        ],
        input: " 1\n1\n",
        expected: " 1\n _\n1\n_\n",
        strip_nul: false,
      },
      {
        args: [
          "--debug",
        ],
        input: " 1\n1\n",
        expected: " 1\n__\n1\n_\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1n",
          "--debug",
        ],
        input: "2,5\n2.4\n",
        expected: "2,5\n_\n2.4\n___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1n",
          "--debug",
        ],
        input: "2.,,3\n2.4\n",
        expected: "2.,,3\n__\n2.4\n___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1n",
          "--debug",
        ],
        input: "2,,3\n2.4\n",
        expected: "2,,3\n_\n2.4\n___\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-n",
          "-z",
          "--debug",
        ],
        input: "1a\02b\0",
        expected: "1a\n_\n2b\n_\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k2b,2",
          "--debug",
        ],
        input: "\0\ta\n",
        expected: ">a\n _\n",
        strip_nul: true,
      },
      {
        args: [
          "-s",
          "-k2.4b,2.3n",
          "--debug",
        ],
        input: "A\tchr10\nB\tchr1\n",
        expected: "A>chr10\n     ^ no match for key\nB>chr1\n     ^ no match for key\n",
        strip_nul: false,
      },
      {
        args: [
          "-s",
          "-k1.2b",
          "--debug",
        ],
        input: "1 2\n1 3\n",
        expected: "1 2\n __\n1 3\n __\n",
        strip_nul: false,
      },
    ] {
      let r = uu.invoke(s, "sort", item.args, stdin: bytes.from_text(item.input))?
      let actual = if item.strip_nul { r.stdout.utf8()?.replace("\0", with: "") } else { r.stdout as Str }
      if actual != item.expected { yield f"{item.args.join(" ")}: {actual} expected {item.expected}" }
    }

    for item in [
      {
        args: [
          "--debug",
          "-k2g",
          "-k1b,1",
        ],
        locale: "C",
        input: "   1²---++3   1,234  Mi\n",
        expected: "   1²---++3   1,234  Mi\n               _\n   _________\n________________________\n",
      },
      {
        args: [
          "--debug",
          "-k2g",
          "-k1b,1",
        ],
        locale: "fr_FR.UTF-8",
        input: "   1²---++3   1,234  Mi\n",
        expected: "   1²---++3   1,234  Mi\n              _____\n   ________\n_______________________\n",
      },
      {
        args: [
          "--debug",
          "-k1,1n",
          "-k1,1g",
          "-k1,1h",
          "-k2,2n",
          "-k2,2g",
          "-k2,2h",
          "-k3,3n",
          "-k3,3g",
          "-k3,3h",
        ],
        locale: "fr_FR.UTF-8",
        input: "+1234 1234Gi 1,234M\n",
        expected: "+1234 1234Gi 1,234M\n^ no match for key\n_____\n^ no match for key\n      ____\n      ____\n      _____\n             _____\n             _____\n             ______\n___________________\n",
      },
    ] {
      let r = uu.invoke(
        s,
        "sort",
        item.args,
        stdin: bytes.from_text(item.input),
        vars: {LC_ALL: item.locale, LC_MESSAGES: "C"},
      )?
      let normalize = rx"(?m)^\^ .*\n"
      let actual = normalize.replace(r.stdout as Str, with: "^ no match for key\n")
      if actual != item.expected { yield f"{item.locale} {item.args.join(" ")}: {actual} expected {item.expected}" }
    }
  }
  assert failures.is_empty(), failures.join("\n")
}

# origin: gnu sort/sort-discrim.log
test test_gnu_sort_sort_discrim_log { |ctx|
  let s = uu.scene(ctx)?
  test.timeout(ctx, 180s)
  let prefix = ["a" for _ in range(65)].join("")
  let expected_prefix = [prefix + [" " for _ in range(5 - f"{number}".byte_len())].join("") + f"{number}\n" for number in range(1, 10001)].join("")
  let shuffled_prefix = uu.invoke(s, "sort", ["-R"], stdin: bytes.from_text(expected_prefix))?
  uu.succeeds(shuffled_prefix)
  let prefix_result = uu.invoke(s, "sort", [], stdin: shuffled_prefix.stdout)?
  uu.succeeds(prefix_result)
  uu.stdout_is(prefix_result, expected_prefix)
  let before = "    -18446744073709551616\n    -18446744073709551615\n    -922337203685477580.1\n    -922337203685477580\n    -922337203685477580.0\n    -576460752303423480.1\n    -576460752303423480\n    -576460752303423480.0\n    -92233720368547758.071\n    -92233720368547758.07\n    -57646075230342348.71\n    -57646075230342348.7\n"
  let after = "    57646075230342348\n    57646075230342348.7\n    57646075230342348.71\n    92233720368547758\n    92233720368547758.07\n    92233720368547758.071\n    576460752303423480\n    576460752303423480.0\n    576460752303423480.1\n    922337203685477580\n    922337203685477580.0\n    922337203685477580.1\n    18446744073709551615\n    18446744073709551616\n"
  let middle = [f"{if number < 0 { "-" } else { "" }}{(if number < 0 { -number } else { number }) / 1000}.{padded((if number < 0 { -number } else { number }) % 1000, 3)}\n" for number in range(-10000, 10001)].join("")
  let expected = before + middle + after
  for option in ["-n", "-h"] {
    let shuffled = uu.invoke(s, "sort", ["-R"], stdin: bytes.from_text(expected))?
    uu.succeeds(shuffled)
    let sorted = uu.invoke(s, "sort", [option], stdin: shuffled.stdout)?
    uu.succeeds(sorted)
    uu.stdout_is(sorted, expected)
  }
}

# origin: gnu sort/sort-exit-early.log
test test_gnu_sort_sort_exit_early_log { |ctx|
  let s = uu.scene(ctx)?
  if unix.id()?.euid == 0 { test.skip("requires an unprivileged owner"); return }
  uu.touch(s, "input"); uu.set_mode(s, "input", 0o444)
  uu.fails_with_code(uu.invoke(s, "sort", ["-o", "input"], timeout: 10s)?, 2)
  uu.touch(s, "output"); uu.set_mode(s, "output", 0o200)
  uu.fails_with_code(uu.invoke(s, "sort", ["-o", "typo", "-", "output"], timeout: 10s)?, 2)
  assert ! uu.exists(s, "typo")?
}

# origin: gnu sort/sort-field-limit.log
test test_gnu_sort_sort_field_limit_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "aa\nbb\n")
  for args in [["+0.18446744073709551615R", "in"], ["+1", "-1.18446744073709551615R", "in"]] {
    let r = uu.invoke(s, "sort", args, vars: {_POSIX2_VERSION: "200809"})?
    uu.succeeds(r)
    uu.stdout_is(r, "aa\nbb\n")
  }
}

# origin: gnu sort/sort-files0-from.log
test test_gnu_sort_sort_files0_from_log { |ctx|
  let s = uu.scene(ctx)?
  let extra = uu.invoke(s, "sort", ["--files0-from=-", "no-such"], stdin: b"a")?
  uu.fails_with_code(extra, 2)
  uu.stderr_only(extra, "sort: extra operand 'no-such'\nfile operands cannot be combined with --files0-from\nTry 'sort --help' for more information.\n")
  let missing = uu.invoke(s, "sort", ["--files0-from=missing"])?
  uu.fails_with_code(missing, 2)
  uu.stderr_only(missing, "sort: open failed: missing: No such file or directory\n")
  let minus = uu.invoke(s, "sort", ["--files0-from=-"], stdin: b"-")?
  uu.fails_with_code(minus, 2)
  uu.stderr_only(minus, "sort: when reading file names from standard input, no file name of '-' allowed\n")
  uu.touch(s, "aux")
  let regular = uu.invoke(s, "sort", ["--files0-from=aux"])?
  uu.fails_with_code(regular, 2)
  uu.no_stdout(regular)
  let strip = rx"""no input from[^
]*"""
  assert strip.replace(regular.stderr as Str, with: "") == "sort: \n"
  let device = uu.invoke(s, "sort", ["--files0-from=/dev/null"])?
  uu.fails_with_code(device, 2)
  uu.stderr_only(device, "sort: no input from '/dev/null'\n")
  for data in [b"\0", b"\0\0"] {
    let r = uu.invoke(s, "sort", ["--files0-from=-"], stdin: data)?
    uu.fails_with_code(r, 2)
    uu.stderr_only(r, "sort: -:1: invalid zero-length file name\n")
  }
  uu.write(s, "g", "a")
  for item in [{data: b"g", out: "a\n"}, {data: b"g\0", out: "a\n"}, {data: b"g\0g", out: "a\na\n"}, {data: b"g\0g\0", out: "a\na\n"}] {
    let r = uu.invoke(s, "sort", ["--files0-from=-"], stdin: item.data)?
    uu.succeeds(r)
    uu.stdout_only(r, item.out)
  }
  uu.touch(s, "g")
  let empty_first = uu.invoke(s, "sort", ["--files0-from=-"], stdin: b"\0g\0")?
  uu.fails_with_code(empty_first, 2)
  uu.stderr_only(empty_first, "sort: -:1: invalid zero-length file name\n")
}

# origin: gnu sort/sort-float.log
test test_gnu_sort_sort_float_log { |ctx|
  let s = uu.scene(ctx)?
  let limits = ["1.189731495357231765e+4932", "1.7976931348623157e+308", "3.4028235e+38", "1.1754944e-38", "2.2250738585072014e-308", "3.3621031431120935063e-4932"]
  var failures = []
  let negative = ["-" + value for value in limits]
  let positive = reversed(limits)
  let canonical = [@negative, "0", @positive].join("\n") + "\n"
  let reverse_input = [@reversed(positive), "0", @reversed(negative)].join("\n") + "\n"
  # A French numeric locale stops a numeric prefix at a dot; limit strings
  # emitted with a comma retain the complete scientific value.
  let french_dots = "-3.3621031431120935063e-4932\n-3.4028235e+38\n-2.2250738585072014e-308\n-1.1754944e-38\n-1.7976931348623157e+308\n-1.189731495357231765e+4932\n0\n1.189731495357231765e+4932\n1.7976931348623157e+308\n1.1754944e-38\n2.2250738585072014e-308\n3.4028235e+38\n3.3621031431120935063e-4932\n"
  for item in [
    {locale: "C", input: reverse_input, expected: canonical},
    {locale: "fr_FR", input: reverse_input, expected: french_dots},
    {locale: "fr_FR", input: reverse_input.replace(".", with: ","), expected: canonical.replace(".", with: ",")},
  ] {
    let r = uu.invoke(s, "sort", ["-sg"], stdin: bytes.from_text(item.input), vars: {LC_ALL: item.locale})?
    uu.succeeds(r)
    if r.stdout != bytes.from_text(item.expected) { failures += [f"{item.locale}: {r.stdout as Str} expected {item.expected}"] }
  }
  let equivalent = uu.invoke(s, "sort", ["-gu"], stdin: b"10\n1e1\n")?
  assert equivalent.stdout.utf8()?.lines().collect().len() == 1
  assert failures.is_empty(), failures.join("\n")
}

# origin: gnu sort/sort-locale.log
test test_gnu_sort_sort_locale_log { |ctx|
  let s = uu.scene(ctx)?
  let failures = collect {
    for setting in ["LC_ALL", "LC_COLLATE"] {
      for item in [
        {locale: "en_US.iso8859-1", left: b"a_a", right: b"a b"},
        {locale: "en_US.iso8859-1", left: b"aaa", right: b"BBB"},
        {locale: "en_US.iso8859-1", left: b"aa\xe9", right: b"aaf"},
        {locale: "fr_FR.UTF-8", left: bytes.from_text("aaé"), right: b"aaf"},
        {locale: "fr_FR.UTF-8", left: bytes.from_text("aéY"), right: b"ae\xcc\x81Z"},
        {locale: "fr_FR.UTF-8", left: b"file1", right: b"file-2"},
      ] {
        let vars: Record = if setting == "LC_ALL" {
          {LC_ALL: item.locale}
        } else {
          {LC_ALL: "", LC_COLLATE: item.locale}
        }
        let left = item.left as Path
        let right = item.right as Path
        uu.at_bytes(s, item.left)?.write("")
        uu.at_bytes(s, item.right)?.write("")
        let input = bytes.concat([item.left, b"\n", item.right, b"\n"])
        let sorted = uu.invoke(s, "sort", [], stdin: input, vars:)?
        let listed = uu.invoke_paths(s, "ls", [p"-1", left, right], vars:)?
        uu.succeeds(sorted)
        uu.succeeds(listed)
        let locale_expected = pair_bytes(item.left, item.right, bash_path_precedes(s, left, right, vars)?)
        let c_expected = pair_bytes(item.left, item.right, bash_path_precedes(s, left, right, {LC_ALL: "C"})?)
        if sorted.stdout != locale_expected {
          yield f"{setting}={item.locale}: sort {byte_values(sorted.stdout)} expected {byte_values(locale_expected)}"
        }

        if listed.stdout != locale_expected {
          yield f"{setting}={item.locale}: ls {byte_values(listed.stdout)} expected {byte_values(locale_expected)}"
        }

        if sorted.stdout != listed.stdout { yield f"{setting}={item.locale}: sort differs from ls" }
        if locale_expected != c_expected and sorted.stdout == c_expected {
          yield f"{setting}={item.locale}: pair does not differ from C ordering"
        }

        uu.at_bytes(s, item.left)?.remove()
        uu.at_bytes(s, item.right)?.remove()
      }
    }
  }
  uu.write_bytes(s, "in", bytes.from_text("aaé\naaf\nAAF\n"))
  let c_utf8 = uu.invoke(s, "sort", ["in"], vars: {LC_ALL: "C.UTF-8"})?
  uu.succeeds(c_utf8)
  assert c_utf8.stdout == bytes.from_text("AAF\naaf\naaé\n")
  assert failures.is_empty(), failures.join("\n")
}

# origin: gnu sort/sort-merge-fdlimit.log
test test_gnu_sort_sort_merge_fdlimit_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "in")
  for number in range(1, 18) { uu.write(s, f"in/{number}", f"{number}\n") }
  uu.write(s, "some-data", sequence(1, 17))
  let files = [f"in/{number}" for number in range(1, 18)] |> sort
  let batch = shell_sort(s, ["--batch-size=20", "/dev/null"], r"""ulimit -n 19 || exit 77; exec "$@";""")?
  if batch.status == 77 { test.skip("cannot change the descriptor limit"); return }
  uu.fails_with_code(batch, 2)
  let plain = shell_sort(s, ["-m", "--batch-size=16"].extend(files), r"""ulimit -n 19 || exit 77; exec "$@";""")?
  assert plain.status == 0 or ! ("open failed" in plain.stderr as Str)
  for extra in [[], ["--random-source=some-data"]] {
    let random = shell_sort(s, ["-mR"].extend(extra).extend(["--batch-size=16"]).extend(files), r"""ulimit -n 20 || exit; exec "$@";""")?
    assert random.status == 0 or ! ("open failed" in random.stderr as Str)
  }
  uu.write(s, "out", "6\n")
  let occupied = shell_sort(s, ["-n", "-m", "--batch-size=7", "-o", "out", "out", "in/1", "in/2", "in/3", "in/4", "in/5", "out"], r"""exec 3<&- 4<&- 5<&- 6<&- 7</dev/null 8<&7 9<&7; ulimit -n 10 || exit; exec "$@";""")?
  uu.succeeds(occupied)
  uu.file_is(s, "out", sequence(1, 6) + "6\n")
}

# origin: gnu sort/sort-rand.log
test test_gnu_sort_sort_rand_log { |ctx|
  let s = uu.scene(ctx)?
  let input = sequence(1, 100)
  uu.write(s, "in", input)
  let random = uu.invoke(s, "sort", ["--random-sort", "in"])?
  uu.succeeds(random)
  assert random.stdout != bytes.from_text(input)
  let restored = uu.invoke(s, "sort", ["-n"], stdin: random.stdout)?
  uu.stdout_is(restored, input)
  let first = uu.invoke(s, "sort", ["--random-sort", "in"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  let second = uu.invoke(s, "sort", ["--random-sort", "in"], vars: {LC_ALL: "fr_FR.UTF-8"})?
  uu.succeeds(first)
  uu.succeeds(second)
  assert first.stdout != second.stdout
  let repeated_restore = uu.invoke(s, "sort", ["-n"], stdin: random.stdout)?
  uu.stdout_is(repeated_restore, input)
}

# origin: gnu sort/sort-spinlock-abuse.log
test test_gnu_sort_sort_spinlock_abuse_log { |ctx|
  let s = uu.scene(ctx)?
  test.timeout(ctx, 40s)
  uu.write(s, "in", sequence(1, 100000))
  uu.mkfifo(s, "fifo")
  let r = shell_sort(s, ["--parallel=2", "in"], r"""(i=1; while [ "$i" -le 80 ]; do read row || exit; printf '%s\n' "$i"; sleep .1; i=$((i+1)); done; cat >/dev/null) <fifo & reader=$!; trap 'kill "$reader" 2>/dev/null; wait "$reader" 2>/dev/null' EXIT; ulimit -t 7 || exit; "$@" >fifo; result=$?; wait "$reader"; exit "$result";""", timeout: 35s)?
  uu.succeeds(r)
}

# origin: gnu sort/sort-unique-segv.log
test test_gnu_sort_sort_unique_segv_log { |ctx|
  let s = uu.scene(ctx)?
  let input = ["\n" for _ in range(7)].join("") + "z\nzzzzzz\nzzzzzzz\nzzzzzzz\nzzzzzzz\nzzzzzzzzz\nzzzzzzzzzzz\nzzzzzzzzzzzz\n"
  uu.write(s, "in", input)
  let single = uu.invoke(s, "sort", ["--parallel=1", "-u", "in"])?
  uu.succeeds(single)
  let parallel = uu.invoke(s, "sort", ["--parallel=2", "-u", "-S", "10b"], stdin: bytes.from_text(input))?
  uu.succeeds(parallel)
  assert parallel.stdout == single.stdout
}

# origin: gnu sort/sort-unique.log
test test_gnu_sort_sort_unique_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "in", "1\n2\n1\n3\n")
  for locale in ["C", "fr_FR", "fr_FR.UTF-8"] {
    let r = uu.invoke(s, "sort", ["-u", "in"], vars: {LC_ALL: locale})?
    uu.succeeds(r)
    uu.stdout_is(r, "1\n2\n3\n")
  }
}

# origin: gnu sort/sort-version.log
test test_gnu_sort_sort_version_log { |ctx|
  let s = uu.scene(ctx)?
  let input = "gcc-c++-10.fc9.tar.gz\ngcc-c++-10.8.12-0.7rc2.fc9.tar.bz2\nglibc-2-0.1.beta1.fc10.rpm\nglibc-common-5-0.2.beta2.fc9.ebuild\nglibc-common-5-0.2b.deb\nglibc-common-11b.ebuild\nglibc-common-11-0.6rc2.ebuild\nlibstdc++-0.5.8.11-0.7rc2.fc10.tar.gz\nlibstdc++-4a.fc8.tar.gz\nlibstdc++-4.10.4.20040204svn.rpm\nlibstdc++-devel-3.fc8.ebuild\nlibstdc++-devel-3a.fc9.tar.gz\nlibstdc++-devel-8.fc8.deb\nlibstdc++-devel-8.6.2-0.4b.fc8\nnss_ldap-1-0.2b.fc9.tar.bz2\nnss_ldap-1-0.6rc2.fc8.tar.gz\nnss_ldap-1.0-0.1a.tar.gz\nnss_ldap-10beta1.fc8.tar.gz\nnss_ldap-10.11.8.6.20040204cvs.fc10.ebuild\nstring start 5.0.0 end of str\nstring start 5.1.0 end of str\nstring start 5.10.0 end of str\nstring start 5.2.0 end of str\nstring start 5.20.0 end of str\nstring start 5.3.0 end of str\nstring start 5.30.0 end of str\nstring start 5.4.0 end of str\nstring start 5.40.0 end of str\nstring start 5.5.0 end of str\nstring start 5.50.0 end of str\nstring start 5.6.0 end of str\nstring start 5.60.0 end of str\nstring start 5.7.0 end of str\nstring start 5.70.0 end of str\nstring start 5.8.0 end of str\nstring start 5.80.0 end of str\nstring start 5.9.0 end of str\nstring start 5.90.0 end of str\nstring start 5.04.0 end of str\n"
  let expected = "gcc-c++-10.fc9.tar.gz\ngcc-c++-10.8.12-0.7rc2.fc9.tar.bz2\nglibc-2-0.1.beta1.fc10.rpm\nglibc-common-5-0.2.beta2.fc9.ebuild\nglibc-common-5-0.2b.deb\nglibc-common-11b.ebuild\nglibc-common-11-0.6rc2.ebuild\nlibstdc++-0.5.8.11-0.7rc2.fc10.tar.gz\nlibstdc++-4a.fc8.tar.gz\nlibstdc++-4.10.4.20040204svn.rpm\nlibstdc++-devel-3.fc8.ebuild\nlibstdc++-devel-3a.fc9.tar.gz\nlibstdc++-devel-8.fc8.deb\nlibstdc++-devel-8.6.2-0.4b.fc8\nnss_ldap-1-0.2b.fc9.tar.bz2\nnss_ldap-1-0.6rc2.fc8.tar.gz\nnss_ldap-1.0-0.1a.tar.gz\nnss_ldap-10beta1.fc8.tar.gz\nnss_ldap-10.11.8.6.20040204cvs.fc10.ebuild\nstring start 5.0.0 end of str\nstring start 5.1.0 end of str\nstring start 5.2.0 end of str\nstring start 5.3.0 end of str\nstring start 5.4.0 end of str\nstring start 5.04.0 end of str\nstring start 5.5.0 end of str\nstring start 5.6.0 end of str\nstring start 5.7.0 end of str\nstring start 5.8.0 end of str\nstring start 5.9.0 end of str\nstring start 5.10.0 end of str\nstring start 5.20.0 end of str\nstring start 5.30.0 end of str\nstring start 5.40.0 end of str\nstring start 5.50.0 end of str\nstring start 5.60.0 end of str\nstring start 5.70.0 end of str\nstring start 5.80.0 end of str\nstring start 5.90.0 end of str\n"
  for suffix in ["", "0"] {
    uu.write(s, "in" + suffix, if suffix == "" { input } else { input.replace(" ", with: "\0") })
    let r = uu.invoke(s, "sort", ["--stable", "--sort=version", "-o", "out" + suffix, "in" + suffix])?
    uu.succeeds(r)
    assert uu.read_text(s, "out" + suffix)?.replace("\0", with: " ") == expected
  }
}
