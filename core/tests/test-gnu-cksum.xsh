use support.uu

type Input = {name: Str, data: Bytes}

proc check_case(s: uu.Scene, util: Str, args: List[Str], files: List[Input], input: Bytes, output: Str, diagnostic: Str, code: Int) [fs, process, env, error] -> Result[Unit, Error] {
  for file in files { uu.write_bytes(s, file.name, file.data)? }
  let r = uu.invoke(s, util, args, stdin: input, timeout: 30s)?
  uu.fails_with_code(r, code)
  uu.stdout_is(r, output)
  uu.stderr_is(r, diagnostic)
  Ok()
}

# origin: gnu cksum/md5sum.log
test test_gnu_cksum_md5sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "d41d8cd98f00b204e9800998ecf8427e  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "0cc175b9c0f1b6a831c399e269772661  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "900150983cd24fb0d6963f7d28e17f72  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("message digest")}], bytes.from_text(""), "f96b697d7cb7938d525a2f31aaf161d0  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdefghijklmnopqrstuvwxyz")}], bytes.from_text(""), "c3fcd3d76192e4007dfb496cca67e13b  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")}], bytes.from_text(""), "d174ab98d277d9f5a5611c2c9f419d9f  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", "f"], [{name: "f", data: bytes.from_text("12345678901234567890123456789012345678901234567890123456789012345678901234567890")}], bytes.from_text(""), "57edf4a22be3c955ac49da2e2107b67a  f\n", "", 0)?
  check_case(s, "md5sum", ["--text", ".\nfoo"], [{name: ".\nfoo", data: bytes.from_text("")}], bytes.from_text(""), "\\d41d8cd98f00b204e9800998ecf8427e  .\\nfoo\n", "", 0)?
  check_case(s, "md5sum", ["--text", ".\\foo"], [{name: ".\\foo", data: bytes.from_text("")}], bytes.from_text(""), "\\d41d8cd98f00b204e9800998ecf8427e  .\\\\foo\n", "", 0)?
  check_case(s, "md5sum", ["--text", ".\rfoo"], [{name: ".\rfoo", data: bytes.from_text("")}], bytes.from_text(""), "\\d41d8cd98f00b204e9800998ecf8427e  .\\rfoo\n", "", 0)?
  check_case(s, "md5sum", ["--check", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\n")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\r\n")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "--strict", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\n\n")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "--strict", "in.md5"], [{name: "f", data: bytes.from_text("")}, {name: "in.md5", data: bytes.from_text("ERR\nd41d8cd98f00b204e9800998ecf8427e  f\n")}], bytes.from_text(""), "f: OK\n", "md5sum: WARNING: 1 line is improperly formatted\n", 1)?
  check_case(s, "md5sum", ["--check", "--status", "f.md5"], [{name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\n")}, {name: "f", data: bytes.from_text("foo")}], bytes.from_text(""), "", "", 1)?
  check_case(s, "md5sum", ["--check", "--quiet", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\n")}], bytes.from_text(""), "", "", 0)?
  check_case(s, "md5sum", ["--check", "--quiet", "f.md5"], [{name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\n")}, {name: "f", data: bytes.from_text("foo")}], bytes.from_text(""), "f: FAILED\n", "md5sum: WARNING: 1 computed checksum did NOT match\n", 1)?
  check_case(s, "md5sum", ["--check", "f.md5"], [{name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\nd41d8cd98f00b204e9800998ecf8427e  f\ninvalid\n")}, {name: "f", data: bytes.from_text("foo")}], bytes.from_text(""), "f: FAILED\nf: FAILED\n", "md5sum: WARNING: 1 line is improperly formatted\nmd5sum: WARNING: 2 computed checksums did NOT match\n", 1)?
  check_case(s, "md5sum", ["--check", "--warn", "f.md5"], [{name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\nd41d8cd98f00b204e9800998ecf8427e  f\ninvalid\n")}, {name: "f", data: bytes.from_text("foo")}], bytes.from_text(""), "f: FAILED\nf: FAILED\n", "md5sum: f.md5: 3: improperly formatted MD5 checksum line\nmd5sum: WARNING: 1 line is improperly formatted\nmd5sum: WARNING: 2 computed checksums did NOT match\n", 1)?
  check_case(s, "md5sum", ["--check", "--warn", "f.md5"], [{name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\nd41d8cd98f00b204e9800998ecf8427e  g\nd41d8cd98f00b204e9800998ecf8427e  f\n")}, {name: "f", data: bytes.from_text("")}, {name: "g", data: bytes.from_text("a")}], bytes.from_text(""), "f: OK\ng: FAILED\nf: OK\n", "md5sum: WARNING: 1 computed checksum did NOT match\n", 1)?
  check_case(s, "md5sum", ["--check", "f.sha1"], [{name: "f.sha1", data: bytes.from_text("SHA1 (f) = d41d8cd98f00b204e9800998ecf8427e\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "", "md5sum: f.sha1: no properly formatted checksum lines found\n", 1)?
  check_case(s, "md5sum", ["--check", "f.md5"], [{name: "f.md5", data: bytes.from_text("MD5 (f) = d41d8cd98f00b204e9800998ecf8427e\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "--status", "f.md5"], [{name: "f.md5", data: bytes.from_text("MD5 (f) = d41d8cd98f00b204e9800998ecf8427e\n")}, {name: "f", data: bytes.from_text("bar")}], bytes.from_text(""), "", "", 1)?
  check_case(s, "md5sum", ["--check", "f.sha1"], [{name: "f.sha1", data: bytes.from_text("SHA1(f)= d41d8cd98f00b204e9800998ecf8427e\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "", "md5sum: f.sha1: no properly formatted checksum lines found\n", 1)?
  check_case(s, "md5sum", ["--check", "f.md5"], [{name: "f.md5", data: bytes.from_text("MD5(f)= d41d8cd98f00b204e9800998ecf8427e\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "--status", "f.md5"], [{name: "f.md5", data: bytes.from_text("MD5(f)= d41d8cd98f00b204e9800998ecf8427e\n")}, {name: "f", data: bytes.from_text("bar")}], bytes.from_text(""), "", "", 1)?
  check_case(s, "md5sum", ["--check", "--ignore-missing", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\nd41d8cd98f00b204e9800998ecf8427e  f.missing\n")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "--ignore-missing", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  f\nd41d8cd98f00b204e9800998ecf8427e  f.missing\n")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "--quiet", "--ignore-missing", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  missing/f\nd41d8cd98f00b204e9800998ecf8427e  f\n")}], bytes.from_text(""), "", "", 0)?
  check_case(s, "md5sum", ["--text", "--ignore-missing", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "", "md5sum: the --ignore-missing option is meaningful only when verifying checksums\nTry 'md5sum --help' for more information.\n", 1)?
  check_case(s, "md5sum", ["--check", "--ignore-missing", "f.md5"], [{name: "f", data: bytes.from_text("")}, {name: "f.md5", data: bytes.from_text("d41d8cd98f00b204e9800998ecf8427e  missing\n")}], bytes.from_text(""), "", "md5sum: f.md5: no file was verified\n", 1)?
  check_case(s, "md5sum", ["--check", "--ignore-missing", "f.md5"], [{name: "f", data: bytes.from_text("9t")}, {name: "f.md5", data: bytes.from_text("006999e6df389641adf1fa3a74801d9d  f\n")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "md5sum", ["--check", "z"], [{name: "z", data: bytes.from_text("MD5 (")}], bytes.from_text(""), "", "md5sum: z: no properly formatted checksum lines found\n", 1)?
  check_case(s, "md5sum", ["--check", "h"], [{name: "h", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/md5sum-33-0.bin".read_bytes()?}], bytes.from_text(""), "", "md5sum: h: no properly formatted checksum lines found\n", 1)?
}

# origin: gnu cksum/sha1sum.log
test test_gnu_cksum_sha1sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "da39a3ee5e6b4b0d3255bfef95601890afd80709  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "86f7e437faa5a7fce15d1ddcb9eaeaea377667b8  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "a9993e364706816aba3e25717850c26c9cd0d89d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")}], bytes.from_text(""), "84983e441c3bd26ebaae4aa1f95129e5e54670f1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdefghijklmnopqrstuvwxyz")}], bytes.from_text(""), "32d10c7b8cf96570ca04ce37f2a19d84240d3a89  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")}], bytes.from_text(""), "761c457bf73b14d27e9e9265c46f4b4dda11f940  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("12345678901234567890123456789012345678901234567890123456789012345678901234567890")}], bytes.from_text(""), "50abf5706a150990a08b2c5ea40fa0e585554732  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/million-a.bin".read_bytes()?}], bytes.from_text(""), "34aa973cd4c4daa4f61eeb2bdbad27316534016f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", ".\nfoo"], [{name: ".\nfoo", data: bytes.from_text("")}], bytes.from_text(""), "\\da39a3ee5e6b4b0d3255bfef95601890afd80709  .\\nfoo\n", "", 0)?
  check_case(s, "sha1sum", ["--text", ".\\foo"], [{name: ".\\foo", data: bytes.from_text("")}], bytes.from_text(""), "\\da39a3ee5e6b4b0d3255bfef95601890afd80709  .\\\\foo\n", "", 0)?
  check_case(s, "sha1sum", ["--text", ".\rfoo"], [{name: ".\rfoo", data: bytes.from_text("")}], bytes.from_text(""), "\\da39a3ee5e6b4b0d3255bfef95601890afd80709  .\\rfoo\n", "", 0)?
  check_case(s, "sha1sum", ["--check", "f.md5"], [{name: "f.md5", data: bytes.from_text("MD5 (f) = da39a3ee5e6b4b0d3255bfef95601890afd80709\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "", "sha1sum: f.md5: no properly formatted checksum lines found\n", 1)?
  check_case(s, "sha1sum", ["--check", "f.sha1"], [{name: "f.sha1", data: bytes.from_text("SHA1 (f) = da39a3ee5e6b4b0d3255bfef95601890afd80709\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "sha1sum", ["--check", "--status", "f.sha1"], [{name: "f.sha1", data: bytes.from_text("SHA1 (f) = da39a3ee5e6b4b0d3255bfef95601890afd80709\n")}, {name: "f", data: bytes.from_text("bar")}], bytes.from_text(""), "", "", 1)?
  check_case(s, "sha1sum", ["--check", "f.md5"], [{name: "f.md5", data: bytes.from_text("MD5(f)= da39a3ee5e6b4b0d3255bfef95601890afd80709\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "", "sha1sum: f.md5: no properly formatted checksum lines found\n", 1)?
  check_case(s, "sha1sum", ["--check", "f.sha1"], [{name: "f.sha1", data: bytes.from_text("SHA1(f)= da39a3ee5e6b4b0d3255bfef95601890afd80709\n")}, {name: "f", data: bytes.from_text("")}], bytes.from_text(""), "f: OK\n", "", 0)?
  check_case(s, "sha1sum", ["--check", "--status", "f.sha1"], [{name: "f.sha1", data: bytes.from_text("SHA1(f)= da39a3ee5e6b4b0d3255bfef95601890afd80709\n")}, {name: "f", data: bytes.from_text("bar")}], bytes.from_text(""), "", "", 1)?
  check_case(s, "sha1sum", ["--check", "z"], [{name: "z", data: bytes.from_text("SHA1 (")}], bytes.from_text(""), "", "sha1sum: z: no properly formatted checksum lines found\n", 1)?
}

# origin: gnu cksum/sha224sum.log
test test_gnu_cksum_sha224sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sha224sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7  f\n", "", 0)?
  check_case(s, "sha224sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")}], bytes.from_text(""), "75388b16512776cc5dba5da1fd890150b0c6455cb4f58b1952522525  f\n", "", 0)?
  check_case(s, "sha224sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/million-a.bin".read_bytes()?}], bytes.from_text(""), "20794655980c91d8bbb4c1ea97618a4bf03f42581948b2ee4ee7ad67  f\n", "", 0)?
}

# origin: gnu cksum/sha256sum.log
test test_gnu_cksum_sha256sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sha256sum", ["--text", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  f\n", "", 0)?
  check_case(s, "sha256sum", ["--text", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb  f\n", "", 0)?
  check_case(s, "sha256sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad  f\n", "", 0)?
  check_case(s, "sha256sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")}], bytes.from_text(""), "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1  f\n", "", 0)?
  check_case(s, "sha256sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/million-a.bin".read_bytes()?}], bytes.from_text(""), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0  f\n", "", 0)?
}

# origin: gnu cksum/sha384sum.log
test test_gnu_cksum_sha384sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sha384sum", ["--text", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "38b060a751ac96384cd9327eb1b1e36a21fdb71114be07434c0cc7bf63f6e1da274edebfe76f65fbd51ad2f14898b95b  f\n", "", 0)?
  check_case(s, "sha384sum", ["--text", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "54a59b9f22b0b80880d8427e548b7c23abd873486e1f035dce9cd697e85175033caa88e6d57bc35efae0b5afd3145f31  f\n", "", 0)?
  check_case(s, "sha384sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7  f\n", "", 0)?
  check_case(s, "sha384sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")}], bytes.from_text(""), "09330c33f71147e83d192fc782cd1b4753111b173b3b05d22fa08086e3b0f712fcc7c71a557e2db966c3e9fa91746039  f\n", "", 0)?
  check_case(s, "sha384sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/million-a.bin".read_bytes()?}], bytes.from_text(""), "9d0e1809716474cb086e834e310a4a1ced149e9c00f248527972cec5704c2a5b07b8b3dc38ecc4ebae97ddd87f3d8985  f\n", "", 0)?
}

# origin: gnu cksum/sha512sum.log
test test_gnu_cksum_sha512sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sha512sum", ["--text", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e  f\n", "", 0)?
  check_case(s, "sha512sum", ["--text", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "1f40fc92da241694750979ee6cf582f2d5d7d28e18335de05abc54d0560e0f5302860c652bf08d560252aa5e74210546f369fbbbce8c12cfc7957b2652fe9a75  f\n", "", 0)?
  check_case(s, "sha512sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f  f\n", "", 0)?
  check_case(s, "sha512sum", ["--text", "f"], [{name: "f", data: bytes.from_text("abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu")}], bytes.from_text(""), "8e959b75dae313da8cf4f72814fc143f8f7779c6eb9f7fa17299aeadb6889018501d289e4900f7e4331b99dec4b5433ac7d329eeb6dd26545e96e55b874be909  f\n", "", 0)?
  check_case(s, "sha512sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/million-a.bin".read_bytes()?}], bytes.from_text(""), "e718483d0ce769644e2e42c7bc15b4638e1f98b13b2044285632a803afa973ebde0ff244877ea60a4cb0432ce577c31beb009c5c2c49aa2e4eadb217ad8cc09b  f\n", "", 0)?
}

# origin: gnu cksum/sm3sum.log
test test_gnu_cksum_sm3sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "cksum", ["--untagged", "--text", "-a", "sm3", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "1ab21d8355cfa17f8e61194831e81a8f22bec8c728fefb747ed035eb5082aa2b  f\n", "", 0)?
  check_case(s, "cksum", ["--untagged", "--text", "-a", "sm3", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "623476ac18f65a2909e43c7fec61b49c7e764a91a18ccb82f1917a29c86c5e88  f\n", "", 0)?
  check_case(s, "cksum", ["--untagged", "--text", "-a", "sm3", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "66c7f0f462eeedd9d1f2d46bdc10e4e24167c4875cf2f7a2297da02b8f4ba8e0  f\n", "", 0)?
  check_case(s, "cksum", ["--untagged", "--text", "-a", "sm3", "f"], [{name: "f", data: bytes.from_text("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")}], bytes.from_text(""), "639b6cc5e64d9e37a390b192df4fa1ea0720ab747ff692b9f38c4e66ad7b8c05  f\n", "", 0)?
  check_case(s, "cksum", ["--untagged", "--text", "-a", "sm3", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/million-a.bin".read_bytes()?}], bytes.from_text(""), "c8aaf89429554029e231941a2acc0ad61ff2a5acd8fadd25847a3a732b3b02c3  f\n", "", 0)?
}

# origin: gnu cksum/sum.log
test test_gnu_cksum_sum_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "00000     0 f\n", "", 0)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "00097     1 f\n", "", 0)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "16556     1 f\n", "", 0)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("message digest")}], bytes.from_text(""), "26423     1 f\n", "", 0)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("abcdefghijklmnopqrstuvwxyz")}], bytes.from_text(""), "53553     1 f\n", "", 0)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")}], bytes.from_text(""), "25587     1 f\n", "", 0)?
  check_case(s, "sum", ["f"], [{name: "f", data: bytes.from_text("12345678901234567890123456789012345678901234567890123456789012345678901234567890")}], bytes.from_text(""), "21845     1 f\n", "", 0)?
  check_case(s, "sum", ["-r", "f"], [{name: "f", data: bytes.from_text("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")}], bytes.from_text(""), "65409     1 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")}], bytes.from_text(""), "33793 2 f\n", "", 0)?
  check_case(s, "sum", ["-r", "f"], [{name: "f", data: bytes.from_text("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")}], bytes.from_text(""), "65223     2 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb")}], bytes.from_text(""), "4099 4 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "0 0 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("a")}], bytes.from_text(""), "97 1 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("abc")}], bytes.from_text(""), "294 1 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("message digest")}], bytes.from_text(""), "1413 1 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("abcdefghijklmnopqrstuvwxyz")}], bytes.from_text(""), "2847 1 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789")}], bytes.from_text(""), "5387 1 f\n", "", 0)?
  check_case(s, "sum", ["-s", "f"], [{name: "f", data: bytes.from_text("12345678901234567890123456789012345678901234567890123456789012345678901234567890")}], bytes.from_text(""), "4200 1 f\n", "", 0)?
}

# origin: gnu cksum/sha1sum-vec.log
test test_gnu_cksum_sha1sum_vec_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "da39a3ee5e6b4b0d3255bfef95601890afd80709  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: bytes.from_text("$")}], bytes.from_text(""), "3cdf2936da2fc556bfa533ab1eb59ce710ac80e5  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-2-0.bin".read_bytes()?}], bytes.from_text(""), "19c1e2048fa7393cfbf2d310ad8209ec11d996e5  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-3-0.bin".read_bytes()?}], bytes.from_text(""), "ca775d8c80faa6f87fa62beca6ca6089d63b56e5  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-4-0.bin".read_bytes()?}], bytes.from_text(""), "71ac973d0e4b50ae9e5043ff4d615381120a25a0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-5-0.bin".read_bytes()?}], bytes.from_text(""), "a6b5b9f854cfb76701c3bddbf374b3094ea49cba  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-6-0.bin".read_bytes()?}], bytes.from_text(""), "d87a0ee74e4b9ad72e6847c87bdeeb3d07844380  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-7-0.bin".read_bytes()?}], bytes.from_text(""), "1976b8dd509fe66bf09c9a8d33534d4ef4f63bfd  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-8-0.bin".read_bytes()?}], bytes.from_text(""), "5a78f439b6db845bb8a558e4ceb106cd7b7ff783  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-9-0.bin".read_bytes()?}], bytes.from_text(""), "f871bce62436c1e280357416695ee2ef9b83695c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-10-0.bin".read_bytes()?}], bytes.from_text(""), "62b243d1b780e1d31cf1ba2de3f01c72aeea0e47  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-11-0.bin".read_bytes()?}], bytes.from_text(""), "1698994a273404848e56e7fda4457b5900de1342  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-12-0.bin".read_bytes()?}], bytes.from_text(""), "056f4cdc02791da7ed1eb2303314f7667518deef  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-13-0.bin".read_bytes()?}], bytes.from_text(""), "9fe2da967bd8441eea1c32df68ddaa9dc1fc8e4b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-14-0.bin".read_bytes()?}], bytes.from_text(""), "73a31777b4ace9384efa8bbead45c51a71aba6dd  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-15-0.bin".read_bytes()?}], bytes.from_text(""), "3f9d7c4e2384eddabff5dd8a31e23de3d03f42ac  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-16-0.bin".read_bytes()?}], bytes.from_text(""), "4814908f72b93ffd011135bee347de9a08da838f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-17-0.bin".read_bytes()?}], bytes.from_text(""), "0978374b67a412a3102c5aa0b10e1a6596fc68eb  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-18-0.bin".read_bytes()?}], bytes.from_text(""), "44ad6cb618bd935460d46d3f921d87b99ab91c1e  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-19-0.bin".read_bytes()?}], bytes.from_text(""), "02dc989af265b09cf8485640842128dcf95e9f39  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-20-0.bin".read_bytes()?}], bytes.from_text(""), "67507b8d497b35d6e99fc01976d73f54aeca75cf  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-21-0.bin".read_bytes()?}], bytes.from_text(""), "1eae0373c1317cb60c36a42a867b716039d441f5  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-22-0.bin".read_bytes()?}], bytes.from_text(""), "9c3834589e5bffac9f50950e0199b3ec2620bec8  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-23-0.bin".read_bytes()?}], bytes.from_text(""), "209f7abc7f3b878ee46cdf3a1fbb9c21c3474f32  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-24-0.bin".read_bytes()?}], bytes.from_text(""), "05fc054b00d97753a9b3e2da8fbba3ee808cef22  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-25-0.bin".read_bytes()?}], bytes.from_text(""), "0c4980ea3a46c757dfbfc5baa38ac6c8e72ddce7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-26-0.bin".read_bytes()?}], bytes.from_text(""), "96a460d2972d276928b69864445bea353bdcffd2  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-27-0.bin".read_bytes()?}], bytes.from_text(""), "f3ef04d8fa8c6fa9850f394a4554c080956fa64b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-28-0.bin".read_bytes()?}], bytes.from_text(""), "f2a31d875d1d7b30874d416c4d2ea6baf0ffbafe  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-29-0.bin".read_bytes()?}], bytes.from_text(""), "f4942d3b9e9588dcfdc6312a84df75d05f111c20  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-30-0.bin".read_bytes()?}], bytes.from_text(""), "310207df35b014e4676d30806fa34424813734dd  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-31-0.bin".read_bytes()?}], bytes.from_text(""), "4da1955b2fa7c7e74e3f47d7360ce530bbf57ca3  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-32-0.bin".read_bytes()?}], bytes.from_text(""), "74c4bc5b26fb4a08602d40ccec6c6161b6c11478  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-33-0.bin".read_bytes()?}], bytes.from_text(""), "0b103ce297338dfc7395f7715ee47539b556ddb6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-34-0.bin".read_bytes()?}], bytes.from_text(""), "efc72d99e3d2311ce14190c0b726bdc68f4b0821  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-35-0.bin".read_bytes()?}], bytes.from_text(""), "660edac0a8f4ce33da0d8dbae597650e97687250  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-36-0.bin".read_bytes()?}], bytes.from_text(""), "fe0a55a988b3b93946a63eb36b23785a5e6efc3e  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-37-0.bin".read_bytes()?}], bytes.from_text(""), "0cbdf2a5781c59f907513147a0de3cc774b54bf3  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-38-0.bin".read_bytes()?}], bytes.from_text(""), "663e40fee5a44bfcb1c99ea5935a6b5bc9f583b0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-39-0.bin".read_bytes()?}], bytes.from_text(""), "00162134256952dd9ae6b51efb159b35c3c138c7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-40-0.bin".read_bytes()?}], bytes.from_text(""), "ceb88e4736e354416e2010fc1061b3b53b81664b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-41-0.bin".read_bytes()?}], bytes.from_text(""), "a6a2c4b6bcc41ddc67278f3df4d8d0b9dd7784ef  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-42-0.bin".read_bytes()?}], bytes.from_text(""), "c23d083cd8820b57800a869f5f261d45e02dc55d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-43-0.bin".read_bytes()?}], bytes.from_text(""), "e8ac31927b78ddec41a31ca7a44eb7177165e7ab  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-44-0.bin".read_bytes()?}], bytes.from_text(""), "e864ec5dbab0f9ff6984ab6ad43a8c9b81cc9f9c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-45-0.bin".read_bytes()?}], bytes.from_text(""), "cfed6269069417a84d6de2347220f4b858bcd530  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-46-0.bin".read_bytes()?}], bytes.from_text(""), "d9217bfb46c96348722c3783d29d4b1a3feda38c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-47-0.bin".read_bytes()?}], bytes.from_text(""), "dec24e5554f79697218d317315fa986229ce3350  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-48-0.bin".read_bytes()?}], bytes.from_text(""), "83a099df7071437ba5495a5b0bfbfefe1c0ef7f3  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-49-0.bin".read_bytes()?}], bytes.from_text(""), "aa3198e30891a83e33ce3bfa0587d86a197d4f80  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-50-0.bin".read_bytes()?}], bytes.from_text(""), "9b6acbeb4989cbee7015c7d515a75672ffde3442  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-51-0.bin".read_bytes()?}], bytes.from_text(""), "b021eb08a436b02658eaa7ba3c88d49f1219c035  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-52-0.bin".read_bytes()?}], bytes.from_text(""), "cae36dab8aea29f62e0855d9cb3cd8e7d39094b1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-53-0.bin".read_bytes()?}], bytes.from_text(""), "02de8ba699f3c1b0cb5ad89a01f2346e630459d7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-54-0.bin".read_bytes()?}], bytes.from_text(""), "88021458847dd39b4495368f7254941859fad44b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-55-0.bin".read_bytes()?}], bytes.from_text(""), "91a165295c666fe85c2adbc5a10329daf0cb81a0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-56-0.bin".read_bytes()?}], bytes.from_text(""), "4b31312eaf8b506811151a9dbd162961f7548c4b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-57-0.bin".read_bytes()?}], bytes.from_text(""), "3fe70971b20558f7e9bac303ed2bc14bde659a62  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-58-0.bin".read_bytes()?}], bytes.from_text(""), "93fb769d5bf49d6c563685954e2aecc024dc02d6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-59-0.bin".read_bytes()?}], bytes.from_text(""), "bc8827c3e614d515e83dea503989dea4fda6ea13  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-60-0.bin".read_bytes()?}], bytes.from_text(""), "e83868dbe4a389ab48e61cfc4ed894f32ae112ac  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-61-0.bin".read_bytes()?}], bytes.from_text(""), "55c95459cde4b33791b4b2bcaaf840930af3f3bd  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-62-0.bin".read_bytes()?}], bytes.from_text(""), "36bb0e2ba438a3e03214d9ed2b28a4d5c578fcaa  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-63-0.bin".read_bytes()?}], bytes.from_text(""), "3acbf874199763eba20f3789dfc59572aca4cf33  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-64-0.bin".read_bytes()?}], bytes.from_text(""), "86be037c4d509c9202020767d860dab039cadace  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-65-0.bin".read_bytes()?}], bytes.from_text(""), "51b57d7080a87394eec3eb2e0b242e553f2827c9  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-66-0.bin".read_bytes()?}], bytes.from_text(""), "1efbfa78866315ce6a71e457f3a750a38facab41  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-67-0.bin".read_bytes()?}], bytes.from_text(""), "57d6cb41aeec20236f365b3a490c61d0cfa39611  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-68-0.bin".read_bytes()?}], bytes.from_text(""), "c532cb64b4ba826372bccf2b4b5793d5b88bb715  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-69-0.bin".read_bytes()?}], bytes.from_text(""), "15833b5631032663e783686a209c6a2b47a1080e  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-70-0.bin".read_bytes()?}], bytes.from_text(""), "d04f2043c96e10cd83b574b1e1c217052cd4a6b2  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-71-0.bin".read_bytes()?}], bytes.from_text(""), "e8882627c64db743f7db8b4413dd033fc63beb20  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-72-0.bin".read_bytes()?}], bytes.from_text(""), "cd2d32286b8867bc124a0af2236fc74be3622199  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-73-0.bin".read_bytes()?}], bytes.from_text(""), "019b70d745375091ed5c7b218445ec986d0f5a82  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-74-0.bin".read_bytes()?}], bytes.from_text(""), "e5ff5fec1dadbaed02bf2dad4026be6a96b3f2af  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-75-0.bin".read_bytes()?}], bytes.from_text(""), "6f4e23b3f2e2c068d13921fe4e5e053ffed4e146  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-76-0.bin".read_bytes()?}], bytes.from_text(""), "25e179602a575c915067566fba6da930e97f8678  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-77-0.bin".read_bytes()?}], bytes.from_text(""), "67ded0e68e235c8a523e051e86108eeb757efbfd  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-78-0.bin".read_bytes()?}], bytes.from_text(""), "af78536ea83c822796745556d62a3ee82c7be098  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-79-0.bin".read_bytes()?}], bytes.from_text(""), "64d7ac52e47834be72455f6c64325f9c358b610d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-80-0.bin".read_bytes()?}], bytes.from_text(""), "9d4866baa3639c13e541f250ffa3d8bc157a491f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-81-0.bin".read_bytes()?}], bytes.from_text(""), "2e258811961d3eb876f30e7019241a01f9517bec  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-82-0.bin".read_bytes()?}], bytes.from_text(""), "8e0ebc487146f83bc9077a1630e0fb3ab3c89e63  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-83-0.bin".read_bytes()?}], bytes.from_text(""), "ce8953741fff3425d2311fbbf4ab481b669def70  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-84-0.bin".read_bytes()?}], bytes.from_text(""), "789d1d2dab52086bd90c0e137e2515ed9c6b59b5  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-85-0.bin".read_bytes()?}], bytes.from_text(""), "b76ce7472700dd68d6328b7aa8437fb051d15745  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-86-0.bin".read_bytes()?}], bytes.from_text(""), "f218669b596c5ffb0b1c14bd03c467fc873230a0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-87-0.bin".read_bytes()?}], bytes.from_text(""), "1ff3bdbe0d504cb0cdfab17e6c37aba6b3cffded  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-88-0.bin".read_bytes()?}], bytes.from_text(""), "2f3cbacbb14405a4652ed52793c1814fd8c4fce0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-89-0.bin".read_bytes()?}], bytes.from_text(""), "982c8ab6ce164f481915af59aaed9fff2a391752  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-90-0.bin".read_bytes()?}], bytes.from_text(""), "5cd92012d488a07ece0e47901d0e083b6bd93e3f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-91-0.bin".read_bytes()?}], bytes.from_text(""), "69603fec02920851d4b3b8782e07b92bb2963009  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-92-0.bin".read_bytes()?}], bytes.from_text(""), "3e90f76437b1ea44cf98a08d83ea24cecf6e6191  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-93-0.bin".read_bytes()?}], bytes.from_text(""), "34c09f107c42d990eb4881d4bf2dddcab01563ae  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-94-0.bin".read_bytes()?}], bytes.from_text(""), "474be0e5892eb2382109bfc5e3c8249a9283b03d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-95-0.bin".read_bytes()?}], bytes.from_text(""), "a04b4f75051786682483252438f6a75bf4705ec6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-96-0.bin".read_bytes()?}], bytes.from_text(""), "be88a6716083eb50ed9416719d6a247661299383  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-97-0.bin".read_bytes()?}], bytes.from_text(""), "c67e38717fee1a5f65ec6c7c7c42afc00cd37f04  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-98-0.bin".read_bytes()?}], bytes.from_text(""), "959ac4082388e19e9be5de571c047ef10c174a8d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-99-0.bin".read_bytes()?}], bytes.from_text(""), "baa7aa7b7753fa0abdc4a541842b5d238d949f0a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-100-0.bin".read_bytes()?}], bytes.from_text(""), "351394dcebc08155d100fcd488578e6ae71d0e9c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-101-0.bin".read_bytes()?}], bytes.from_text(""), "ab8be94c5af60d9477ef1252d604e58e27b2a9ee  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-102-0.bin".read_bytes()?}], bytes.from_text(""), "3429ec74a695fdd3228f152564952308afe0680a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-103-0.bin".read_bytes()?}], bytes.from_text(""), "907fa46c029bc67eaa8e4f46e3c2a232f85bd122  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-104-0.bin".read_bytes()?}], bytes.from_text(""), "2644c87d1fbbbc0fc8d65f64bca2492da15baae4  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-105-0.bin".read_bytes()?}], bytes.from_text(""), "110a3eeb408756e2e81abaf4c5dcd4d4c6afcf6d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-106-0.bin".read_bytes()?}], bytes.from_text(""), "cd4fdc35fac7e1adb5de40f47f256ef74d584959  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-107-0.bin".read_bytes()?}], bytes.from_text(""), "8e6e273208ac256f9eccf296f3f5a37bc8a0f9f7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-108-0.bin".read_bytes()?}], bytes.from_text(""), "fe0606100bdbc268db39b503e0fdfe3766185828  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-109-0.bin".read_bytes()?}], bytes.from_text(""), "6c63c3e58047bcdb35a17f74eeba4e9b14420809  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-110-0.bin".read_bytes()?}], bytes.from_text(""), "bcc2bd305f0bcda8cf2d478ef9fe080486cb265f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-111-0.bin".read_bytes()?}], bytes.from_text(""), "ce5223fd3dd920a3b666481d5625b16457dcb5e8  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-112-0.bin".read_bytes()?}], bytes.from_text(""), "948886776e42e4f5fae1b2d0c906ac3759e3f8b0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-113-0.bin".read_bytes()?}], bytes.from_text(""), "4c12a51fcfe242f832e3d7329304b11b75161efb  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-114-0.bin".read_bytes()?}], bytes.from_text(""), "c54bdd2050504d92f551d378ad5fc72c9ed03932  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-115-0.bin".read_bytes()?}], bytes.from_text(""), "8f53e8fa79ea09fd1b682af5ed1515eca965604c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-116-0.bin".read_bytes()?}], bytes.from_text(""), "2d7e17f6294524ce78b33eab72cdd08e5ff6e313  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-117-0.bin".read_bytes()?}], bytes.from_text(""), "64582b4b57f782c9302bfe7d07f74aa176627a3a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-118-0.bin".read_bytes()?}], bytes.from_text(""), "6d88795b71d3e386bbd1eb830fb9f161ba98869f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-119-0.bin".read_bytes()?}], bytes.from_text(""), "86ad34a6463f12cee6de9596aba72f0df1397fd1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-120-0.bin".read_bytes()?}], bytes.from_text(""), "7eb46685a57c0d466152dc339c8122548c757ed1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-121-0.bin".read_bytes()?}], bytes.from_text(""), "e7a98fb0692684054407cc221abc60c199d6f52a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-122-0.bin".read_bytes()?}], bytes.from_text(""), "34df1306662206fd0a5fc2969a4beec4eb0197f7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-123-0.bin".read_bytes()?}], bytes.from_text(""), "56cf7ebf08d10f0cb9fe7ee3b63a5c3a02bcb450  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-124-0.bin".read_bytes()?}], bytes.from_text(""), "3bae5cb8226642088da760a6f78b0cf8eddea9f1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-125-0.bin".read_bytes()?}], bytes.from_text(""), "6475df681e061fa506672c27cbabfa9aa6ddff62  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-126-0.bin".read_bytes()?}], bytes.from_text(""), "79d81991fa4e4957c8062753439dbfd47bbb277d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-127-0.bin".read_bytes()?}], bytes.from_text(""), "bae224477b20302e881f5249f52ec6c34da8ecef  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-128-0.bin".read_bytes()?}], bytes.from_text(""), "ede4deb4293cfe4138c2c056b7c46ff821cc0acc  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-129-0.bin".read_bytes()?}], bytes.from_text(""), "a771fa5c812bd0c9596d869ec99e4f4ac988b13f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-130-0.bin".read_bytes()?}], bytes.from_text(""), "e99d566212bbbceee903946f6100c9c96039a8f4  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-131-0.bin".read_bytes()?}], bytes.from_text(""), "b48ce6b1d13903e3925ae0c88cb931388c013f9c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-132-0.bin".read_bytes()?}], bytes.from_text(""), "e647d5baf670d4bf3afc0a6b72a2424b0c64f194  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-133-0.bin".read_bytes()?}], bytes.from_text(""), "65c1cd932a06b05cd0b43afb3bc7891f6bcef45c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-134-0.bin".read_bytes()?}], bytes.from_text(""), "70ffae353a5cd0f8a65a8b2746d0f16281b25ec7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-135-0.bin".read_bytes()?}], bytes.from_text(""), "cc8221f2b829b8cf39646bf46888317c3eb378ea  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-136-0.bin".read_bytes()?}], bytes.from_text(""), "26accc2d6d51ff7bf3e5895588907765111bb69b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-137-0.bin".read_bytes()?}], bytes.from_text(""), "01072915b8e868d9b28e759cf2bc1aea4bb92165  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-138-0.bin".read_bytes()?}], bytes.from_text(""), "3016115711d74236adf0c371e47992f87a428598  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-139-0.bin".read_bytes()?}], bytes.from_text(""), "bf30417999c1368f008c1f19feca4d18a5e1c3c9  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-140-0.bin".read_bytes()?}], bytes.from_text(""), "62ba49087185f2742c26e1c1f4844112178bf673  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-141-0.bin".read_bytes()?}], bytes.from_text(""), "e1f6b9536f384dd3098285bbfd495a474140dc5a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-142-0.bin".read_bytes()?}], bytes.from_text(""), "b522dae1d67726eba7c4136d4e2f6d6d645ac43e  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-143-0.bin".read_bytes()?}], bytes.from_text(""), "e9a021c3eb0b9f2c710554d4bf21b19f78e09478  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-144-0.bin".read_bytes()?}], bytes.from_text(""), "df13573188f3bf705e697a3e1f580145f2183377  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-145-0.bin".read_bytes()?}], bytes.from_text(""), "188835cfe52ecfa0c4135c2825f245dc29973970  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-146-0.bin".read_bytes()?}], bytes.from_text(""), "41b615a34ee2cec9d84a91b141cfab115821950b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-147-0.bin".read_bytes()?}], bytes.from_text(""), "ab3dd6221d2afe6613b815da1c389eec74aa0337  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-148-0.bin".read_bytes()?}], bytes.from_text(""), "0706d414b4aa7fb4a9051aa70d6856a7264054fb  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-149-0.bin".read_bytes()?}], bytes.from_text(""), "3cbf8151f3a00b1d5a809cbb8c4f3135055a6bd1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-150-0.bin".read_bytes()?}], bytes.from_text(""), "da5d6a0319272bbccea63acfa6799756ffda6840  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-151-0.bin".read_bytes()?}], bytes.from_text(""), "fb4429c95f6277b346d3b389413758dfffeedc98  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-152-0.bin".read_bytes()?}], bytes.from_text(""), "2c6e30d9c895b42dcccfc84c906ec88c09b20de1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-153-0.bin".read_bytes()?}], bytes.from_text(""), "3de3189a5e19f225cdce254dff23dacd22c61363  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-154-0.bin".read_bytes()?}], bytes.from_text(""), "93530a9bc9a817f6922518a73a1505c411d05da2  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-155-0.bin".read_bytes()?}], bytes.from_text(""), "e31354345f832d31e05c1b842d405d4bd4588ec8  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-156-0.bin".read_bytes()?}], bytes.from_text(""), "3ff76957e80b60cf74d015ad431fca147b3af232  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-157-0.bin".read_bytes()?}], bytes.from_text(""), "34ae3b806be143a84dce82e4b830eb7d3d2bac69  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-158-0.bin".read_bytes()?}], bytes.from_text(""), "d7447e53d66bb5e4c26e8b41f83efd107bf4adda  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-159-0.bin".read_bytes()?}], bytes.from_text(""), "77dd2a4482705bc2e9dc96ec0a13395771ac850c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-160-0.bin".read_bytes()?}], bytes.from_text(""), "eaa1465db1f59de3f25eb8629602b568e693bb57  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-161-0.bin".read_bytes()?}], bytes.from_text(""), "9329d5b40e0dc43aa25fed69a0fa9c211a948411  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-162-0.bin".read_bytes()?}], bytes.from_text(""), "e94c0b6aa62aa08c625faf817ddf8f51ec645273  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-163-0.bin".read_bytes()?}], bytes.from_text(""), "7ff02b909d82ad668e31e547e0fb66cb8e213771  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-164-0.bin".read_bytes()?}], bytes.from_text(""), "5bb3570858fa1744123bac2873b0bb9810f53fa1  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-165-0.bin".read_bytes()?}], bytes.from_text(""), "905f43940b3591ce39d1145acb1eca80ab5e43cd  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-166-0.bin".read_bytes()?}], bytes.from_text(""), "336c79fbd82f33e490c577e3f791c3cbfe842aff  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-167-0.bin".read_bytes()?}], bytes.from_text(""), "5c6d07a6b44f7a75a64f6ce592f3bae91e022210  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-168-0.bin".read_bytes()?}], bytes.from_text(""), "7e0d3e9d33127f4a30eb8d9c134a58409fa8695b  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-169-0.bin".read_bytes()?}], bytes.from_text(""), "9a5f50dfcfb19286206c229019f0abf25283028c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-170-0.bin".read_bytes()?}], bytes.from_text(""), "dca737e269f9d8626d488988c996e06b352c0708  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-171-0.bin".read_bytes()?}], bytes.from_text(""), "b8ffc1d4972fce63241e0e77850ac46dde75dbfa  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-172-0.bin".read_bytes()?}], bytes.from_text(""), "e9c9bf41c8549354151b977003ce1d830be667db  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-173-0.bin".read_bytes()?}], bytes.from_text(""), "0942908960b54f96cb43452e583f4f9cb66e398a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-174-0.bin".read_bytes()?}], bytes.from_text(""), "fce34051c34d4b81b85ddc4b543cde8007e284b3  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-175-0.bin".read_bytes()?}], bytes.from_text(""), "61e8916532503627f4024d13884640a46f1d61d4  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-176-0.bin".read_bytes()?}], bytes.from_text(""), "f008d5d7853b6a17b7466cd9e18bd135e520faf4  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-177-0.bin".read_bytes()?}], bytes.from_text(""), "bd8d2e873cf659b5c77aac1616827ef8a3b1a3b3  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-178-0.bin".read_bytes()?}], bytes.from_text(""), "b25a04dd425302ed211a1c2412d2410fa10c63b6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-179-0.bin".read_bytes()?}], bytes.from_text(""), "a404e21588123e0893718b4b44e91414a785b91f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-180-0.bin".read_bytes()?}], bytes.from_text(""), "a1e13bc55bf6dad83cf3aabda3287ad68681ea64  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-181-0.bin".read_bytes()?}], bytes.from_text(""), "d5fd35ffabed6733c92365929df0fb4cae864d15  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-182-0.bin".read_bytes()?}], bytes.from_text(""), "c12e9c280ee9c079e0506ff89f9b20536e0a83ef  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-183-0.bin".read_bytes()?}], bytes.from_text(""), "e22769dc00748a9bbd6c05bbc8e81f2cd1dc4e2d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-184-0.bin".read_bytes()?}], bytes.from_text(""), "f29835a93475740e888e8c14318f3ca45a3c8606  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-185-0.bin".read_bytes()?}], bytes.from_text(""), "1a1d77c6d0f97c4b620faa90f3f8644408e4b13d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-186-0.bin".read_bytes()?}], bytes.from_text(""), "4ec84870e9bdd25f523c6dfb6edd605052ca4eaa  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-187-0.bin".read_bytes()?}], bytes.from_text(""), "d689513fed08b80c39b67371959bc4e3fecb0537  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-188-0.bin".read_bytes()?}], bytes.from_text(""), "c4fed58f209fc3c34ad19f86a6dacadc86c04d33  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-189-0.bin".read_bytes()?}], bytes.from_text(""), "051888c6d00029c176de792b84dece2dc1c74b00  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-190-0.bin".read_bytes()?}], bytes.from_text(""), "1a3540bee05518505827954f58b751c475aeece0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-191-0.bin".read_bytes()?}], bytes.from_text(""), "dfa19180359d5a7a38e842f172359caf4208fc05  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-192-0.bin".read_bytes()?}], bytes.from_text(""), "7b0fa84ebbcff7d7f4500f73d79660c4a3431b67  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-193-0.bin".read_bytes()?}], bytes.from_text(""), "9e886081c9acaad0f97b10810d1de6fcdce6b5f4  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-194-0.bin".read_bytes()?}], bytes.from_text(""), "a4d46e4ba0ae4b012f75b1b50d0534d578ae9cb6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-195-0.bin".read_bytes()?}], bytes.from_text(""), "6342b199ee64c7b2c9cbcd4f2dcb65acef51516f  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-196-0.bin".read_bytes()?}], bytes.from_text(""), "aabfd63688eb678357869130083e1b52f6ea861d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-197-0.bin".read_bytes()?}], bytes.from_text(""), "f732b7372daf44801f81effe3108726239837936  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-198-0.bin".read_bytes()?}], bytes.from_text(""), "5e9347fe4574cdcb80281ed092191199badd7b42  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-199-0.bin".read_bytes()?}], bytes.from_text(""), "d5776b7dfff75c1358abdbbb3f27a20bb6ca7c55  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-200-0.bin".read_bytes()?}], bytes.from_text(""), "022b7ada472fb7a9da9219621c9c5f563d3792f6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-201-0.bin".read_bytes()?}], bytes.from_text(""), "7f1de4eca20362da624653d225a5b3f7964a9ff2  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-202-0.bin".read_bytes()?}], bytes.from_text(""), "ca0f2b1bfb4469c11ed006a994734f0f2f5efd17  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-203-0.bin".read_bytes()?}], bytes.from_text(""), "833d63f5c2ea0cd43ec15f2b9dd97ff12b030479  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-204-0.bin".read_bytes()?}], bytes.from_text(""), "14fd356190416c00592b86ff7ca50b622f85593a  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-205-0.bin".read_bytes()?}], bytes.from_text(""), "4ab6b57eddef1ce935622f935c1619ae7c1667d6  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-206-0.bin".read_bytes()?}], bytes.from_text(""), "b456a6a968acd66caa974f96a9a916e700aa3c5d  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-207-0.bin".read_bytes()?}], bytes.from_text(""), "fd1c257fe046b2a27e2f0cd55ed2deca845f01d7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-208-0.bin".read_bytes()?}], bytes.from_text(""), "66e0d01780f1063e2929eaad74826bc64060e38c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-209-0.bin".read_bytes()?}], bytes.from_text(""), "a8478df406f179fd4ef97f4574d7f99ea1ce9eb8  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-210-0.bin".read_bytes()?}], bytes.from_text(""), "248e58cf09a372114fc2f93b09c5fc14f3d0059e  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-211-0.bin".read_bytes()?}], bytes.from_text(""), "f15767de91796a6816977efa4fced4b7fd9b8a57  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-212-0.bin".read_bytes()?}], bytes.from_text(""), "36a6bc5e680e15675d9696338c88b36248bbbaf4  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-213-0.bin".read_bytes()?}], bytes.from_text(""), "4dea6251b2a6df017a8093ab066ee3863a4ec369  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-214-0.bin".read_bytes()?}], bytes.from_text(""), "d30e70e357d57e3d82ca554b8a3d58dff528fa94  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-215-0.bin".read_bytes()?}], bytes.from_text(""), "70ca84d827f7fd61446233f88cf2f990b0f3e2aa  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-216-0.bin".read_bytes()?}], bytes.from_text(""), "8d500c9cfde0288530a2106b70bed39326c52c3c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-217-0.bin".read_bytes()?}], bytes.from_text(""), "f3d4d139edfc24596377bc97a96fb7621f27ffc7  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-218-0.bin".read_bytes()?}], bytes.from_text(""), "5509baffac6d507860cefc5ab5832cb63cd4b687  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-219-0.bin".read_bytes()?}], bytes.from_text(""), "0c0aea0c2fd7a620c77866b1a177481e26b4f592  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-220-0.bin".read_bytes()?}], bytes.from_text(""), "149176007fee58a591e3f00f8db658b605f8390c  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-221-0.bin".read_bytes()?}], bytes.from_text(""), "17c0d7b0256159f3626786ffdb20237ae154fa84  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-222-0.bin".read_bytes()?}], bytes.from_text(""), "741a58618abeb1d983d67afdcbc49aa397a3b8e0  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-223-0.bin".read_bytes()?}], bytes.from_text(""), "b738d6b3409eb9ed2f1719b84d13f7c36169cdec  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-224-0.bin".read_bytes()?}], bytes.from_text(""), "3d33de31f64055d3b128ac9a6aa3f92dfd4f5330  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-225-0.bin".read_bytes()?}], bytes.from_text(""), "b6925f4df94949b8844c867428ba3dedf4cf2b51  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-226-0.bin".read_bytes()?}], bytes.from_text(""), "cf5e7256292abec431d8e8b9cbeaf22af072377e  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-227-0.bin".read_bytes()?}], bytes.from_text(""), "975dce94902923977f129c0e4acf40ad28ddb9aa  f\n", "", 0)?
  check_case(s, "sha1sum", ["--text", "f"], [{name: "f", data: fp"{ctx.core_dir}/tests/data/gnu/cksum/sha1sum-vec-228-0.bin".read_bytes()?}], bytes.from_text(""), "333b0259b18ce64d6b52cf563dd3041e5f63a516  f\n", "", 0)?
}

# origin: gnu cksum/cksum-base64.log
test test_gnu_cksum_cksum_base64_log { |ctx|
  let s = uu.scene(ctx)?
  check_case(s, "cksum", ["--base64", "-a", "sysv", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "0 0 f\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "bsd", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "00000     0 f\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "crc", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "4294967295 0 f\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "crc32b", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "0 0 f\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "md5", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg==\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "sha1", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "SHA1 (f) = 2jmj7l5rSw0yVb/vlWAYkK/YBwk=\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "sha512", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg==\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "sha3", "--length=512", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "SHA3-512 (f) = pp9zzKI6msXItWfcGFp1bpfJghZP4lhZ4NHcwUdcgKYVshI68fX5TBHj6UAsOsVY9QAZnZW20+MBdYWGKB3NJg==\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "blake2b", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg==\n", "", 0)?
  check_case(s, "cksum", ["--base64", "-a", "sm3", "f"], [{name: "f", data: bytes.from_text("")}], bytes.from_text(""), "SM3 (f) = GrIdg1XPoX+OYRlIMegajyK+yMco/vt0ftA161CCqis=\n", "", 0)?
  check_case(s, "cksum", ["--check", "--strict"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg=="), "f: OK\n", "", 0)?
  check_case(s, "cksum", ["--check", "--strict"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA1 (f) = 2jmj7l5rSw0yVb/vlWAYkK/YBwk="), "f: OK\n", "", 0)?
  check_case(s, "cksum", ["--check", "--strict"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg=="), "f: OK\n", "", 0)?
  check_case(s, "cksum", ["--check", "--strict"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA3-512 (f) = pp9zzKI6msXItWfcGFp1bpfJghZP4lhZ4NHcwUdcgKYVshI68fX5TBHj6UAsOsVY9QAZnZW20+MBdYWGKB3NJg=="), "f: OK\n", "", 0)?
  check_case(s, "cksum", ["--check", "--strict"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg=="), "f: OK\n", "", 0)?
  check_case(s, "cksum", ["--check", "--strict"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SM3 (f) = GrIdg1XPoX+OYRlIMegajyK+yMco/vt0ftA161CCqis="), "f: OK\n", "", 0)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg="), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA1 (f) = 2jmj7l5rSw0yVb/vlWAYkK/YBwk"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg="), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA3-512 (f) = pp9zzKI6msXItWfcGFp1bpfJghZP4lhZ4NHcwUdcgKYVshI68fX5TBHj6UAsOsVY9QAZnZW20+MBdYWGKB3NJg="), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg="), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SM3 (f) = GrIdg1XPoX+OYRlIMegajyK+yMco/vt0ftA161CCqis"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("MD5 (f) = 1B2M2Y8AsgTpgAmY7PhCfg"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA512 (f) = z4PhNX7vuL3xVChQ1m2AB9Yg5AULVxXcg/SpIdNs6c5H0NE8XYXysP+DGNKHfuwvY7kxvUdBeoGlODJ6+SfaPg"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("SHA3-512 (f) = pp9zzKI6msXItWfcGFp1bpfJghZP4lhZ4NHcwUdcgKYVshI68fX5TBHj6UAsOsVY9QAZnZW20+MBdYWGKB3NJg"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["--check"], [{name: "f", data: bytes.from_text("")}], bytes.from_text("BLAKE2b (f) = eGoC90IBWQPGxv2FJVLScpEvR0DhWEdhiobiF/cfVBnSXhAxr+5YUxOJZESTTrBLkDpoWxRIt1XVb3Aa/pvizg"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  check_case(s, "cksum", ["-a", "sha1", "--check"], [], bytes.from_text("\\0\\0\\0"), "", "cksum: 'standard input': no properly formatted checksum lines found\n", 1)?
  let help = uu.invoke(s, "cksum", ["--help"])?
  uu.succeeds(help)
  var algorithms: List[Str] = []
  var in_digest = false
  for line in help.stdout.utf8()?.lines() {
    if line.starts_with("DIGEST determines") { in_digest = true; continue }
    if in_digest {
      if line.trim() == "" { break }
      let clean = line.trim().replace("- ", with: "")
      algorithms += [clean.fields()[0].split(":")[0]]
    }
  }
  assert algorithms == ["sysv", "bsd", "crc", "crc32b", "md5", "sha1", "sha2", "sha3", "blake2b", "sm3"]

}

# origin: gnu cksum/md5sum-newline.log
test test_gnu_cksum_md5sum_newline_log { |ctx|
  let s = uu.scene(ctx)?
  uu.touch(s, "a\nb")?
  let escaped = uu.invoke(s, "md5sum", ["--text", "a\nb"])?
  uu.succeeds(escaped)
  uu.stdout_only(escaped, "\\d41d8cd98f00b204e9800998ecf8427e  a\\nb\n")
  let zero = uu.invoke(s, "md5sum", ["--text", "--zero", "a\nb"])?
  uu.succeeds(zero)
  uu.stdout_only_bytes(zero, b"d41d8cd98f00b204e9800998ecf8427e  a\nb\0")
}

# origin: gnu cksum/cksum-base64-untagged.log
test test_gnu_cksum_cksum_base64_untagged_log { |ctx|
  let s = uu.scene(ctx)?
  uu.write(s, "inp", "test input\n")?
  for algorithm in ["sha2", "sha3", "blake2b"] {
    let lengths = if algorithm == "blake2b" { [8, 216, 224, 232, 248, 256, 264, 376, 384, 392, 504, 512] } else { [224, 256, 384, 512] }
    for length in lengths {
      let digest = uu.invoke(s, "cksum", ["-a", algorithm, "--length", f"{length}", "--base64", "--untagged", "inp"])?
      uu.succeeds(digest)
      uu.no_stderr(digest)
      uu.write_bytes(s, "check", digest.stdout)?
      let checked = uu.invoke(s, "cksum", ["-a", algorithm, "--check", "check"])?
      uu.succeeds(checked)
      uu.stdout_only(checked, "inp: OK\n")
      if algorithm != "blake2b" {
        let data = digest.stdout.utf8()?
        let cut = data.byte_len() - "  inp\n".byte_len() - 1
        uu.write(s, "truncated", data.byte_slice(0, length: cut) + "  inp\n")?
        let invalid = uu.invoke(s, "cksum", ["-a", algorithm, "--check", "truncated"])?
        uu.fails_with_code(invalid, 1)
        uu.stderr_only(invalid, "cksum: truncated: no properly formatted checksum lines found\n")
      }
    }
  }
}

# origin: gnu cksum/cksum-a.log
test test_gnu_cksum_cksum_a_log { |ctx|
  let s = uu.scene(ctx)?
  for algorithm in ["bsd", "sysv", "crc", "md5", "sha1", "sha224", "sha256", "sha384", "sha512", "blake2b"] {
    let utility = if algorithm == "bsd" or algorithm == "sysv" { "sum" } else if algorithm == "crc" { "cksum" } else if algorithm == "blake2b" { "b2sum" } else { algorithm + "sum" }
    for mode in ["-b", "-t"] {
      let options = if algorithm == "bsd" { ["-r"] } else if algorithm == "sysv" { ["-s"] } else if algorithm == "crc" { [] } else { [mode] }
      let separate = uu.invoke(s, utility, options + ["/dev/null"])?
      uu.succeeds(separate)
      let generic = uu.invoke(s, "cksum", ["--untagged", mode, "--algorithm=" + algorithm, "/dev/null"])?
      uu.succeeds(generic)
      uu.no_stderr(generic)
      assert separate.stdout == generic.stdout
      if algorithm in ["sha224", "sha256", "sha384", "sha512"] {
        let named = uu.invoke(s, "cksum", ["--algorithm=" + algorithm, "/dev/null"])?
        let sized = uu.invoke(s, "cksum", ["--algorithm=sha2", "--length=" + algorithm.byte_slice(3), "/dev/null"])?
        uu.succeeds(named)
        uu.succeeds(sized)
        assert named.stdout == sized.stdout
      }
      if !(algorithm in ["bsd", "sysv", "crc"]) {
        uu.write_bytes(s, "out-c", generic.stdout)?
        uu.succeeds(uu.invoke(s, "cksum", ["--check", "--algorithm=" + algorithm, "out-c"])?)
      }
    }
  }
  for args in [["-a", "bsd", "--check"], ["-a", "sha22"], ["--text", "--tag", "-a", "md5"], ["--tag", "--text", "-a", "md5"]] {
    uu.fails_with_code(uu.invoke(s, "cksum", args)?, 1)
  }
  let normal = uu.invoke(s, "cksum", ["--untagged", "-a", "md5", "/dev/null"])?
  let reset = uu.invoke(s, "cksum", ["--tag", "--untagged", "-a", "md5", "/dev/null"])?
  uu.succeeds(normal)
  uu.succeeds(reset)
  assert normal.stdout == reset.stdout
  let binary = uu.invoke(s, "cksum", ["--tag", "--untagged", "--binary", "-a", "md5", "/dev/null"])?
  uu.succeeds(binary)
  uu.stdout_contains(binary, " *")
  for args in [["--binary", "--untagged"], ["--binary", "--tag", "--untagged"]] {
    let ordered = uu.invoke(s, "cksum", args + ["-a", "md5", "/dev/null"])?
    uu.succeeds(ordered)
    assert binary.stdout == ordered.stdout
  }
}

proc tagged_roundtrip(s: uu.Scene, util: Str, base: List[Str], lengths: List[Int], names: List[Str], tag: List[Str], check_name: Str, openssl_name: Str) [fs, process, env, error] -> Result[Unit, Error] {
  var checks = ""
  var expected = ""
  for name in names {
    uu.write(s, name, name + "\n")?
    for length in lengths {
      let r = uu.invoke(s, util, base + ["-l", f"{length}"] + tag + [name])?
      uu.succeeds(r)
      uu.no_stderr(r)
      checks += r.stdout.utf8()?
      expected += (if name in [" b", "*c", " "] { "'" + name + "'" } else { name }) + ": OK\n"
    }
  }
  uu.write(s, check_name, checks)?
  let verified = uu.invoke(s, util, base + ["--strict", "-c", check_name])?
  uu.succeeds(verified)
  uu.stdout_only(verified, expected)
  var openssl = ""
  for line in checks.lines() {
    openssl += line.replace(" (", with: "(").replace(" =", with: "=") + "\n"
  }
  uu.write(s, openssl_name, openssl)?
  let alternate = uu.invoke(s, util, base + ["--strict", "-c", openssl_name])?
  uu.succeeds(alternate)
  uu.stdout_only(alternate, expected)
  Ok()
}

# origin: gnu cksum/b2sum.log
test test_gnu_cksum_b2sum_log { |ctx|
  let s = uu.scene(ctx)?
  for util in ["b2sum", "cksum"] {
    let base = if util == "cksum" { ["-a", "blake2b"] } else { [] }
    tagged_roundtrip(s, util, base, [0, 128], ["a", " b", "*c", "44", " "], if util == "b2sum" { ["--tag"] } else { [] }, "check.b2sum", "openssl.b2sum")?
    let untag = if util == "cksum" { ["--untagged"] } else { [] }
    var values = ""
    for length in [0, 128] {
      let r = uu.invoke(s, util, base + untag + ["--text", "-l", f"{length}", "/dev/null"])?
      uu.succeeds(r)
      values += r.stdout.utf8()?
      uu.write_bytes(s, "check.b2sum", r.stdout)?
      for options in [["-l", f"{length}"], []] {
        uu.succeeds(uu.invoke(s, util, base + options + ["--strict", "-c", "check.b2sum"])?)
      }
    }
    uu.write(s, "check.vals", values)?
    let digest = uu.invoke(s, util, base + untag + ["--length=128", "check.vals"])?
    uu.succeeds(digest)
    uu.stdout_only(digest, "796485dd32fe9b754ea5fd6c721271d9  check.vals\n")
    uu.write(s, "crash.check", "BLAKE2\nBLAKE2b\nBLAKE2-\nBLAKE2(\nBLAKE2 (\n")?
    uu.fails_with_code(uu.invoke(s, util, base + ["-c", "crash.check"])?, 1)
    uu.write(s, "overflow.check", "0A0BA0")?
    uu.fails_with_code(uu.invoke(s, util, base + ["-c", "overflow.check"])?, 1)
    uu.succeeds(uu.invoke(s, util, base + ["-l", "123", "-l", "128", "/dev/null"])?)
    for length in ["513", "1024", "18446744073709551616"] {
      let r = uu.invoke(s, util, base + ["-l", length, "/dev/null"])?
      uu.fails_with_code(r, 1)
      uu.stderr_is(r, f"{util}: invalid length: '{length}'\n{util}: maximum digest length for 'BLAKE2b' is 512 bits\n")
    }
  }
}

# origin: gnu cksum/cksum-sha3.log
test test_gnu_cksum_cksum_sha3_log { |ctx|
  let s = uu.scene(ctx)?
  let base = ["-a", "sha3"]
  tagged_roundtrip(s, "cksum", base, [224, 256, 384, 512], ["a", " b", "*c", "44", " "], [], "check.sha3", "openssl.sha3")?
  var values = ""
  for length in [224, 256, 384, 512] {
    let r = uu.invoke(s, "cksum", base + ["--untagged", "--text", "-l", f"{length}", "/dev/null"])?
    uu.succeeds(r)
    values += r.stdout.utf8()?
    uu.write_bytes(s, "check.sha3", r.stdout)?
    for options in [["-l", f"{length}"], []] {
      uu.succeeds(uu.invoke(s, "cksum", base + options + ["--strict", "-c", "check.sha3"])?)
    }
  }
  uu.write(s, "check.vals", values)?
  let digest = uu.invoke(s, "cksum", base + ["--length=256", "check.vals"])?
  uu.succeeds(digest)
  uu.stdout_only(digest, "SHA3-256 (check.vals) = b4753bf1696fda712821b665494c89090ffb0e87b8645559ad9f5db25b42d4f3\n")
  uu.write(s, "inp", "SHA3-248 (check.vals) = b4753bf1696fda712821b665494c89090ffb0e87b8645559ad9f5db25b42d4\n")?
  let truncated = uu.invoke(s, "cksum", base + ["-c", "--warn", "inp"])?
  uu.fails_with_code(truncated, 1)
  uu.stderr_only(truncated, "cksum: inp: 1: improperly formatted SHA3 checksum line\ncksum: inp: no properly formatted checksum lines found\n")
  uu.succeeds(uu.invoke(s, "cksum", base + ["-l", "253", "-l", "256", "/dev/null"])?)
  for length in ["216", "248", "376", "504", "513", "1024", "18446744073709551616"] {
    for mode in [[], ["--check"]] {
      let r = uu.invoke(s, "cksum", base + ["-l", length] + mode + ["/dev/null"])?
      uu.fails_with_code(r, 1)
      uu.stderr_is(r, f"cksum: invalid length: '{length}'\ncksum: digest length for 'SHA3' must be 224, 256, 384, or 512\n")
    }
  }
}

# origin: gnu cksum/cksum-raw.log
test test_gnu_cksum_cksum_raw_log { |ctx|
  let s = uu.scene(ctx)?
  let timestamp = uu.invoke(s, "date", [])?
  uu.succeeds(timestamp)
  uu.write_bytes(s, "file.in", timestamp.stdout)?
  for algorithm in ["bsd", "sysv", "crc", "md5", "sha1", "sha2", "sha3", "blake2b", "sm3"] {
    let lengths = if algorithm in ["sha2", "sha3"] { [224, 256, 384, 512] } else if algorithm == "blake2b" { [8, 256, 512] } else { [0] }
    for length in lengths {
      let options = ["-a", algorithm, "-l" + f"{length}"]
      let raw = uu.invoke(s, "cksum", ["--raw"] + options + ["file.in"])?
      uu.succeeds(raw)
      uu.no_stderr(raw)
      let textual = uu.invoke(s, "cksum", ["--untagged"] + options, stdin: timestamp.stdout)?
      uu.succeeds(textual)
      uu.no_stderr(textual)
      let encoded = if algorithm in ["bsd", "sysv", "crc"] {
        var number = 0
        for index in range(raw.stdout.len()) { number = number * 256 + (raw.stdout.byte_at(index) ?? 0) }
        let value = f"{number}"
        if algorithm == "bsd" { ["0" for n in range(5 - value.byte_len())].join("") + value } else { value }
      } else { hex_bytes(raw.stdout) }
      assert textual.stdout.utf8()?.split(" ")[0] == encoded
    }
  }
  uu.fails_with_code(uu.invoke(s, "cksum", ["--base64", "--raw"])?, 1)
  uu.fails_with_code(uu.invoke(s, "cksum", ["--raw", "/dev/null", "/dev/null"])?, 1)
}

# origin: gnu cksum/cksum.log
test test_gnu_cksum_cksum_log { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "cksum", ["missing"])?, 1)
  let disabled = "glibc.cpu.hwcaps=-AVX512F,-AVX2,-AVX,-PMULL"
  let debug = uu.invoke(s, "cksum", ["--debug", "/dev/null"], vars: {GLIBC_TUNABLES: disabled})?
  uu.succeeds(debug)
  assert !regex.compile(r"using.*hardware support")?.matches(debug.stderr.utf8()?)
  var all = b""
  let octets = bytes.from_ints([n for n in range(256)])?
  for offset in range(-1, 7) {
    all = bytes.concat([all, bytes.from_ints(if offset < 0 { [0] } else { [n for n in range(offset + 1)] })?, octets])
  }
  let cases = [
    {input: all, crc: "4097727897", crc32b: "559400337"},
    {input: bytes.from_ints([n for n in range(131)])?, crc: "3800919234", crc32b: "3739179551"},
    {input: bytes.from_ints([n for n in range(65)])?, crc: "796287823", crc32b: "1086353368"},
    {input: bytes.from_text([f"{n}" + "\n" for n in range(1, 12781)].join("")), crc: "3720986905", crc32b: "388883562"},
    {input: bytes.from_text([f"{n}" + "\n" for n in range(1, 12796)].join("")), crc: "4278270357", crc32b: "2796628507"},
  ]
  for case in cases {
    uu.write_bytes(s, "in", case.input)?
    var tunable = "glibc.cpu.hwcaps="
    for feature in ["NONE", "AVX512F", "AVX2", "AVX", "PMULL"] {
      tunable += "-" + feature + ","
      for algorithm in ["crc", "crc32b"] {
        let value = if algorithm == "crc" { case.crc } else { case.crc32b }
        for invocation in range(2) {
          let r = uu.invoke(s, "cksum", ["-a", algorithm, "in"], vars: {GLIBC_TUNABLES: tunable})?
          uu.succeeds(r)
          uu.stdout_only(r, f"{value} {case.input.len()} in\n")
        }
      }
    }
  }
}

# origin: gnu cksum/md5sum-bsd.log
test test_gnu_cksum_md5sum_bsd_log { |ctx|
  let s = uu.scene(ctx)?
  var standard = ""
  var reverse = ""
  var tags = ""
  var expected = ""
  for name in ["a", " b", "*c", "dd", " "] {
    uu.write(s, name, name + "\n")?
    let r = uu.invoke(s, "md5sum", ["--text", name])?
    uu.succeeds(r)
    let line = r.stdout.utf8()?
    standard += line
    reverse += line.byte_slice(0, length: 33) + line.byte_slice(34)
    let tag = uu.invoke(s, "md5sum", ["--tag", name])?
    uu.succeeds(tag)
    tags += tag.stdout.utf8()?
    expected += (if name in [" b", "*c", " "] { "'" + name + "'" } else { name }) + ": OK\n"
  }
  for case in [{name: "check.md5sum", content: standard}, {name: "check.md5", content: reverse}, {name: "check.md5", content: tags}] {
    uu.write(s, case.name, case.content)?
    let r = uu.invoke(s, "md5sum", ["--strict", "-c", case.name])?
    uu.succeeds(r)
    uu.stdout_only(r, expected)
  }
  uu.write(s, "check2.md5sum", "____not_all_hex_so_no_match_____ blah\n" + standard)?
  let header = uu.invoke(s, "md5sum", ["-c", "check2.md5sum"])?
  uu.succeeds(header)
  uu.stdout_is(header, expected)
  uu.stderr_is(header, "md5sum: WARNING: 1 line is improperly formatted\n")
  let tail = reverse.byte_slice((reverse.find("\n") ?? 0) + 1)
  uu.fails_with_code(uu.invoke(s, "md5sum", ["--strict", "-c"], stdin: bytes.from_text(tail))?, 1)
  for options in [["--tag", "--check"], ["--tag", "--text"]] {
    uu.fails_with_code(uu.invoke(s, "md5sum", options + ["/dev/null"])?, 1)
  }
  uu.write(s, "backslash\\is\\not\\dir\\sep", "\n")?
  var escapes = ""
  for name in ["a\\b", "a\\", "\\a", "a\nb", "a\tb"] {
    uu.touch(s, name)?
    let r = uu.invoke(s, "md5sum", ["--tag", name])?
    uu.succeeds(r)
    escapes += r.stdout.utf8()?
  }
  uu.write(s, "check.md5", escapes)?
  let checked = uu.invoke(s, "md5sum", ["--strict", "-c", "check.md5"])?
  uu.succeeds(checked)
  uu.stdout_only(checked, "'a\\b': OK\n'a\\': OK\n'\\a': OK\n'a'$'\\n''b': OK\n'a'$'\\t''b': OK\n")
  uu.touch(s, "test\n\\\\file")?
  let escaped = uu.invoke(s, "md5sum", ["--tag", "test\n\\\\file"])?
  uu.succeeds(escaped)
  uu.stdout_only(escaped, "\\MD5 (test\\n\\\\\\\\file) = d41d8cd98f00b204e9800998ecf8427e\n")
}

# origin: gnu cksum/sum-sysv.log
test test_gnu_cksum_sum_sysv_log { |ctx|
  let s = uu.scene(ctx)?
  let block = bytes.from_ints([255 for n in range(65537)])?
  let data = bytes.concat([block for n in range(257)])
  for case in [{input: data, expected: "65535 32897\n"}, {input: bytes.concat([data, b"\xff"]), expected: "254 32897\n"}] {
    let r = uu.invoke(s, "sum", ["-s"], stdin: case.input, timeout: 30s)?
    uu.succeeds(r)
    uu.stdout_only(r, case.expected)
  }
}

# origin: gnu cksum/md5sum-parallel.log
test test_gnu_cksum_md5sum_parallel_log { |ctx|
  let s = uu.scene(ctx)?
  uu.mkdir(s, "tmp")?
  let names = [f"tmp/{n}" for n in range(1, 501)]
  for name in names { uu.touch(s, name)? }
  let checksum_argv = uu.argv(s, "md5sum", [])?
  let xargs_argv = uu.argv(s, "xargs", [p"-n500", p"-P2"] + checksum_argv)?
  let cat_argv = uu.argv(s, "cat", [])?
  # The shared pipe exercises record atomicity with three batches and two workers.
  # Every child command still uses the isolated applet launcher.
  let reader = ["'" + word.display().replace("'", with: "'\\''") + "'" for word in cat_argv].join(" ")
  let payload = bytes.from_text((names + names + names).join("\n") + "\n")
  let output = uu.at(s, "out")
  let diagnostic = uu.at(s, "err")
  let script = Path("\"$@\" | " + reader)
  let pipeline = process.command_argv(p"/bin/sh", [p"sh", p"-c", script, p"sh"] + xargs_argv,
    s.root, {}, payload, output, diagnostic, timeout: 30s)
  assert process.run(pipeline)?.exit_code()? == 0
  assert uu.read(s, "err")? == b""
  let lines = uu.read_text(s, "out")?.lines()
  assert lines.len() == 1500
  for line in lines { assert regex.compile(r"^[0-9a-f]{32}  ")?.matches(line) }
}

proc hex_bytes(data: Bytes) -> Str {
  let digits = "0123456789abcdef"
  var value = ""
  for index in range(data.len()) {
    let byte = data.byte_at(index) ?? 0
    value += digits.byte_slice(byte / 16, length: 1) + digits.byte_slice(byte % 16, length: 1)
  }
  value
}
