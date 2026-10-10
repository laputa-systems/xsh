type Ran = {status: Int, stdout: Str, stderr: Str}

# Runs core/factor.xsh by its real path (so the invoked name is `factor` and
# `lib.gnu` resolves beside it) with `input` as standard input, capturing both
# streams to files.
proc factor_run(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Ran] {
  let root = test.temp_dir(ctx, name: "factor")?
  let stdin = test.temp_file(ctx, name: "factor-stdin", contents: input)?
  let out = fp"{root}/stdout"
  let err = fp"{root}/stderr"
  let script = fp"{ctx.core_dir}/factor.xsh"
  let argv = [ctx.xsh_bin.display(), script.display()].extend(args)
  let plan = process.command_argv(ctx.xsh_bin, argv, root, {LC_ALL: "C"}, stdin, out, err)
  let status = process.run(plan)?
  Ok({status: status.exit_code()?, stdout: out.read_bytes()?.utf8() ?? "", stderr: err.read_text()?})
}

proc factor_text(ctx: TestContext, args: List[Str], input = b"") [fs, process, error] -> Result[Str] {
  let result = factor_run(ctx, args, input)?
  assert result.status == 0, f"factor {args.join(" ")}: {result.stderr}"
  Ok(result.stdout)
}

test test_factor_small_numbers { |ctx|
  assert factor_text(ctx, ["0", "1", "2", "12", "97", "1001"])? == "0:\n1:\n2: 2\n12: 2 2 3\n97: 97\n1001: 7 11 13\n"
  assert factor_text(ctx, ["3", "6", " +9"])? == "3: 3\n6: 2 3\n9: 3 3\n", "arguments may carry blanks and a plus sign"
  assert factor_text(ctx, ["0012"])? == "12: 2 2 3\n", "the number prints in canonical form"
}

test test_factor_exponents { |ctx|
  assert factor_text(ctx, ["-h", "1234", "10240"])? == "1234: 2 617\n10240: 2^11 5\n"
  assert factor_text(ctx, ["--exponents", "360"])? == "360: 2^3 3^2 5\n"
  assert factor_text(ctx, ["-hh", "1234", "10240"])? == "1234: 2 617\n10240: 2^11 5\n", "repeating an option is legal"
  assert factor_text(ctx, ["--expo", "8"])? == "8: 2^3\n"
}

test test_factor_reads_standard_input_when_there_are_no_operands { |ctx|
  assert factor_text(ctx, [], b"12 15\n7\n")? == "12: 2 2 3\n15: 3 5\n7: 7\n"
  assert factor_text(ctx, [], b"42\0")? == "42: 2 3 7\n", "a trailing NUL is ignored"
  assert factor_text(ctx, [], b"\t 6 \t\n\n9")? == "6: 2 3\n9: 3 3\n"

  let thousand = bytes.from_text([f"{n}" for n in range(0, 11)].join(" ") + "\n")
  assert factor_text(ctx, [], thousand)? == "0:\n1:\n2: 2\n3: 3\n4: 2 2\n5: 5\n6: 2 3\n7: 7\n8: 2 2 2\n9: 3 3\n10: 2 5\n"
}

test test_factor_large_numbers { |ctx|
  assert factor_text(ctx, ["4611686018427387896"])? == "4611686018427387896: 2 2 2 179951 3203431780337\n"
  assert factor_text(ctx, ["18446744073709551557"])? == "18446744073709551557: 18446744073709551557\n"
  assert factor_text(ctx, ["18446744073709551616"])? == "18446744073709551616: 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2 2\n"
  assert factor_text(ctx, ["158909489063877810457"])? == "158909489063877810457: 3401347 3861211 12099721\n"
  assert factor_text(ctx, ["222087527029934481871"])? == "222087527029934481871: 15601 26449 111427 4830277\n"
  assert factor_text(ctx, ["12847291069740315094892340035"])? == "12847291069740315094892340035: 5 4073 18899 522591721 63874247821\n"
  assert factor_text(ctx, ["-h", "340282366920938463463374607431768211456"])? == "340282366920938463463374607431768211456: 2^128\n"
  assert factor_text(ctx, ["+170141183460469231731687303715884105729"])? == "170141183460469231731687303715884105729: 3 56713727820156410577229101238628035243\n"
  assert factor_text(ctx, ["-h", "56539106683390492137844827055225747632151249167945695848217966183182073341"])? == "56539106683390492137844827055225747632151249167945695848217966183182073341: 34359738421^7\n", "prime powers are found by root extraction"
}

test test_factor_invalid_numbers_are_reported_and_skipped { |ctx|
  let args = factor_run(ctx, ["a", "4"])?
  assert args.status == 1
  assert args.stdout == "4: 2 2\n"
  assert args.stderr == "factor: 'a' is not a valid positive integer\n", args.stderr

  let words = factor_run(ctx, ["x1", "7.5", ""])?
  assert words.status == 1
  assert words.stdout == ""
  assert words.stderr == "factor: 'x1' is not a valid positive integer\nfactor: '7.5' is not a valid positive integer\nfactor: '' is not a valid positive integer\n", words.stderr

  let piped = factor_run(ctx, [], b"6 9\0abc\0 +4 \xff 3 1.5\n")?
  assert piped.status == 1
  assert piped.stdout == "6: 2 3\n9: 3 3\n4: 2 2\n3: 3\n", piped.stdout
  assert piped.stderr == "factor: '\\377' is not a valid positive integer\nfactor: '1.5' is not a valid positive integer\n", piped.stderr
}

test test_factor_options_follow_gnu_grammar { |ctx|
  let unknown = factor_run(ctx, ["-1"])?
  assert unknown.status == 1
  assert unknown.stderr == "factor: invalid option -- '1'\nTry 'factor --help' for more information.\n", unknown.stderr

  let help = factor_run(ctx, ["--help"])?
  assert help.status == 0
  assert "Usage: factor [OPTION] [NUMBER]..." in help.stdout

  let version = factor_run(ctx, ["--version"])?
  assert version.stdout.starts_with("factor")
}

test test_factor_clustered_factors { |ctx|
  assert factor_text(ctx, ["1000112004278059472142857"])? == "1000112004278059472142857: 1000003 1000033 1000037 1000039\n", "primes of one size cluster are all found"
  assert factor_text(ctx, ["-h", "1000148008409224570707382030594142843"])? == "1000148008409224570707382030594142843: 1000003^2 1000033^2 1000037 1000039\n", "repeated clustered factors keep their exponents"
}
