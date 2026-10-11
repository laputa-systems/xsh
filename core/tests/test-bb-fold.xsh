use support.uu as uu

# origin: busybox fold/fold -s
test test_bb_fold_fold_s_d8330e49 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w", "7", "-s"], stdin: bytes.from_text("123456\tasdf"))?
  uu.succeeds(r)
  uu.stdout_only(r, "123456\n\t\nasdf")
}

# origin: busybox fold/fold -sw66 with unicode input
test test_bb_fold_fold_sw66_with_unicode_input_cba04180 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-sw66"], stdin: bytes.from_text("The Andromeda Galaxy (pronounced /ænˈdrɒmədə/, also known as Messier 31, M31, or NGC224; often referred to as the Great Andromeda Nebula in older texts) is a spiral galaxy approximately 2,500,000 light-years (1.58×10^11 AU) away in the constellation Andromeda. It is the nearest spiral galaxy to our own, the Milky Way.\nГалактика або Туманність Андромеди (також відома як M31 за каталогом Мессьє та NGC224 за Новим загальним каталогом) — спіральна галактика, що знаходиться на відстані приблизно у 2,5 мільйони світлових років від нашої планети у сузір'ї Андромеди. На початку ХХІ ст. в центрі галактики виявлено чорну дірку."))?
  uu.succeeds(r)
  uu.stdout_only(r, "The Andromeda Galaxy (pronounced /ænˈdrɒmədə/, also known as \nMessier 31, M31, or NGC224; often referred to as the Great \nAndromeda Nebula in older texts) is a spiral galaxy approximately \n2,500,000 light-years (1.58×10^11 AU) away in the constellation \nAndromeda. It is the nearest spiral galaxy to our own, the Milky \nWay.\nГалактика або Туманність Андромеди \n(також відома як M31 за каталогом \nМессьє та NGC224 за Новим загальним \nкаталогом) — спіральна галактика, \nщо знаходиться на відстані \nприблизно у 2,5 мільйони світлових \nроків від нашої планети у сузір'ї \nАндромеди. На початку ХХІ ст. в \nцентрі галактики виявлено чорну \nдірку.")
}

# origin: busybox fold/fold -w1
test test_bb_fold_fold_w1_10fe6a61 { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-w1"], stdin: bytes.from_text("qq w eee r tttt y"))?
  uu.succeeds(r)
  uu.stdout_only(r, "q\nq\n \nw\n \ne\ne\ne\n \nr\n \nt\nt\nt\nt\n \ny")
}

# origin: busybox fold/fold with NULs
test test_bb_fold_fold_with_NULs_9a6deb8f { |ctx|
  let s = uu.scene(ctx)?
  let r = uu.invoke(s, "fold", ["-sw22"], stdin: bytes.from_text("The NUL is here:>\0< and another one is here:>\0< - they must be preserved\n"))?
  uu.succeeds(r)
  uu.stdout_only(r, "The NUL is here:>\0< \nand another one is \nhere:>\0< - they must \nbe preserved\n")
}

