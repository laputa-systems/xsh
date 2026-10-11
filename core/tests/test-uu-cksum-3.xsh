##! Transcribed checksum verification tests from the uutils integration suite.

use support.uu as uu

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_1___sha2__::len_1_224
test test_uu_cksum_test_check_tagged_sha_single_file_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha224_single_file.expected", "sha224_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha224_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_1___sha2__::len_2_256
test test_uu_cksum_test_check_tagged_sha_single_file_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha256_single_file.expected", "sha256_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha256_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_1___sha2__::len_3_384
test test_uu_cksum_test_check_tagged_sha_single_file_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha384_single_file.expected", "sha384_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha384_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_1___sha2__::len_4_512
test test_uu_cksum_test_check_tagged_sha_single_file_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha512_single_file.expected", "sha512_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha512_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_2___sha3__::len_1_224
test test_uu_cksum_test_check_tagged_sha_single_file_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha3_224_single_file.expected", "sha3_224_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_224_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_2___sha3__::len_2_256
test test_uu_cksum_test_check_tagged_sha_single_file_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha3_256_single_file.expected", "sha3_256_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_256_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_2___sha3__::len_3_384
test test_uu_cksum_test_check_tagged_sha_single_file_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha3_384_single_file.expected", "sha3_384_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_384_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_single_file::algo_2___sha3__::len_4_512
test test_uu_cksum_test_check_tagged_sha_single_file_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "sha3_512_single_file.expected", "sha3_512_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_512_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_1___sha2__::len_1_224
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha224_multiple_files.expected", "sha224_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha224_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_1___sha2__::len_2_256
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha256_multiple_files.expected", "sha256_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha256_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_1___sha2__::len_3_384
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha384_multiple_files.expected", "sha384_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha384_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_1___sha2__::len_4_512
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha512_multiple_files.expected", "sha512_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha512_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_2___sha3__::len_1_224
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha3_224_multiple_files.expected", "sha3_224_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_224_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_2___sha3__::len_2_256
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha3_256_multiple_files.expected", "sha3_256_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_256_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_2___sha3__::len_3_384
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha3_384_multiple_files.expected", "sha3_384_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_384_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_tagged_sha_multiple_files::algo_2___sha3__::len_4_512
test test_uu_cksum_test_check_tagged_sha_multiple_files_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.fixture(s, "cksum", "sha3_512_multiple_files.expected", "sha3_512_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "sha3_512_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_1___sha2__::len_1_224
test test_uu_cksum_test_check_untagged_sha_single_file_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha224_single_file.expected", "untagged/sha224_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha224_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_1___sha2__::len_2_256
test test_uu_cksum_test_check_untagged_sha_single_file_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha256_single_file.expected", "untagged/sha256_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha256_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_1___sha2__::len_3_384
test test_uu_cksum_test_check_untagged_sha_single_file_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha384_single_file.expected", "untagged/sha384_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha384_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_1___sha2__::len_4_512
test test_uu_cksum_test_check_untagged_sha_single_file_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha512_single_file.expected", "untagged/sha512_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha512_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_2___sha3__::len_1_224
test test_uu_cksum_test_check_untagged_sha_single_file_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_224_single_file.expected", "untagged/sha3_224_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_224_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_2___sha3__::len_2_256
test test_uu_cksum_test_check_untagged_sha_single_file_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_256_single_file.expected", "untagged/sha3_256_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_256_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_2___sha3__::len_3_384
test test_uu_cksum_test_check_untagged_sha_single_file_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_384_single_file.expected", "untagged/sha3_384_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_384_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_single_file::algo_2___sha3__::len_4_512
test test_uu_cksum_test_check_untagged_sha_single_file_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_512_single_file.expected", "untagged/sha3_512_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_512_single_file.expected"])?
  uu.succeeds(r)
  uu.stdout_is(r, "lorem_ipsum.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_1___sha2__::len_1_224
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_1___sha2___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha224_multiple_files.expected", "untagged/sha224_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha224_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_1___sha2__::len_2_256
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_1___sha2___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha256_multiple_files.expected", "untagged/sha256_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha256_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_1___sha2__::len_3_384
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_1___sha2___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha384_multiple_files.expected", "untagged/sha384_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha384_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_1___sha2__::len_4_512
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_1___sha2___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha512_multiple_files.expected", "untagged/sha512_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2", "untagged/sha512_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_2___sha3__::len_1_224
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_2___sha3___len_1_224 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_224_multiple_files.expected", "untagged/sha3_224_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_224_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_2___sha3__::len_2_256
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_2___sha3___len_2_256 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_256_multiple_files.expected", "untagged/sha3_256_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_256_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_2___sha3__::len_3_384
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_2___sha3___len_3_384 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_384_multiple_files.expected", "untagged/sha3_384_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_384_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_multiple_files::algo_2___sha3__::len_4_512
test test_uu_cksum_test_check_untagged_sha_multiple_files_algo_2___sha3___len_4_512 { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "alice_in_wonderland.txt", "alice_in_wonderland.txt")?
  uu.mkdir(s, "untagged")?
  uu.fixture(s, "cksum", "untagged/sha3_512_multiple_files.expected", "untagged/sha3_512_multiple_files.expected")?
  let r = uu.invoke(s, "cksum", ["--check", "--algorithm=sha3", "untagged/sha3_512_multiple_files.expected"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "lorem_ipsum.txt: OK\n")
  uu.stdout_contains(r, "alice_in_wonderland.txt: OK\n")
}

# origin: uutils test_cksum::test_check_untagged_sha_invalid_length::case_1_sha2
test test_uu_cksum_test_check_untagged_sha_invalid_length_case_1_sha2 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.touch(s, "c")?
  uu.touch(s, "d")?
  uu.touch(s, "e")?
  let r = uu.invoke(s, "cksum", ["-a", "sha2", "-c"], stdin: bytes.from_text("d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f  a\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  b\n38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b  c\ncf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e  d\nxxxx  e"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  uu.stdout_contains(r, "b: OK")
  uu.stdout_contains(r, "c: OK")
  uu.stdout_contains(r, "d: OK")
  assert !("e: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_sha_invalid_length::case_2_sha3
test test_uu_cksum_test_check_untagged_sha_invalid_length_case_2_sha3 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  uu.touch(s, "c")?
  uu.touch(s, "d")?
  uu.touch(s, "e")?
  let r = uu.invoke(s, "cksum", ["-a", "sha3", "-c"], stdin: bytes.from_text("6b4e03423667dbb73b6e15454f0eb1abd4597f9a1b078e3f5b5a6bc7  a\na7ffc6f8bf1ed76651c14756a061d662f580ff4de43b49fa82d80a4b80f8434a  b\n0c63a75b845e4f7d01107d852e4c2485c51a50aaaa94fc61995e71bbee983a2ac3713831264adb47fb6bd1e058d5f004  c\na69f73cca23a9ac5c8b567dc185a756e97c982164fe25859e0d1dcc1475c80a615b2123af1f5f94c11e3e9402c3ac558f500199d95b6d3e301758586281dcd26  d\nxxxx  e"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  uu.stdout_contains(r, "b: OK")
  uu.stdout_contains(r, "c: OK")
  uu.stdout_contains(r, "d: OK")
  assert !("e: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_1_md5
test test_uu_cksum_test_check_untagged_with_invalid_length_case_1_md5 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "md5", "-c"], stdin: bytes.from_text("e3b0  b\nd41d8cd98f00b204e9800998ecf8427e  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_2_sha1
test test_uu_cksum_test_check_untagged_with_invalid_length_case_2_sha1 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "sha1", "-c"], stdin: bytes.from_text("e3b0  b\nda39a3ee5e6b4b0d3255bfef95601890afd80709  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_3_sha224
test test_uu_cksum_test_check_untagged_with_invalid_length_case_3_sha224 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "sha224", "-c"], stdin: bytes.from_text("e3b0  b\nd14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_4_sha256
test test_uu_cksum_test_check_untagged_with_invalid_length_case_4_sha256 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "sha256", "-c"], stdin: bytes.from_text("e3b0  b\ne3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_5_sha384
test test_uu_cksum_test_check_untagged_with_invalid_length_case_5_sha384 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "sha384", "-c"], stdin: bytes.from_text("e3b0  b\n38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_6_sha512
test test_uu_cksum_test_check_untagged_with_invalid_length_case_6_sha512 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "sha512", "-c"], stdin: bytes.from_text("e3b0  b\ncf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_untagged_with_invalid_length::case_7_sm3
test test_uu_cksum_test_check_untagged_with_invalid_length_case_7_sm3 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-a", "sm3", "-c"], stdin: bytes.from_text("e3b0  b\n1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b  a"))?
  uu.succeeds(r)
  uu.stdout_contains(r, "a: OK")
  assert !("b: FAILED" in r.stdout.utf8()?)
  uu.stderr_contains(r, "WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_sha2_tagged_variant
test test_uu_cksum_check_sha2_tagged_variant { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r0_0 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA224 (f) = d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f"))?
  uu.succeeds(r0_0)
  uu.stdout_is(r0_0, "f: OK\n")
  let r0_1 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA2-224 (f) = d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f"))?
  uu.succeeds(r0_1)
  uu.stdout_is(r0_1, "f: OK\n")
  let r1_0 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA256 (f) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"))?
  uu.succeeds(r1_0)
  uu.stdout_is(r1_0, "f: OK\n")
  let r1_1 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA2-256 (f) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"))?
  uu.succeeds(r1_1)
  uu.stdout_is(r1_1, "f: OK\n")
  let r2_0 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA384 (f) = 38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b"))?
  uu.succeeds(r2_0)
  uu.stdout_is(r2_0, "f: OK\n")
  let r2_1 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA2-384 (f) = 38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b"))?
  uu.succeeds(r2_1)
  uu.stdout_is(r2_1, "f: OK\n")
  let r3_0 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA512 (f) = cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"))?
  uu.succeeds(r3_0)
  uu.stdout_is(r3_0, "f: OK\n")
  let r3_1 = uu.invoke(s, "cksum", ["--check", "--algorithm=sha2"], stdin: bytes.from_text("SHA2-512 (f) = cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e"))?
  uu.succeeds(r3_1)
  uu.stdout_is(r3_1, "f: OK\n")
}

# origin: uutils test_cksum::test_check_sha2_tagged_missing_hint
test test_uu_cksum_check_sha2_tagged_missing_hint { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  uu.touch(s, "b")?
  let r = uu.invoke(s, "cksum", ["-c"], stdin: bytes.from_text("SHA2 (a) = d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f\nSHA2 (b) = xxxx"))?
  uu.fails(r)
  uu.stderr_contains(r, "no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_tagged_missing_algo
test test_uu_cksum_check_tagged_missing_algo { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cksum", ["-c"], stdin: bytes.from_text("(a) = d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f\na) = d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f\n(a = d14a028c2a3a2bc9476102bb288234c415a2b01f828ea62ac5b3e42f"))?
  uu.fails(r)
  uu.stderr_contains(r, "no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_incorrectly_formatted_checksum_keeps_processing_b64
test test_uu_cksum_check_incorrectly_formatted_checksum_keeps_processing_b64 { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r0 = uu.invoke(s, "cksum", ["--check"], stdin: bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg==\nMD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg="))?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "f: OK")
  uu.stderr_contains(r0, "cksum: WARNING: 1 line is improperly formatted")
  
  let r1 = uu.invoke(s, "cksum", ["--check"], stdin: bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg=\nMD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg=="))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "f: OK")
  uu.stderr_contains(r1, "cksum: WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_incorrectly_formatted_checksum_keeps_processing_hex
test test_uu_cksum_check_incorrectly_formatted_checksum_keeps_processing_hex { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r0 = uu.invoke(s, "cksum", ["--check"], stdin: bytes.from_text("MD5 (f) = d41d8cd98f00b204e9800998ecf8427e\nMD5 (f) = d41d8cd98f00b204e9800998ecf8427"))?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "f: OK")
  uu.stderr_contains(r0, "cksum: WARNING: 1 line is improperly formatted")
  
  let r1 = uu.invoke(s, "cksum", ["--check"], stdin: bytes.from_text("MD5 (f) = d41d8cd98f00b204e9800998ecf8427\nMD5 (f) = d41d8cd98f00b204e9800998ecf8427e"))?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "f: OK")
  uu.stderr_contains(r1, "cksum: WARNING: 1 line is improperly formatted")
}

# origin: uutils test_cksum::test_check_md5_format
test test_uu_cksum_check_md5_format { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "empty")?
  uu.write(s, "f", "d41d8cd98f00b204e9800998ecf8427e *empty\n")?
  let r = uu.invoke(s, "cksum", ["-a", "md5", "--check", "f"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "empty: OK")
  uu.write(s, "not-empty", "42")?
  uu.write(s, "f2", "a1d0c6e83f027327d8461063f4ac58a6 *not-empty\n")?
  let r2 = uu.invoke(s, "cksum", ["-a", "md5", "--check", "f", "f2"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "empty: OK")
  uu.stdout_contains(r2, "not-empty: OK")
}

# origin: uutils test_cksum::test_check_mix_hex_base64
test test_uu_cksum_check_mix_hex_base64 { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo1.dat", "foo")?
  uu.write(s, "foo2.dat", "foo")?
  uu.write(s, "hex_b64", "BLAKE2b-128 (foo2.dat) = 04136e24f85d470465c3db66e58ed56c\nBLAKE2b-128 (foo1.dat) = BBNuJPhdRwRlw9tm5Y7VbA==")?
  uu.write(s, "b64_hex", "BLAKE2b-128 (foo1.dat) = BBNuJPhdRwRlw9tm5Y7VbA==\nBLAKE2b-128 (foo2.dat) = 04136e24f85d470465c3db66e58ed56c")?
  let r0 = uu.invoke(s, "cksum", ["--check", uu.at(s, "hex_b64").display()])?
  uu.succeeds(r0)
  uu.stdout_only(r0, "foo2.dat: OK\nfoo1.dat: OK\n")
  let r1 = uu.invoke(s, "cksum", ["--check", uu.at(s, "b64_hex").display()])?
  uu.succeeds(r1)
  uu.stdout_only(r1, "foo1.dat: OK\nfoo2.dat: OK\n")
}

# origin: uutils test_cksum::test_check_pipe
test test_uu_cksum_check_pipe { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r = uu.invoke(s, "cksum", ["--check", "-"], stdin: bytes.from_text("f"))?
  uu.fails_with_code(r, 1)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cksum: 'standard input': no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_unknown_checksum_file
test test_uu_cksum_check_unknown_checksum_file { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cksum", ["--check", "missing"])?
  uu.fails(r)
  uu.stderr_only(r, "cksum: missing: No such file or directory\n")
}

# origin: uutils test_cksum::test_check_trailing_space_fails
test test_uu_cksum_check_trailing_space_fails { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "foo", "foo-content\n")?
  uu.write(s, "CHECKSUM", "SHA1 (foo) = 058ab38dd3603703b3a7063cf95dc51a4286b6fe    \n")?
  let r = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "CHECKSUM: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_check_failed_to_read
test test_uu_cksum_check_failed_to_read { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "CHECKSUM", "SHA1 (dir) = ffffffffffffffffffffffffffffffffffffffff\nSHA1 (not-file) = ffffffffffffffffffffffffffffffffffffffff\n")?
  uu.mkdir(s, "dir")?
  let r0 = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.fails(r0)
  uu.stdout_is(r0, "dir: FAILED open or read\nnot-file: FAILED open or read\n")
  uu.stderr_contains(r0, "cksum: WARNING: 2 listed files could not be read")
  
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUM", "--ignore-missing"])?
  uu.fails(r1)
  uu.stdout_is(r1, "dir: FAILED open or read\n")
  uu.stderr_contains(r1, "cksum: WARNING: 1 listed file could not be read")
}

# origin: uutils test_cksum::test_cksum_check_empty_line
test test_uu_cksum_cksum_check_empty_line { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "CHECKSUM", "SHA384 (f) = 38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b\nBLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\nBLAKE2b-384 (f) = b32811423377f52d7862286ee1a72ee540524380fda1724a6f25d7978c6fd3244a6caf0498812673c5e05ef583825100\nSM3 (f) = 1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b\n\n")?
  let r = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "f: OK\nf: OK\nf: OK\nf: OK\n")
  assert !("line is improperly formatted" in r.stderr.utf8()?)
}

# origin: uutils test_cksum::test_cksum_check_space
test test_uu_cksum_cksum_check_space { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "CHECKSUM", "SHA384 (f) = 38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b\nBLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\nBLAKE2b-384 (f) = b32811423377f52d7862286ee1a72ee540524380fda1724a6f25d7978c6fd3244a6caf0498812673c5e05ef583825100\nSM3 (f) = 1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b\n  \n")?
  let r = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "f: OK\nf: OK\nf: OK\nf: OK\n")
  uu.stderr_contains(r, "line is improperly formatted")
}

# origin: uutils test_cksum::test_cksum_check_leading_info
test test_uu_cksum_cksum_check_leading_info { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "CHECKSUM", "\\SHA384 (f) = 38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b\n\\BLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\n\\BLAKE2b-384 (f) = b32811423377f52d7862286ee1a72ee540524380fda1724a6f25d7978c6fd3244a6caf0498812673c5e05ef583825100\n\\SM3 (f) = 1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b\n")?
  let r = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "f: OK\nf: OK\nf: OK\nf: OK\n")
}

# origin: uutils test_cksum::test_cksum_check
test test_uu_cksum_cksum_check { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "CHECKSUM", "SHA384 (f) = 38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b\nBLAKE2b (f) = 786a02f742015903c6c6fd852552d272912f4740e15847618a86e217f71f5419d25e1031afee585313896444934eb04b903a685b1448b755d56f701afe9be2ce\nBLAKE2b-384 (f) = b32811423377f52d7862286ee1a72ee540524380fda1724a6f25d7978c6fd3244a6caf0498812673c5e05ef583825100\nSM3 (f) = 1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b\n")?
  let r0 = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.succeeds(r0)
  uu.stdout_contains(r0, "f: OK\nf: OK\nf: OK\nf: OK\n")
  assert !("line is improperly formatted" in r0.stderr.utf8()?)
  
  let r1 = uu.invoke(s, "cksum", ["--check", "--strict", "CHECKSUM"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "f: OK\nf: OK\nf: OK\nf: OK\n")
  assert !("line is improperly formatted" in r1.stderr.utf8()?)
  
  uu.append(s, "CHECKSUM", "incorrect data")?
  let r2 = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.succeeds(r2)
  uu.stdout_contains(r2, "f: OK\nf: OK\nf: OK\nf: OK\n")
  uu.stderr_contains(r2, "line is improperly formatted")
  
  let r3 = uu.invoke(s, "cksum", ["--check", "--strict", "CHECKSUM"])?
  uu.fails(r3)
  uu.stdout_contains(r3, "f: OK\nf: OK\nf: OK\nf: OK\n")
  uu.stderr_contains(r3, "line is improperly formatted")
  
}

# origin: uutils test_cksum::test_cksum_check_case
test test_uu_cksum_cksum_check_case { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.write(s, "CHECKSUM", "Sha1 (f) = da39a3ee5e6b4b0d3255bfef95601890afd80709\n")?
  let r = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.fails(r)
}

# origin: uutils test_cksum::test_cksum_check_invalid
test test_uu_cksum_cksum_check_invalid { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.touch(s, "CHECKSUM")?
  let genCHECKSUM = uu.invoke(s, "cksum", ["-a", "sha384", "f"])?
  uu.succeeds(genCHECKSUM)
  uu.append(s, "CHECKSUM", genCHECKSUM.stdout.utf8()?)?
  uu.append(s, "CHECKSUM", "again incorrect data\naze\n")?
  let r0 = uu.invoke(s, "cksum", ["--check", "--strict", "CHECKSUM"])?
  uu.fails(r0)
  uu.stdout_contains(r0, "f: OK\n")
  uu.stderr_contains(r0, "2 lines")
  
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.succeeds(r1)
  uu.stdout_contains(r1, "f: OK\n")
  uu.stderr_contains(r1, "2 lines")
  
}

# origin: uutils test_cksum::test_cksum_check_failed
test test_uu_cksum_cksum_check_failed { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.touch(s, "CHECKSUM")?
  let genCHECKSUM = uu.invoke(s, "cksum", ["-a", "sha384", "f"])?
  uu.succeeds(genCHECKSUM)
  uu.append(s, "CHECKSUM", genCHECKSUM.stdout.utf8()?)?
  uu.append(s, "CHECKSUM", "again incorrect data\naze\nSM3 (input) = 7cfb120d4fabea2a904948538a438fdb57c725157cb40b5aee8d937b8351477e\n")?
  let r0 = uu.invoke(s, "cksum", ["--check", "--strict", "CHECKSUM"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "input: No such file or directory")
  uu.stderr_contains(r0, "2 lines are improperly formatted\n")
  uu.stderr_contains(r0, "1 listed file could not be read\n")
  uu.stdout_contains(r0, "f: OK\n")
  
  let r1 = uu.invoke(s, "cksum", ["--check", "CHECKSUM"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "input: No such file or directory")
  uu.stderr_contains(r1, "2 lines are improperly formatted\n")
  uu.stderr_contains(r1, "1 listed file could not be read\n")
  uu.stdout_contains(r1, "f: OK\n")
  
  uu.touch(s, "CHECKSUM2")?
  uu.write(s, "f2", "42")?
  let genCHECKSUM2 = uu.invoke(s, "cksum", ["-a", "sha384", "f2"])?
  uu.succeeds(genCHECKSUM2)
  uu.append(s, "CHECKSUM2", genCHECKSUM2.stdout.utf8()?)?
  uu.append(s, "CHECKSUM2", "again incorrect data\naze\nSM3 (input2) = 7cfb120d4fabea2a904948538a438fdb57c725157cb40b5aee8d937b8351477e\nagain incorrect data\naze\nSM3 (input2) = 7cfb120d4fabea2a904948538a438fdb57c725157cb40b5aee8d937b8351477e\n")?
  let r2 = uu.invoke(s, "cksum", ["--check", "CHECKSUM", "CHECKSUM2"])?
  uu.fails(r2)
  uu.stderr_contains(r2, "input2: No such file or directory")
  uu.stderr_contains(r2, "4 lines are improperly formatted\n")
  uu.stderr_contains(r2, "2 listed files could not be read\n")
  uu.stdout_contains(r2, "f: OK\n")
  uu.stdout_contains(r2, "2: OK\n")
}

# origin: uutils test_cksum::test_cksum_mixed
test test_uu_cksum_cksum_mixed { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  uu.touch(s, "CHECKSUM")?
  let gen0 = uu.invoke(s, "cksum", ["-a", "sha384", "f"])?
  uu.succeeds(gen0)
  uu.append(s, "CHECKSUM", gen0.stdout.utf8()?)?
  let gen1 = uu.invoke(s, "cksum", ["-a", "blake2b", "f"])?
  uu.succeeds(gen1)
  uu.append(s, "CHECKSUM", gen1.stdout.utf8()?)?
  let gen2 = uu.invoke(s, "cksum", ["-a", "blake2b", "-l", "384", "f"])?
  uu.succeeds(gen2)
  uu.append(s, "CHECKSUM", gen2.stdout.utf8()?)?
  let gen3 = uu.invoke(s, "cksum", ["-a", "sm3", "f"])?
  uu.succeeds(gen3)
  uu.append(s, "CHECKSUM", gen3.stdout.utf8()?)?
  let r = uu.invoke(s, "cksum", ["--check", "-a", "sm3", "CHECKSUM"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "f: OK")
  uu.stderr_contains(r, "3 lines are improperly formatted")
}

# origin: uutils test_cksum::test_cksum_garbage
test test_uu_cksum_cksum_garbage { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "check-file", "garbage MD5 (README.md) = e5773576fc75ff0f8eba14f61587ae28")?
  let r0 = uu.invoke(s, "cksum", ["--check", "check-file"])?
  uu.fails(r0)
  uu.stderr_contains(r0, "check-file: no properly formatted checksum lines found")
  
  uu.write(s, "check-file", "MD5 (README.md) = e5773576fc75ff0f8eba14f61587ae28 garbage")?
  let r1 = uu.invoke(s, "cksum", ["--check", "check-file"])?
  uu.fails(r1)
  uu.stderr_contains(r1, "check-file: no properly formatted checksum lines found")
}

# origin: uutils test_cksum::test_empty_file
test test_uu_cksum_empty_file { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a")?
  let r = uu.invoke(s, "cksum", ["a"])?
  uu.succeeds(r)
  uu.no_stderr(r)
  uu.stdout_is(r, "4294967295 0 a\n")
}

# origin: uutils test_cksum::test_dev_null
test test_uu_cksum_dev_null { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cksum", ["--tag", "--untagged", "--algorithm=md5", "/dev/null"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "d41d8cd98f00b204e9800998ecf8427e ")
}

# origin: uutils test_cksum::test_invalid_arg
test test_uu_cksum_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "cksum", ["--definitely-invalid"])?
  uu.fails_with_code(r, 1)
}

# origin: uutils test_cksum::test_crc_for_bigger_than_32_bytes
test test_uu_cksum_crc_for_bigger_than_32_bytes { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "chars.txt", "chars.txt")?
  let r = uu.invoke(s, "cksum", ["chars.txt"])?
  uu.succeeds(r)
  let words = r.stdout.utf8()?.split(" ")
  assert words[0] == "586047089"
  assert words[1] == "16"
}

# origin: uutils test_cksum::test_fail_on_folder
test test_uu_cksum_fail_on_folder { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  let r = uu.invoke(s, "cksum", ["a_folder"])?
  uu.fails(r)
  uu.no_stdout(r)
  uu.stderr_contains(r, "cksum: a_folder: Is a directory")
}

# origin: uutils test_cksum::test_folder_and_file
test test_uu_cksum_folder_and_file { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "a_folder")?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "crc_single_file.expected", "crc_single_file.expected")?
  let r = uu.invoke(s, "cksum", ["a_folder", "lorem_ipsum.txt"])?
  uu.fails(r)
  uu.stderr_contains(r, "cksum: a_folder: Is a directory")
  uu.stdout_is_bytes(r, uu.read(s, "crc_single_file.expected")?)
}

# origin: uutils test_cksum::test_conflicting_options
test test_uu_cksum_conflicting_options { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "f")?
  let r0 = uu.invoke(s, "cksum", ["--binary", "--check", "f"])?
  uu.fails_with_code(r0, 1)
  uu.no_stdout(r0)
  uu.stderr_contains(r0, "cksum: the --binary and --text options are meaningless when verifying checksums")
  
  let r1 = uu.invoke(s, "cksum", ["--tag", "-c", "-a", "md5"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: the --tag option is meaningless when verifying checksums")
  
}

# origin: uutils test_cksum::test_length_with_wrong_algorithm
test test_uu_cksum_length_with_wrong_algorithm { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r0 = uu.invoke(s, "cksum", ["--length=16", "--algorithm=md5", "lorem_ipsum.txt"])?
  uu.fails_with_code(r0, 1)
  uu.no_stdout(r0)
  uu.stderr_contains(r0, "cksum: --length is only supported with --algorithm blake2b, sha2, or sha3")
  
  let r1 = uu.invoke(s, "cksum", ["--length=16", "--algorithm=md5", "-c", "foo.sums"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: --length is only supported with --algorithm blake2b, sha2, or sha3")
}

# origin: uutils test_cksum::test_length_not_supported
test test_uu_cksum_length_not_supported { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  let r0 = uu.invoke(s, "cksum", ["--length=15", "lorem_ipsum.txt"])?
  uu.fails_with_code(r0, 1)
  uu.no_stdout(r0)
  uu.stderr_contains(r0, "cksum: --length is only supported with --algorithm blake2b, sha2, or sha3")
  
  let r1 = uu.invoke(s, "cksum", ["-l", "158", "-c", "-a", "crc", "/tmp/xxx"])?
  uu.fails_with_code(r1, 1)
  uu.no_stdout(r1)
  uu.stderr_contains(r1, "cksum: --length is only supported with --algorithm blake2b, sha2, or sha3")
}

# origin: uutils test_cksum::test_length_is_zero_with_wrong_algorithm
test test_uu_cksum_length_is_zero_with_wrong_algorithm { |ctx|
  let s = uu.scene(ctx)?
  uu.fixture(s, "cksum", "lorem_ipsum.txt", "lorem_ipsum.txt")?
  uu.fixture(s, "cksum", "md5_single_file.expected", "md5_single_file.expected")?
  let r0 = uu.invoke(s, "cksum", ["--length=0", "-a", "md5", "lorem_ipsum.txt"])?
  uu.succeeds(r0)
  uu.no_stderr(r0)
  uu.stdout_is_bytes(r0, uu.read(s, "md5_single_file.expected")?)
  uu.fixture(s, "cksum", "crc_single_file.expected", "crc_single_file.expected")?
  let r1 = uu.invoke(s, "cksum", ["--length=0", "-a", "crc", "lorem_ipsum.txt"])?
  uu.succeeds(r1)
  uu.no_stderr(r1)
  uu.stdout_is_bytes(r1, uu.read(s, "crc_single_file.expected")?)
  uu.fixture(s, "cksum", "sha1_single_file.expected", "sha1_single_file.expected")?
  let r2 = uu.invoke(s, "cksum", ["--length=0", "-a", "sha1", "lorem_ipsum.txt"])?
  uu.succeeds(r2)
  uu.no_stderr(r2)
  uu.stdout_is_bytes(r2, uu.read(s, "sha1_single_file.expected")?)
  uu.fixture(s, "cksum", "sha224_single_file.expected", "sha224_single_file.expected")?
  let r3 = uu.invoke(s, "cksum", ["--length=0", "-a", "sha224", "lorem_ipsum.txt"])?
  uu.succeeds(r3)
  uu.no_stderr(r3)
  uu.stdout_is_bytes(r3, uu.read(s, "sha224_single_file.expected")?)
  uu.fixture(s, "cksum", "sha256_single_file.expected", "sha256_single_file.expected")?
  let r4 = uu.invoke(s, "cksum", ["--length=0", "-a", "sha256", "lorem_ipsum.txt"])?
  uu.succeeds(r4)
  uu.no_stderr(r4)
  uu.stdout_is_bytes(r4, uu.read(s, "sha256_single_file.expected")?)
  uu.fixture(s, "cksum", "sha384_single_file.expected", "sha384_single_file.expected")?
  let r5 = uu.invoke(s, "cksum", ["--length=0", "-a", "sha384", "lorem_ipsum.txt"])?
  uu.succeeds(r5)
  uu.no_stderr(r5)
  uu.stdout_is_bytes(r5, uu.read(s, "sha384_single_file.expected")?)
  uu.fixture(s, "cksum", "sha512_single_file.expected", "sha512_single_file.expected")?
  let r6 = uu.invoke(s, "cksum", ["--length=0", "-a", "sha512", "lorem_ipsum.txt"])?
  uu.succeeds(r6)
  uu.no_stderr(r6)
  uu.stdout_is_bytes(r6, uu.read(s, "sha512_single_file.expected")?)
}
