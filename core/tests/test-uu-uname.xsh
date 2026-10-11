##! Transcribed from the uutils coreutils integration suite, tests/by-util/test_uname.rs.

use support.uu as uu

# The flat class preserves the upstream Unicode symbol allowance while
# avoiding nested classes and property escapes unsupported by the native regex.
# Scalar ranges are the frozen upstream regex dependency's Unicode categories.
const INVISIBLE = rx"[^\x20-\x7e¦©®°҂֍-֎؎-؏۞۩۽-۾߶৺୰௳-௸௺౿൏൹༁-༃༓༕-༗༚-༟༴༶༸྾-࿅࿇-࿌࿎-࿏࿕-࿘႞-႟᎐-᎙᙭᥀᧞-᧿᭡-᭪᭴-᭼℀-℁℃-℆℈-℉℔№-℗℞-℣℥℧℩℮℺-℻⅊⅌-⅍⅏↊-↋↕-↙↜-↟↡-↢↤-↥↧-↭↯-⇍⇐-⇑⇓⇕-⇳⌀-⌇⌌-⌟⌢-⌨⌫-⍻⍽-⎚⎴-⏛⏢-␩⑀-⑊⒜-ⓩ─-▶▸-◀◂-◷☀-♮♰-❧➔-➿⠀-⣿⬀-⬯⭅-⭆⭍-⭳⭶-⮕⮗-⯿⳥-⳪⹐-⹑⺀-⺙⺛-⻳⼀-⿕⿰-⿿〄〒-〓〠〶-〷〾-〿㆐-㆑㆖-㆟㇀-㇥㇯㈀-㈞㈪-㉇㉐㉠-㉿㊊-㊰㋀-㏿䷀-䷿꒐-꓆꠨-꠫꠶-꠷꠹꩷-꩹﵀-﵏﷏﷽-﷿￤￨￭-￮￼-�𐄷-𐄿𐅹-𐆉𐆌-𐆎𐆐-𐆜𐆠𐇐-𐇼𐡷-𐡸𐫈𑜿𑿕-𑿜𑿡-𑿱𖬼-𖬿𖭅𛲜𜰀-𜳯𜴀-𜺳𜽐-𜿃𝀀-𝃵𝄀-𝄦𝄩-𝅘𝅥𝅲𝅪-𝅬𝆃-𝆄𝆌-𝆩𝆮-𝇪𝈀-𝉁𝉅𝌀-𝍖𝠀-𝧿𝨷-𝨺𝩭-𝩴𝩶-𝪃𝪅-𝪆𞅏𞲬𞴮🀀-🀫🀰-🂓🂠-🂮🂱-🂿🃁-🃏🃑-🃵🄍-🆭🇦-🈂🈐-🈻🉀-🉈🉐-🉑🉠-🉥🌀-🏺🐀-🛗🛜-🛬🛰-🛼🜀-🝶🝻-🟙🟠-🟫🟰🠀-🠋🠐-🡇🡐-🡙🡠-🢇🢐-🢭🢰-🢻🣀-🣁🤀-🩓🩠-🩭🩰-🩼🪀-🪉🪏-🫆🫎-🫜🫟-🫩🫰-🫸🬀-🮒🮔-🯯]"
const TRAILING_SPACE = rx"[\x09-\x0d    -  -   　]+$"

# origin: uutils test_uname::test_invalid_arg
test test_uu_uname_invalid_arg { |ctx|
  let s = uu.scene(ctx)?
  uu.fails_with_code(uu.invoke(s, "uname", ["--definitely-invalid"])?, 1)
}

# origin: uutils test_uname::test_uname
test test_uu_uname_uname { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "uname", [])?)
}

# origin: uutils test_uname::test_uname_compatible
test test_uu_uname_uname_compatible { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "uname", ["-a"])?)
}

# origin: uutils test_uname::test_uname_name
test test_uu_uname_uname_name { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "uname", ["-n"])?)
}

# origin: uutils test_uname::test_uname_processor
test test_uu_uname_uname_processor { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uname", ["-p"])?
  uu.succeeds(r)
  assert TRAILING_SPACE.replace(r.stdout.utf8()?, with: "") == "unknown"
}

# origin: uutils test_uname::test_uname_hardware_platform
test test_uu_uname_uname_hardware_platform { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uname", ["-i"])?
  uu.succeeds(r)
  assert TRAILING_SPACE.replace(r.stdout.utf8()?, with: "") == "unknown"
  uu.no_stderr(r)
}

# origin: uutils test_uname::test_uname_machine
test test_uu_uname_uname_machine { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "uname", ["-m"])?)
}

# origin: uutils test_uname::test_uname_kernel_version
test test_uu_uname_uname_kernel_version { |ctx|
  let s = uu.scene(ctx)?
  uu.succeeds(uu.invoke(s, "uname", ["-v"])?)
}

# origin: uutils test_uname::test_uname_kernel
test test_uu_uname_uname_kernel { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uname", ["-o"])?
  uu.succeeds(r)
  assert "linux" in r.stdout.utf8()?.lower()
}

# origin: uutils test_uname::test_uname_help
test test_uu_uname_uname_help { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uname", ["--help"])?
  uu.succeeds(r)
  uu.stdout_contains(r, "system information")
}

# origin: uutils test_uname::test_uname_output_for_invisible_chars
test test_uu_uname_uname_output_for_invisible_chars { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uname", ["--all"])?
  uu.succeeds(r)
  let pattern = INVISIBLE
  assert ! pattern.matches(TRAILING_SPACE.replace(r.stdout.utf8()?, with: ""))
}

# origin: uutils test_uname::test_uname_all_labeled
test test_uu_uname_uname_all_labeled { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "uname", ["-A"])?
  uu.succeeds(r)
  let stdout = r.stdout.utf8()?
  assert stdout.lines().len() == 6
  for label in ["Kernel name: ", "Node name: ", "Kernel release: ", "Kernel version: ", "Machine: ", "Operating system: "] {
    assert label in stdout, f"missing {label}"
  }
  assert ! ("Processor:" in stdout)
  assert ! ("Hardware platform:" in stdout)
}

# origin: uutils test_uname::test_uname_all_labeled_long_flag
test test_uu_uname_uname_all_labeled_long_flag { |ctx|
  let s = uu.scene(ctx)?
  let short = uu.invoke(s, "uname", ["-A"])?
  uu.succeeds(short)
  let long = uu.invoke(s, "uname", ["--all-labeled"])?
  uu.succeeds(long)
  uu.stdout_is_bytes(long, short.stdout)
}
