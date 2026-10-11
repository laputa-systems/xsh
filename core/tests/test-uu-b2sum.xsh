##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_b2sum.rs.
##! Each test names its origin; expected values are the original assertions.

use support.uu as uu

# origin: uutils test_b2sum::b2sum::test_check
test test_uu_b2sum_b2sum_check { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "b2sum", "input.txt", "input.txt")?
  uu.fixture(s, "b2sum", "b2sum.checkfile", "b2sum.checkfile")?
  let r = uu.invoke(s, "b2sum", ["--length=512", "--check", "b2sum.checkfile"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "input.txt: OK\n")
}

# origin: uutils test_b2sum::b2sum::test_missing_file
test test_uu_b2sum_b2sum_missing_file { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "file1\n")?
  uu.write(s, "c", "file3\n")?
  let r = uu.invoke(s, "b2sum", ["--length=512", "a", "b", "c"])?
  uu.fails(r)
  uu.stdout_contains(r, "a\n")
  uu.stdout_contains(r, "c\n")
  uu.stderr_contains(r, "b: No such file or directory")
}

# origin: uutils test_b2sum::b2sum::test_single_file
test test_uu_b2sum_b2sum_single_file { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "b2sum", "input.txt", "input.txt")?
  uu.fixture(s, "b2sum", "b2sum.expected", "b2sum.expected")?
  let r = uu.invoke(s, "b2sum", ["--length=512", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.read_text(s, "b2sum.expected")? == r.stdout.utf8()?.split(" ")[0]
}

# origin: uutils test_b2sum::b2sum::test_stdin
test test_uu_b2sum_b2sum_stdin { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "b2sum", "input.txt", "input.txt")?
  uu.fixture(s, "b2sum", "b2sum.expected", "b2sum.expected")?
  let r = uu.invoke_from_path(s, "b2sum", ["--length=512"], uu.at(s, "input.txt"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.read_text(s, "b2sum.expected")? == r.stdout.utf8()?.split(" ")[0]
}

# origin: uutils test_b2sum::b2sum::test_stdin_with_dash_directory
test test_uu_b2sum_b2sum_stdin_with_dash_directory { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "b2sum", "input.txt", "input.txt")?
  uu.fixture(s, "b2sum", "b2sum.expected", "b2sum.expected")?
  uu.mkdir(s, "-")?
  let r = uu.invoke_from_path(s, "b2sum", ["--length=512"], uu.at(s, "input.txt"))?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.read_text(s, "b2sum.expected")? == r.stdout.utf8()?.split(" ")[0]
}

# origin: uutils test_b2sum::b2sum::test_zero
test test_uu_b2sum_b2sum_zero { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "b2sum", "input.txt", "input.txt")?
  uu.fixture(s, "b2sum", "b2sum.expected", "b2sum.expected")?
  let r = uu.invoke(s, "b2sum", ["--length=512", "--zero", "input.txt"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  assert uu.read_text(s, "b2sum.expected")? == r.stdout.utf8()?.split(" ")[0]
}

# origin: uutils test_b2sum::test_check_b2sum_length_option_0
test test_uu_b2sum_check_b2sum_length_option_0 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  uu.write(s, "testf.b2sum", "9e2bf63e933e610efee4a8d6cd4a9387e80860edee97e27db3b37a828d226ab1eb92a9cdd8ca9ca67a753edaf8bd89a0558496f67a30af6f766943839acf0110  testf\n")?
  let r = uu.invoke(s, "b2sum", ["--length=0", "-c", uu.at(s, "testf.b2sum").display()])?
  uu.succeeds(r)
  uu.stdout_only(r, "testf: OK\n")
}

# origin: uutils test_b2sum::test_check_b2sum_length_option_8
test test_uu_b2sum_check_b2sum_length_option_8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  uu.write(s, "testf.b2sum", "6a  testf\n")?
  let r = uu.invoke(s, "b2sum", ["--length=8", "-c", uu.at(s, "testf.b2sum").display()])?
  uu.succeeds(r)
  uu.stdout_only(r, "testf: OK\n")
}

# origin: uutils test_b2sum::test_check_b2sum_length_duplicate
test test_uu_b2sum_check_b2sum_length_duplicate { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  let r = uu.invoke(s, "b2sum", ["--length=123", "--length=128", "testf"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "d6d45901dec53e65d2b55fb6e2ab67b0")
}

# origin: uutils test_b2sum::test_check_status_reports_unusable_checksum_input
test test_uu_b2sum_check_status_reports_unusable_checksum_input { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "b2sum", ["-c", "--status"], b"not-a-checksum-line\n")?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "'standard input': no properly formatted checksum lines found")
}

# origin: uutils test_b2sum::test_invalid_b2sum_length_option_not_multiple_of_8
test test_uu_b2sum_invalid_b2sum_length_option_not_multiple_of_8 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  let r = uu.invoke(s, "b2sum", ["--length=9", uu.at(s, "testf").display()])?
  uu.fails_with_code(r, 1)
  uu.stderr_contains(r, "b2sum: invalid length: '9'")
  uu.stderr_contains(r, "b2sum: length is not a multiple of 8")
}

# origin: uutils test_b2sum::test_invalid_b2sum_length_option_too_large::case_1
test test_uu_b2sum_test_invalid_b2sum_length_option_too_large_case_1 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  let r = uu.invoke(s, "b2sum", ["--length", "513", uu.at(s, "testf").display()])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "b2sum: invalid length: '513'")
  uu.stderr_contains(r, "b2sum: maximum digest length for 'BLAKE2b' is 512 bits")
}

# origin: uutils test_b2sum::test_invalid_b2sum_length_option_too_large::case_2
test test_uu_b2sum_test_invalid_b2sum_length_option_too_large_case_2 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  let r = uu.invoke(s, "b2sum", ["--length", "1024", uu.at(s, "testf").display()])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "b2sum: invalid length: '1024'")
  uu.stderr_contains(r, "b2sum: maximum digest length for 'BLAKE2b' is 512 bits")
}

# origin: uutils test_b2sum::test_invalid_b2sum_length_option_too_large::case_3
test test_uu_b2sum_test_invalid_b2sum_length_option_too_large_case_3 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "testf", "foobar\n")?
  let r = uu.invoke(s, "b2sum", ["--length", "18446744073709552000", uu.at(s, "testf").display()])?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "b2sum: invalid length: '18446744073709552000'")
  uu.stderr_contains(r, "b2sum: maximum digest length for 'BLAKE2b' is 512 bits")
}

# origin: uutils test_b2sum::test_check_b2sum_tag_output
test test_uu_b2sum_check_b2sum_tag_output { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r0 = uu.invoke(s, "b2sum", ["--length=0", "--tag", "f"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "BLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n")
  let r1 = uu.invoke(s, "b2sum", ["--length=128", "--tag", "f"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "BLAKE2b-128 (f) = cae66941d9efbd404e4d88758ea67670\n")
}

# origin: uutils test_b2sum::test_check_b2sum_verify
test test_uu_b2sum_check_b2sum_verify { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "a", "a\n")?
  let r0 = uu.invoke(s, "b2sum", ["--tag", "a"])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "BLAKE2b (a) = bedfbb90d858c2d67b7ee8f7523be3d3b54004ef9e4f02f2ad79a1d05bfdfe49b81e3c92ebf99b504102b6bf003fa342587f5b3124c205f55204e8c4b4ce7d7c\n")
  let r1 = uu.invoke(s, "b2sum", ["--tag", "-l", "128", "a"])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "BLAKE2b-128 (a) = b93e0fc7bb21633c08bba07c5e71dc00\n")
}

# origin: uutils test_b2sum::test_invalid_arg
test test_uu_b2sum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "b2sum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_b2sum::test_check_b2sum_strict_check
test test_uu_b2sum_check_b2sum_strict_check { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let checksums = [
    "2e  f\n",
    "e4a6a0577479b2b4  f\n",
    "cae66941d9efbd404e4d88758ea67670  f\n",
    "246c0442cd564aced8145b8b60f1370aa7  f\n",
    "0e5751c026e543b2e8ab2eb06099daa1d1e5df47778f7787faab45cdf12fe3a8  f\n",
    "4ded8c5fc8b12f3273f877ca585a44ad6503249a2b345d6d9c0e67d85bcb700db4178c0303e93b8f4ad758b8e2c9fd8b3d0c28e585f1928334bb77d36782e8  f\n",
    "786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce  f\n",
  ]
  uu.write(s, "ck", checksums.join(""))?
  let output = "f: OK\nf: OK\nf: OK\nf: OK\nf: OK\nf: OK\nf: OK\n"
  let r0 = uu.invoke(s, "b2sum", ["-c", uu.at(s, "ck").display()])?
  uu.succeeds(r0)
  uu.stdout_only(r0, output)
  let r1 = uu.invoke(s, "b2sum", ["--strict", "-c", uu.at(s, "ck").display()])?
  uu.succeeds(r1)
  uu.stdout_only(r1, output)
}
