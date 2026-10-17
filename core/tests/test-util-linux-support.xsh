use support.uu
use support.util_linux as ul

test test_util_linux_comparison_rejects_observable_mutants { |_ctx|
  let reference: uu.Ran = {util: "lslocks", args: ["--raw"], status: 0,
    stdout: b"holder=21 blocker=22 inode=101\n", stderr: b""}
  assert ul.equal(reference, reference, reference_state: b"owned=101", candidate_state: b"owned=101")
  for mutant in [
    {...reference, stdout: b"holder=21 blocker=22 inode=101 \n"},
    {...reference, stderr: b"warning\n"},
    {...reference, status: 1},
    {...reference, stdout: b"holder=22 blocker=21 inode=101\n"},
    {...reference, stdout: b"holder=21 blocker=22 inode=102\n"},
    {...reference, stdout: b"holder=PID blocker=PID inode=INODE\n"},
  ] {
    assert ! ul.equal(reference, mutant)
  }
  assert ! ul.equal(reference, reference, reference_state: b"owned=101", candidate_state: b"owned=102")
}

test test_util_linux_reference_preserves_raw_path_arguments { |ctx|
  let s = uu.scene(ctx)?
  let operand = uu.at_bytes(s, b"raw-\xff")?
  operand.write("fixture")
  let result = ul.reference_paths(s, "mountpoint", [p"--quiet", operand])?
  assert result.status == 32
  assert result.stdout == b"" and result.stderr == b""
}
