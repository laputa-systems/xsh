# Case definitions for generate.py. Each `case(...)` runs in the reference
# container; see generate.py for the layout.

BASE = nums(1, 12)
EDITED = lines("1", "2", "3", "new three", "4", "FIVE", "6", "7", "8", "10", "11", "12")
PU = udiff(BASE, EDITED)
PC = cdiff(BASE, EDITED)
PN = "3a4\n> new three\n5c6\n< 5\n---\n> FIVE\n9d9\n< 9\n"
PE = "9d\n5c\nFIVE\n.\n3a\nnew three\n.\n"

# Formats and operand forms
case("unified-operand", "t pu", t=BASE, pu=PU)
case("unified-stdin", "t", stdin=PU, t=BASE)
case("unified-named-by-header", "", stdin=PU, f=BASE)
case("context-operand", "t pc", t=BASE, pc=PC)
case("normal-operand", "t pn", t=BASE, pn=PN)
case("normal-stdin", "t", stdin=PN, t=BASE)
case("ed-operand", "t pe", t=BASE, pe=PE)
case("ed-flag", "-e t pe", t=BASE, pe=PE)
case("forced-unified", "-u t pu", t=BASE, pu=PU)
case("forced-unified-on-context", "-u t pc", t=BASE, pc=PC)
case("forced-context", "-c t pc", t=BASE, pc=PC)
case("forced-context-on-unified", "-c t pu", t=BASE, pu=PU)
case("forced-normal", "-n t pn", t=BASE, pn=PN)
case("forced-normal-on-unified", "-n t pu", t=BASE, pu=PU)
case("forced-ed-on-unified", "-e t pu", t=BASE, pu=PU)
case("long-forced-formats", "--unified --context t pu", t=BASE, pu=PU)
case("context-pure-add", "t p", t=BASE, p=cdiff(BASE, lines(*(["1", "2", "3", "x", "y", "4"] + [str(n) for n in range(5, 13)]))))
case("context-pure-delete", "t p", t=BASE, p=cdiff(BASE, lines(*([str(n) for n in range(1, 12) if n != 6]))))
case("unified-pure-add-at-start", "t p", t=BASE, p="--- f\n+++ f\n@@ -0,0 +1,2 @@\n+a\n+b\n")
case("unified-zero-context", "t p", t=BASE, p="--- f\n+++ f\n@@ -5 +5 @@\n-5\n+five\n@@ -9,0 +10 @@\n+nine and a half\n")
case("unified-missing-newline-final-marker", "t p", t="a\nb\nc", p="--- f\n+++ f\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n-c\n\\ No newline at end of file\n+c\n")
case("unified-function-text-kept-in-reject", "t p", t=nums(1, 3), p="--- f\n+++ f\n@@ -1,2 +1,2 @@ void main()\n 9\n-8\n+7\n")
case("hunk-only-no-header", "t p", t=BASE, p="@@ -3,3 +3,3 @@\n 3\n-4\n+FOUR\n 5\n")
case("preamble-and-trailer", "t p", t=BASE, p="This is a message.\nIt precedes the patch.\n\n" + PU + "-- \ntrailing signature\n")
case("two-files-one-patch", "", stdin=udiff("a\n", "A\n", "x", "x") + udiff("b\n", "B\n", "y", "y"), x="a\n", y="b\n")
case("two-files-with-named-operand", "z", stdin=udiff("a\n", "A\n", "x", "x") + udiff("A\n", "AA\n", "y", "y"), z="a\n", x="a\n", y="A\n")
case("index-line-names-file", "", stdin="Index: ix\n===================================================================\n@@ -1 +1 @@\n-1\n+2\n", ix="1\n")
case("index-line-vs-headers", "", stdin="Index: ix\n===\n--- ix.orig\n+++ ix.new\n@@ -1 +1 @@\n-1\n+2\n", ix="1\n")
case("empty-input", "t", stdin="", t=BASE)
case("garbage-input", "t", stdin="hello\nworld\n", t=BASE)
case("header-only-input", "t", stdin="--- f\n+++ f\n", t=BASE)

# Offsets and fuzz
case("offset-forward", "t pu", t="a\nb\n" + BASE, pu=PU)
case("offset-negative", "t p", t=nums(4, 12), p="--- f\n+++ f\n@@ -7,3 +7,3 @@\n 7\n-8\n+EIGHT\n 9\n")
case("offset-two-hunks-shifted", "h p", h=nums(101, 110) + nums(1, 40), p=udiff(nums(1, 40), nums(1, 4) + nums(8, 29) + "X30\n" + nums(31, 35) + "ins1\nins2\n" + nums(36, 40)))
case("offset-first-hunk-fails", "h p", h=nums(1, 4) + "zz\n" + nums(6, 40), p=udiff(nums(1, 40), nums(1, 4) + "FIVE\n" + nums(6, 29) + "X30\n" + nums(31, 40)))
case("offset-second-hunk-fails", "h p", h=nums(1, 28) + "q1\nq2\n" + nums(29, 40), p=udiff(nums(1, 40), nums(1, 4) + "FIVE\n" + nums(6, 29) + "X30\n" + nums(31, 40)))
case("fuzz-one-line", "t p", t=nums(1, 8) + "N9\n10\n11\n12\n", p=udiff(nums(1, 20), nums(1, 9) + "TEN\n" + nums(11, 20), n=1).replace("-10", "-10"))
for tag, edits, fuzz in [
    ("fuzz0-ctx1", {9: "N9"}, 0),
    ("fuzz1-ctx1", {9: "N9"}, 1),
    ("fuzz1-ctx1-both", {9: "N9", 11: "N11"}, 1),
    ("fuzz2-ctx1-both", {9: "N9", 11: "N11"}, 2),
]:
    body = [str(n) for n in range(1, 21)]
    for at, text in edits.items():
        body[at - 1] = text
    changed = [str(n) for n in range(1, 21)]
    changed[9] = "TEN"
    case("fuzz-" + tag, "-F %d t p" % fuzz, t=lines(*body), p=udiff(nums(1, 20), lines(*changed), n=1))
for tag, edits, fuzz in [
    ("fuzz0-ctx3-last", {9: "N9"}, 0),
    ("fuzz1-ctx3-last", {9: "N9"}, 1),
    ("fuzz1-ctx3-outer", {7: "N7", 13: "N13"}, 1),
    ("fuzz2-ctx3-inner", {8: "N8", 9: "N9", 11: "N11", 12: "N12"}, 2),
    ("fuzz3-ctx3-inner", {8: "N8", 9: "N9", 11: "N11", 12: "N12"}, 3),
    ("fuzz3-ctx3-all", {7: "N7", 8: "N8", 9: "N9", 11: "N11", 12: "N12", 13: "N13"}, 3),
    ("fuzz4-ctx3-all", {7: "N7", 8: "N8", 9: "N9", 11: "N11", 12: "N12", 13: "N13"}, 4),
]:
    body = [str(n) for n in range(1, 21)]
    for at, text in edits.items():
        body[at - 1] = text
    changed = [str(n) for n in range(1, 21)]
    changed[9] = "TEN"
    case("fuzz-" + tag, "-F %d t p" % fuzz, t=lines(*body), p=udiff(nums(1, 20), lines(*changed), n=3))
case("fuzz-default-two", "t p", t=lines(*([str(n) for n in range(1, 21)][:6] + ["N7", "N8", "9", "10", "11", "N12", "N13"] + [str(n) for n in range(14, 21)])), p=udiff(nums(1, 20), nums(1, 9) + "TEN\n" + nums(11, 20)))
case("fuzz-bad-number", "-F x t pu", t=BASE, pu=PU)
case("ignore-whitespace-off", "t p", t="a  b\nc d\ne\n", p="--- w\n+++ w\n@@ -1,3 +1,3 @@\n a b\n-  c   d\n+X\n e\n")
case("ignore-whitespace-on", "-l t p", t="a  b\nc d\ne\n", p="--- w\n+++ w\n@@ -1,3 +1,3 @@\n a b\n-  c   d\n+X\n e\n")
case("ignore-whitespace-trailing", "-l t p", t="a b  \n\tc d \ne\n", p="--- w\n+++ w\n@@ -1,3 +1,3 @@\n a b\n-  c   d\n+X\n e\n")
case("ignore-whitespace-long", "--ignore-whitespace t p", t="a  b\nc d\ne\n", p="--- w\n+++ w\n@@ -1,3 +1,3 @@\n a b\n-  c   d\n+X\n e\n")

# Rejects
case("reject-unified", "t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-context", "t pc", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pc=PC)
case("reject-normal", "t pn", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pn=PN)
case("reject-normal-change", "t p", t=nums(1, 12), p="3,4c3,5\n< x\n< y\n---\n> a\n> b\n> c\n")
case("reject-normal-delete", "t p", t=nums(1, 12), p="3,4d2\n< x\n< y\n")
case("reject-file-option", "-r my.rej t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-file-long", "--reject-file=my.rej t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-file-in-directory", "-r rd/x.rej t pu", dirs=["rd"], t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-format-unified-on-context", "--reject-format=unified t pc", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pc=PC)
case("reject-format-context-on-unified", "--reject-format=context t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-format-bad", "--reject-format=bogus t pu", t=BASE, pu=PU)
case("reject-second-hunk-shifted-header", "t p", t=nums(1, 29) + "zz\n" + nums(31, 40), p=udiff(nums(1, 40), nums(1, 4) + "FIVE\nextra1\nextra2\n" + nums(6, 29) + "THIRTY\n" + nums(31, 40)))
case("reject-second-hunk-context", "t p", t=nums(1, 29) + "zz\n" + nums(31, 40), p=cdiff(nums(1, 40), nums(1, 4) + "FIVE\nextra1\nextra2\n" + nums(6, 29) + "THIRTY\n" + nums(31, 40)))
case("reject-with-timestamps", "t p", t=lines("1", "zz", "3"), p=udiff(nums(1, 3), lines("1", "TWO", "3"), "f", "f", 3, "2020-01-02 03:04:05.000000000 +0000", "2020-01-02 03:04:06.000000000 +0000"))
case("reject-no-newline-at-end", "t p", t=nums(1, 3), p="--- f\n+++ f\n@@ -1,3 +1,3 @@\n 1\n-X\n+2\n-3\n\\ No newline at end of file\n+3\n")
case("reject-only-fails-when-dry-run", "--dry-run t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-quiet", "-s t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-force", "-f t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("reject-missing-named-file", "nosuch pu", pu=PU)
case("reject-no-backup-if-mismatch", "--no-backup-if-mismatch t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)

# Reversed patches
base40 = nums(1, 40)
both = nums(1, 4) + "FIVE\n" + nums(6, 29) + "THIRTY\n" + nums(31, 40)
rp = udiff(base40, both)
case("reverse-explicit", "-R t p", t=both, p=rp)
case("reverse-explicit-long", "--reverse t p", t=both, p=rp)
case("reverse-detected-default", "t p", t=both, p=rp)
case("reverse-detected-force", "-f t p", t=both, p=rp)
case("reverse-detected-batch", "-t t p", t=both, p=rp)
case("reverse-detected-forward", "-N t p", t=both, p=rp)
case("reverse-detected-forward-quiet", "-s -N t p", t=both, p=rp)
case("reverse-second-hunk-applied-default", "t p", t=nums(1, 29) + "THIRTY\n" + nums(31, 40), p=rp)
case("reverse-second-hunk-applied-forward", "-N t p", t=nums(1, 29) + "THIRTY\n" + nums(31, 40), p=rp)
case("reverse-first-hunk-applied-force", "-f t p", t=nums(1, 4) + "FIVE\n" + nums(6, 40), p=rp)
case("unreversed-with-r-default", "-R t p", t=base40, p=rp)
case("unreversed-with-r-batch", "-R -t t p", t=base40, p=rp)
case("unreversed-with-r-force", "-R -f t p", t=base40, p=rp)
case("unreversed-with-r-forward", "-R -N t p", t=base40, p=rp)
case("reverse-reject", "-R t p", t=nums(1, 4) + "zz\n" + nums(6, 29) + "THIRTY\n" + nums(31, 40), p=rp)
case("reverse-context-diff", "-R t pc", t=EDITED, pc=PC)
case("reverse-normal-diff", "-R t pn", t=EDITED, pn=PN)

# File name selection
sel = {"a__f": "a\n", "b__f": "a\n", "g": "a\n", "d__e__h": "a\n"}


def sel_patch(old, new):
    return "--- %s\n+++ %s\n@@ -1 +1 @@\n-a\n+z\n" % (old, new)


for tag, old, new, opts in [
    ("both-exist-old-wins", "a/f", "b/f", "-p0"),
    ("old-exists-new-missing", "a/f", "nob/f", "-p0"),
    ("old-missing-new-exists", "noa/f", "b/f", "-p0"),
    ("neither-exists", "noa/f", "nob/f", "-p0"),
    ("fewer-components-wins", "g", "a/f", "-p0"),
    ("fewer-components-wins-new", "a/f", "g", "-p0"),
    ("deep-vs-shallow", "d/e/h", "g", "-p0"),
    ("strip-one-missing", "a/f", "b/f", "-p1"),
    ("strip-default-basename", "a/f", "b/f", ""),
    ("old-devnull-new-exists", "/dev/null", "g", "-p0"),
    ("new-devnull-old-exists", "g", "/dev/null", "-p0"),
    ("devnull-then-missing", "/dev/null", "noexist", "-p0"),
    ("missing-then-devnull", "noexist", "/dev/null", "-p0"),
]:
    case("select-" + tag, opts, stdin=sel_patch(old, new), **sel)
case("select-strip-two-components", "-p2", stdin="--- x/y/f\n+++ x/y/f\n@@ -1 +1 @@\n-a\n+z\n", f="a\n", x__y__f="a\n")
case("select-strip-too-many", "-p3", stdin="--- x/y/f\n+++ x/y/f\n@@ -1 +1 @@\n-a\n+z\n", f="a\n", x__y__f="a\n")
case("select-strip-doubled-slashes", "-p1", stdin="--- x//y/f\n+++ x//y/f\n@@ -1 +1 @@\n-a\n+z\n", y__f="a\n")
case("select-strip-bad-number", "-p x", stdin=sel_patch("f", "f"), f="a\n")
case("select-strip-negative", "-p -1", stdin=sel_patch("f", "f"), f="a\n")
case("select-strip-long-form", "--strip=1", stdin=sel_patch("a/f", "a/f"), f="a\n")
case("select-name-with-tab-timestamp", "", stdin="--- a b\t2020-01-01\n+++ a b\t2020-01-01\n@@ -1 +1 @@\n-a\n+z\n", **{"a%20b": "a\n"})
case("select-name-ends-at-space", "", stdin="--- a b\n+++ a b\n@@ -1 +1 @@\n-a\n+z\n", **{"a%20b": "a\n"})
case("select-name-quoted", "", stdin="--- \"a b\"\n+++ \"a b\"\n@@ -1 +1 @@\n-a\n+z\n", **{"a%20b": "a\n"})
case("select-name-with-timestamp-no-tab", "", stdin="--- f 2020-01-01 10:00:00.000000000 +0000\n+++ f 2020-01-01 10:00:00.000000000 +0000\n@@ -1 +1 @@\n-a\n+z\n", f="a\n")
case("select-dotdot-refused", "-p0", stdin="--- /dev/null\n+++ ../outside\n@@ -0,0 +1 @@\n+bad\n")
case("select-dotdot-inside-refused", "-p0", stdin="--- /dev/null\n+++ sub/../x\n@@ -0,0 +1 @@\n+bad\n")
case("select-absolute-refused", "-p0", stdin="--- /dev/null\n+++ /tmp/abs_x\n@@ -0,0 +1 @@\n+bad\n")
case("select-symlink-directory-refused", "-p0", stdin="--- /dev/null\n+++ lnk/zz\n@@ -0,0 +1 @@\n+bad\n", links={"lnk": "elsewhere"}, dirs=["elsewhere"])
case("select-not-found-quiet", "-s", stdin=sel_patch("nof", "nof"))
case("select-not-found-force", "-f", stdin=sel_patch("nof", "nof"))
case("select-not-found-batch", "-t", stdin=sel_patch("nof", "nof"))
case("select-not-found-two-hunks", "", stdin="--- nf\n+++ nf\n@@ -1 +1 @@\n-a\n+z\n@@ -5 +5 @@\n-b\n+y\n")
case("select-not-found-preamble", "", stdin="some preamble\n--- nf\n+++ nf\n@@ -1 +1 @@\n-a\n+z\n")
case("select-operand-overrides-headers", "g", stdin=sel_patch("nope", "nope2"), g="a\n")
case("select-operand-with-p", "-p1 g", stdin=sel_patch("a/f", "a/f"), g="a\n")

# Create and delete
PCREATE = "--- /dev/null\n+++ n\n@@ -0,0 +1,2 @@\n+a\n+b\n"
PDELETE = "--- d\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n"
case("create-new-file", "", stdin=PCREATE)
case("create-over-empty-file", "", stdin=PCREATE, n="")
case("create-over-content-default", "", stdin=PCREATE, n="old\n")
case("create-over-content-force", "-f", stdin=PCREATE, n="old\n")
case("create-over-content-batch", "-t", stdin=PCREATE, n="old\n")
case("create-over-content-forward", "-N", stdin=PCREATE, n="old\n")
case("create-over-matching-content", "", stdin=PCREATE, n="a\nb\n")
case("create-via-zero-range-same-names", "", stdin="--- n\n+++ n\n@@ -0,0 +1,2 @@\n+a\n+b\n")
case("create-via-epoch-timestamp", "", stdin="--- n\t1970-01-01 00:00:00.000000000 +0000\n+++ n\t2020-01-01 00:00:00.000000000 +0000\n@@ -0,0 +1,2 @@\n+a\n+b\n")
case("create-in-new-directories", "-p0", stdin="--- /dev/null\n+++ sub/dir/n\n@@ -0,0 +1,2 @@\n+a\n+b\n")
case("create-named-operand", "m", stdin=PCREATE)
case("create-reversed-removes-file", "-R -p0", stdin="--- /dev/null\n+++ n\n@@ -0,0 +1,2 @@\n+a\n+b\n", n="a\nb\n")
case("create-reversed-removes-empty-directories", "-R -p0", stdin="--- /dev/null\n+++ sub/dir/n\n@@ -0,0 +1,2 @@\n+a\n+b\n", sub__dir__n="a\nb\n")
case("create-reversed-nonexistent", "-R -p0", stdin="--- /dev/null\n+++ n\n@@ -0,0 +1,2 @@\n+a\n+b\n")
case("create-context-diff", "", stdin="*** /dev/null\n--- n\n***************\n*** 0 ****\n--- 1,2 ----\n+ a\n+ b\n")
case("delete-existing", "", stdin=PDELETE, d="a\nb\n")
case("delete-missing-default", "", stdin=PDELETE)
case("delete-missing-force", "-f", stdin=PDELETE)
case("delete-missing-batch", "-t", stdin=PDELETE)
case("delete-missing-forward", "-N", stdin=PDELETE)
case("delete-mismatched-content", "", stdin=PDELETE, d="a\nx\n")
case("delete-extra-content", "", stdin=PDELETE, d="a\nb\nc\n")
case("delete-same-names-leaves-empty", "", stdin="--- delf\n+++ delf\n@@ -1,2 +0,0 @@\n-a\n-b\n", delf="a\nb\n")
case("delete-same-names-with-E", "-E", stdin="--- delf\n+++ delf\n@@ -1,2 +0,0 @@\n-a\n-b\n", delf="a\nb\n")
case("delete-epoch-new-name", "", stdin="--- delf\n+++ delf\t1970-01-01 00:00:00.000000000 +0000\n@@ -1,2 +0,0 @@\n-a\n-b\n", delf="a\nb\n")
case("delete-with-E-and-content-left", "-E", stdin="--- n\n+++ n\n@@ -1 +1 @@\n-old\n+\n", n="old\n")
case("delete-E-long", "--remove-empty-files", stdin="--- n\n+++ n\n@@ -1 +0,0 @@\n-old\n", n="old\n")
case("delete-removes-empty-directories", "-p0", stdin="--- sub/dir/n\n+++ /dev/null\n@@ -1 +0,0 @@\n-a\n", sub__dir__n="a\n")
case("devnull-with-ordinary-hunk", "", stdin="--- /dev/null\n+++ n\n@@ -1 +1 @@\n-old\n+new\n", n="old\n")
case("delete-devnull-ordinary-hunk", "-p0", stdin="--- g\n+++ /dev/null\n@@ -1 +1 @@\n-a\n+z\n", g="a\n")

# Backups
SUBPU = PU.replace("--- f\n+++ f\n", "--- sub/t\n+++ sub/t\n")
case("backup-flag", "-b t pu", t=BASE, pu=PU)
case("backup-long", "--backup t pu", t=BASE, pu=PU)
case("backup-suffix", "-b -z .bak t pu", t=BASE, pu=PU)
case("backup-suffix-long", "--backup --suffix=.sfx t pu", t=BASE, pu=PU)
case("backup-prefix-new-directory", "-b -B bk/ t pu", t=BASE, pu=PU)
case("backup-prefix-existing-directory", "-b -B bk/ t pu", dirs=["bk"], t=BASE, pu=PU)
case("backup-basename-prefix", "-b -Y pre- t pu", t=BASE, pu=PU)
case("backup-basename-prefix-in-directory", "-p0 -b -Y pre-", stdin=SUBPU, sub__t=BASE)
case("backup-prefix-in-directory", "-p0 -b -B pre-", stdin=SUBPU, sub__t=BASE)
case("backup-prefix-and-suffix", "-p0 -b -B bk/ -z .x", stdin=SUBPU, sub__t=BASE)
case("backup-numbered", "-p0 -b -V numbered", stdin=SUBPU, sub__t=BASE)
case("backup-numbered-continues", "-p0 -b -V numbered", stdin=SUBPU, sub__t=BASE, **{"sub/t.~1~": "one\n", "sub/t.~2~": "two\n"})
case("backup-existing-with-numbered", "-p0 -b -V existing", stdin=SUBPU, sub__t=BASE, **{"sub/t.~1~": "one\n"})
case("backup-existing-without-numbered", "-p0 -b -V existing", stdin=SUBPU, sub__t=BASE)
case("backup-simple-with-numbered", "-p0 -b -V simple", stdin=SUBPU, sub__t=BASE, **{"sub/t.~1~": "one\n"})
case("backup-never-alias", "-p0 -b -V never", stdin=SUBPU, sub__t=BASE, **{"sub/t.~1~": "one\n"})
case("backup-none-follows-existing", "-p0 -b -V none", stdin=SUBPU, sub__t=BASE, **{"sub/t.~1~": "one\n"})
case("backup-t-alias-numbered", "-p0 -b -V t", stdin=SUBPU, sub__t=BASE)
case("backup-bad-style", "-p0 -b -V bogus", stdin=SUBPU, sub__t=BASE)
case("backup-style-from-patch-env", "-p0 -b", stdin=SUBPU, sub__t=BASE, env={"PATCH_VERSION_CONTROL": "numbered"})
case("backup-style-from-env", "-p0 -b", stdin=SUBPU, sub__t=BASE, env={"VERSION_CONTROL": "numbered"})
case("backup-if-mismatch-offset", "t pu", t="a\n" + BASE, pu=PU)
case("backup-if-mismatch-posix-env", "t pu", t="a\n" + BASE, pu=PU, env={"POSIXLY_CORRECT": "1"})
case("backup-if-mismatch-posix-option", "--posix t pu", t="a\n" + BASE, pu=PU)
case("backup-if-mismatch-explicit", "--backup-if-mismatch t pu", t="a\n" + BASE, pu=PU, env={"POSIXLY_CORRECT": "1"})
case("backup-no-backup-if-mismatch", "--no-backup-if-mismatch t pu", t="a\n" + BASE, pu=PU)
case("backup-clean-apply-makes-none", "t pu", t=BASE, pu=PU)
case("backup-with-output-option", "-b -o out t pu", t=BASE, pu=PU)
case("backup-created-file-empty-backup", "-b", stdin="--- /dev/null\n+++ n\n@@ -0,0 +1 @@\n+a\n")
case("backup-named-missing-file-fails", "-b nosuch p", p="--- f\n+++ f\n@@ -1 +1 @@\n-a\n+b\n")

# Miscellaneous options
case("directory-option", "-d dir1 t pu", dir1__t=BASE, pu=PU)
case("directory-option-input-inside", "-d dir1 -i pu t", dir1__t=BASE, dir1__pu=PU)
case("directory-option-input-outside", "-d dir1 -i ../pu t", dir1__t=BASE, pu=PU)
case("directory-option-long", "--directory=dir1 -i ../pu t", dir1__t=BASE, pu=PU)
case("directory-option-missing", "-d nosuchdir -i pu", pu=PU)
case("directory-option-not-a-directory", "-d t -i pu", t=BASE, pu=PU)
case("input-option", "-i pu t", t=BASE, pu=PU)
case("input-option-long", "--input=pu t", t=BASE, pu=PU)
case("input-option-missing", "-i nosuch t", t=BASE)
case("input-option-and-second-operand", "-i pu t other", t=BASE, pu=PU)
case("input-stdin-dash", "t -", stdin=PU, t=BASE)
case("output-option", "-o out t pu", t=BASE, pu=PU)
case("output-option-new-directory-exists", "-o od/out t pu", dirs=["od"], t=BASE, pu=PU)
case("output-option-stdout", "-o - t pu", t=BASE, pu=PU)
case("output-option-stdout-quiet", "-s -o - t pu", t=BASE, pu=PU)
case("output-option-reject", "-o out2 t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("output-option-long", "--output=oo t pu", t=BASE, pu=PU)
case("quiet-flag", "-s t pu", t=BASE, pu=PU)
case("quiet-long-silent", "--silent t pu", t=BASE, pu=PU)
case("quiet-long-quiet", "--quiet t pu", t="a\n" + BASE, pu=PU)
case("verbose-flag", "--verbose t pu", t=BASE, pu=PU)
case("verbose-context-diff", "--verbose t pc", t=BASE, pc=PC)
case("verbose-normal-diff", "--verbose t pn", t=BASE, pn=PN)
case("verbose-two-patches", "--verbose t", stdin=PU + PU, t=BASE)
case("verbose-preamble", "--verbose t", stdin="hello\nthere\n" + PU, t=BASE)
case("verbose-hunk-only", "--verbose t", stdin="@@ -1,3 +1,3 @@\n 1\n-2\n+X\n 3\n", t=BASE)
case("verbose-failure", "--verbose t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("verbose-trailing-garbage", "--verbose t", stdin=PU + "garbage\n", t=BASE)
case("dry-run-success", "--dry-run t pu", t=BASE, pu=PU)
case("dry-run-quiet", "--dry-run -s t pu", t=BASE, pu=PU)
case("dry-run-failure", "--dry-run t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("dry-run-create", "--dry-run", stdin=PCREATE)
case("dry-run-backup", "--dry-run -b t pu", t=BASE, pu=PU)
case("force-flag", "--force t pu", t=BASE, pu=PU)
case("batch-flag", "--batch t pu", t=BASE, pu=PU)
case("forward-flag", "--forward t pu", t=BASE, pu=PU)
case("fuzz-long", "--fuzz=3 t pu", t=BASE, pu=PU)
case("strip-and-quiet-long", "--input pu --strip=0 --quiet t", t=BASE, pu=PU)
case("remove-empty-long-no-effect", "--remove-empty-files t pu", t=BASE, pu=PU)
case("double-dash-operands", "-- t pu", t=BASE, pu=PU)
case("options-after-operands", "t pu -s", t=BASE, pu=PU)
case("option-bundling", "-sp0 t pu", t=BASE, pu=PU)
case("option-abbreviation", "--quie t pu", t=BASE, pu=PU)
case("option-ambiguous-abbreviation", "--re t pu", t=BASE, pu=PU)
case("help-flag", "--help")
case("unknown-long-option", "--bogus")
case("unknown-short-option", "-k t pu", t=BASE, pu=PU)
case("missing-option-argument-short", "-z")
case("missing-option-argument-long", "--prefix")
case("extra-operand", "t pu extra", t=BASE, pu=PU)
case("merge-clean-apply", "--merge t pu", t=BASE, pu=PU)
case("ifdef-merged-output", "-D FOO t pu", t=BASE, pu=PU)
case("ifdef-long", "--ifdef=FOO t pu", t=BASE, pu=PU)
case("ifdef-with-failed-hunk", "-D FOO t pu", t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU)
case("get-zero-accepted", "-g 0 t pu", t=BASE, pu=PU)
case("get-bad-number", "-g x t pu", t=BASE, pu=PU)
case("follow-symlinks-accepted", "--follow-symlinks t pu", t=BASE, pu=PU)
case("posix-accepted", "--posix t pu", t=BASE, pu=PU)
case("binary-flag-accepted", "--binary t pu", t=BASE, pu=PU)
case("read-only-warn", "--read-only=warn t pu", t=BASE, pu=PU)
case("read-only-bad-value", "--read-only=bogus t pu", t=BASE, pu=PU)
QA = "--- \"a b\"\n+++ \"a b\"\n@@ -1 +1 @@\n-1\n+2\n"
case("quoting-style-literal", "--quoting-style=literal", stdin=QA, **{"a%20b": "1\n"})
case("quoting-style-c", "--quoting-style=c", stdin=QA, **{"a%20b": "1\n"})
case("quoting-style-shell-always", "--quoting-style=shell-always", stdin=QA, **{"a%20b": "1\n"})
case("quoting-style-escape", "--quoting-style=escape", stdin=QA, **{"a%20b": "1\n"})
case("quoting-style-env", "", stdin=QA, env={"QUOTING_STYLE": "c"}, **{"a%20b": "1\n"})
case("quoting-style-bad", "--quoting-style=bogus", stdin="--- f\n+++ f\n@@ -1 +1 @@\n-1\n+2\n", f="1\n")
case("quoted-name-default-shell", "", stdin=QA, **{"a%20b": "1\n"})

# Prereq
PR = "Prereq: %s\n--- t\n+++ t\n@@ -1,2 +1,2 @@\n version %s\n-x\n+y\n"
case("prereq-missing-default", "t p", t="version 2.0\nx\n", p=PR % ("1.0", "2.0"))
case("prereq-present", "t p", t="version 1.0\nx\n", p=PR % ("1.0", "1.0"))
case("prereq-missing-force", "-f t p", t="version 2.0\nx\n", p=PR % ("1.0", "2.0"))
case("prereq-missing-batch", "-t t p", t="version 2.0\nx\n", p=PR % ("1.0", "2.0"))
case("prereq-empty", "t p", t="version 2.0\nx\n", p=PR % ("", "2.0"))
case("prereq-multiple-words", "t p", t="version 2.0\nx\n", p=PR % ("1.0 2.0", "2.0"))

# Line endings, missing final newlines, binary content
LF = "a\nb\nc\n"
CRLF = "a\r\nb\r\nc\r\n"
NONL = "a\nb\nc"
P_LF = "--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n"
P_CRLF = "--- t\r\n+++ t\r\n@@ -1,3 +1,3 @@\r\n a\r\n-b\r\n+B\r\n c\r\n"
P_NL1 = "--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n c\n\\ No newline at end of file\n"
P_NL2 = "--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n-c\n\\ No newline at end of file\n+c\n"
P_NL3 = "--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n-b\n+B\n-c\n+C\n\\ No newline at end of file\n"
for src, content in [("lf", LF), ("crlf", CRLF), ("nonl", NONL)]:
    for pname, patch in [("plf", P_LF), ("pcrlf", P_CRLF), ("pnl1", P_NL1), ("pnl2", P_NL2), ("pnl3", P_NL3)]:
        case("endings-%s-%s" % (src, pname), "t p", t=content, p=patch)
        case("endings-%s-%s-binary" % (src, pname), "--binary t p", t=content, p=patch)
case("endings-crlf-patch-two-files", "", stdin=P_CRLF.replace("t\r\n", "x\r\n") + P_CRLF.replace("t\r\n", "y\r\n"), x=LF, y=LF)
case("endings-mismatch-first-line-only", "t p", t="a\r\nb\r\nc\n", p="--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n-X\n+B\n c\n")
case("endings-lf-file-cr-in-second-line", "--binary t p", t="a\nb\r\nc\n", p="--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n-X\n+B\n c\n")
case("binary-nul-bytes", "t p", t=b"a\0b\n\xff\xfe\nc\n", p=b"--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\0b\n-\xff\xfe\n+\xfd\n c\n")
case("binary-non-utf8-context-fails", "t p", t=b"a\xff\nb\n", p=b"--- t\n+++ t\n@@ -1,2 +1,2 @@\n a\xfe\n-b\n+B\n")
case("empty-file-add", "t p", t="", p="--- t\n+++ t\n@@ -0,0 +1,2 @@\n+a\n+b\n")
case("empty-result-keeps-file", "t p", t="a\nb\n", p="--- t\n+++ t\n@@ -1,2 +0,0 @@\n-a\n-b\n")
case("long-lines", "t p", t="x" * 20000 + "\nsecond\n", p="--- t\n+++ t\n@@ -1,2 +1,2 @@\n " + "x" * 20000 + "\n-second\n+2nd\n")
case("blank-context-lines", "t p", t="a\n\nb\n", p="--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n \n-b\n+B\n")
case("empty-context-line-without-space", "t p", t="a\n\nb\n", p="--- t\n+++ t\n@@ -1,3 +1,3 @@\n a\n\n-b\n+B\n")
case("many-files", "", stdin="".join("--- m%d\n+++ m%d\n@@ -1 +1 @@\n-%d\n+N%d\n" % (i, i, i, i) for i in range(1, 13)), **{"m%d" % i: "%d\n" % i for i in range(1, 13)})
case("many-hunks", "t p", t=nums(1, 200), p=udiff(nums(1, 200), "".join(("X%d\n" % n) if n % 10 == 0 else "%d\n" % n for n in range(1, 201)), n=1))
case("trailing-whitespace-sensitive", "t p", t="a \nb\n", p="--- t\n+++ t\n@@ -1,2 +1,2 @@\n a\n-b\n+B\n")

# Malformed patches
case("malformed-hunk-header", "t p", t=BASE, p="--- f\n+++ f\n@@ -1,2 @@\n 1\n-2\n+X\n")
case("malformed-body-line", "t p", t=BASE, p="--- f\n+++ f\n@@ -1,3 +1,3 @@\n 1\n?oops\n 3\n")
case("malformed-eof-in-hunk", "t p", t=BASE, p="--- f\n+++ f\n@@ -1,3 +1,3 @@\n 1\n-2\n")
case("malformed-count-mismatch", "t p", t=BASE, p="--- f\n+++ f\n@@ -1,5 +1,6 @@\n 1\n-2\n+TWO\n 3\n 4\n 5\n")
case("truncated-trailing-context", "t p", t=nums(1, 5), p="--- f\n+++ f\n@@ -1,5 +1,5 @@\n 1\n-2\n+TWO\n 3\n 4\n")
case("extra-lines-after-counts", "t p", t=nums(1, 5), p="--- f\n+++ f\n@@ -1,3 +1,3 @@\n 1\n-2\n+TWO\n 3\n 4\n 5\n")
case("missing-plus-header", "t p", t=nums(1, 5), p="--- f\n@@ -1,3 +1,3 @@\n 1\n-2\n+TWO\n 3\n")
case("malformed-after-good-file", "", stdin=udiff("a\n", "A\n", "x", "x") + "--- y\n+++ y\n@@ -1,2 +1,2 @@\n a\n-b\n", x="a\n", y="a\nb\n")
case("context-missing-new-half", "t p", t=BASE, p="*** f\n--- f\n***************\n*** 1,2 ****\n  1\n- 2\n")

# Special files
RO = "--- t\n+++ t\n@@ -1,3 +1,3 @@\n 1\n 2\n-3\n+X\n"
case("read-only-file-default", "t p", t="1\n2\n3\n", p=RO, chmod=[("444", "t")])
case("read-only-file-ignore", "--read-only=ignore t p", t="1\n2\n3\n", p=RO, chmod=[("444", "t")])
case("read-only-file-fail", "--read-only=fail t p", t="1\n2\n3\n", p=RO, chmod=[("444", "t")])
case("read-only-file-quiet", "-s t p", t="1\n2\n3\n", p=RO, chmod=[("444", "t")])
case("directory-as-target", "dd p", dirs=["dd"], p=RO)
case("symlink-as-target", "lnk p", real="1\n2\n3\n", p=RO, links={"lnk": "real"})
case("symlink-as-target-follow", "--follow-symlinks lnk p", real="1\n2\n3\n", p=RO, links={"lnk": "real"})
case("symlink-dangling-target", "lnk p", p=RO, links={"lnk": "nonexist"})
case("preserves-executable-bit", "t p", t="1\n2\n3\n", p=RO, chmod=[("755", "t")])
case("preserves-mode-on-backup", "-b t p", t="1\n2\n3\n", p=RO, chmod=[("700", "t")])

# Timestamps
TZP = "--- tz\t2001-02-03 04:05:06.000000000 +0000\n+++ tz\t2005-06-07 08:09:10.123456789 +0000\n@@ -1 +1 @@\n-1\n+2\n"
case("set-utc-time-matches", "-Z tz p", tz="1\n", p=TZP, touch=[("981173106", "tz")], mtimes=["tz"])
case("set-time-matches", "-T tz p", tz="1\n", p=TZP, touch=[("981173106", "tz")], mtimes=["tz"])
case("set-utc-time-mismatch", "-Z tz p", tz="1\n", p=TZP, touch=[("1000000000", "tz")])
case("set-utc-contents-mismatch", "-Z tz p", tz="a\n1\n", p=TZP, touch=[("981173106", "tz")])
case("set-utc-only-new-stamp", "-Z tz p", tz="1\n", p="--- tz\n+++ tz\t2005-06-07 08:09:10.000000000 +0000\n@@ -1 +1 @@\n-1\n+2\n", touch=[("1000000000", "tz")], mtimes=["tz"])
case("set-utc-no-stamps", "-Z tz p", tz="1\n", p="--- tz\n+++ tz\n@@ -1 +1 @@\n-1\n+2\n", touch=[("1000000000", "tz")])
case("set-utc-quiet", "-s -Z tz p", tz="1\n", p=TZP, touch=[("1000000000", "tz")])
case("set-utc-dry-run", "--dry-run -Z tz p", tz="1\n", p=TZP, touch=[("981173106", "tz")], mtimes=["tz"])
case("set-utc-long", "--set-utc tz p", tz="1\n", p=TZP, touch=[("981173106", "tz")], mtimes=["tz"])
case("set-time-long", "--set-time tz p", tz="1\n", p=TZP, touch=[("981173106", "tz")], mtimes=["tz"])
case("set-utc-created-file", "-Z", stdin="--- tz\t1970-01-01 00:00:00.000000000 +0000\n+++ tz\t2005-06-07 08:09:10.000000000 +0000\n@@ -0,0 +1 @@\n+1\n", mtimes=["tz"])

# Git patches
G_MOD = "diff --git a/a b/a\nindex 1111111..2222222 100644\n--- a/a\n+++ b/a\n@@ -1,3 +1,3 @@\n 1\n-2\n+TWO\n 3\n"
G_NEW = "diff --git a/newf b/newf\nnew file mode 100755\nindex 0000000..2222222\n--- /dev/null\n+++ b/newf\n@@ -0,0 +1,2 @@\n+hello\n+world\n"
G_DEL = "diff --git a/a b/a\ndeleted file mode 100644\nindex 1111111..0000000\n--- a/a\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-1\n-2\n-3\n"
G_REN = "diff --git a/a b/renamed\nsimilarity index 100%\nrename from a\nrename to renamed\n"
G_RENEDIT = "diff --git a/a b/renamed\nsimilarity index 66%\nrename from a\nrename to renamed\nindex 1111111..2222222 100644\n--- a/a\n+++ b/renamed\n@@ -1,3 +1,3 @@\n 1\n-2\n+TWO\n 3\n"
G_COPY = "diff --git a/a b/copied\nsimilarity index 100%\ncopy from a\ncopy to copied\n"
G_MODE = "diff --git a/exe b/exe\nold mode 100644\nnew mode 100755\n"
G_MODEBACK = "diff --git a/exe b/exe\nold mode 100755\nnew mode 100644\n"
G_BIN = "diff --git a/bin b/bin\nnew file mode 100644\nindex 0000000..2222222\nGIT binary patch\nliteral 4\nLcmZQzU|?VZ00jU6\n\n"
G_LINK = "diff --git a/ln b/ln\nnew file mode 120000\nindex 0000000..2222222\n--- /dev/null\n+++ b/ln\n@@ -0,0 +1 @@\n+target\n\\ No newline at end of file\n"
G_EMPTY = "diff --git a/e b/e\nnew file mode 100644\nindex 0000000..e69de29\n"
G_ACROSS = "diff --git a/sub/g b/other/h\nsimilarity index 100%\nrename from sub/g\nrename to other/h\n"
case("git-modify", "-p1", stdin=G_MOD, a="1\n2\n3\n")
case("git-modify-no-strip", "", stdin=G_MOD, a="1\n2\n3\n")
case("git-modify-p0-fails", "-p0", stdin=G_MOD, a="1\n2\n3\n")
case("git-new-executable", "-p1", stdin=G_NEW)
case("git-delete", "-p1", stdin=G_DEL, a="1\n2\n3\n")
case("git-rename", "-p1", stdin=G_REN, a="1\n2\n3\n")
case("git-rename-with-edit", "-p1", stdin=G_RENEDIT, a="1\n2\n3\n")
case("git-copy", "-p1", stdin=G_COPY, a="1\n2\n3\n")
case("git-mode-add-exec", "-p1", stdin=G_MODE, exe="x\n", chmod=[("644", "exe")])
case("git-mode-remove-exec", "-p1", stdin=G_MODEBACK, exe="x\n", chmod=[("755", "exe")])
case("git-mode-no-strip", "", stdin=G_MODE, exe="x\n")
case("git-binary-refused", "-p1", stdin=G_BIN)
case("git-new-symlink", "-p1", stdin=G_LINK)
case("git-new-empty-file", "-p1", stdin=G_EMPTY)
case("git-rename-into-new-directory", "-p1", stdin=G_ACROSS, sub__g="y\n")
case("git-rename-missing-source", "-p1", stdin=G_REN)
case("git-two-patches", "-p1", stdin=G_MOD + G_MODE, a="1\n2\n3\n", exe="x\n")
case("git-reverse", "-R -p1", stdin=G_MOD, a="1\nTWO\n3\n")
case("git-dry-run", "--dry-run -p1", stdin=G_MOD, a="1\n2\n3\n")
case("git-failure-rejects-unified", "-p1", stdin=G_MOD, a="1\nx\n3\n")
case("git-verbose", "--verbose -p1", stdin=G_MOD, a="1\n2\n3\n")
case("git-rename-reverse", "-R -p1", stdin=G_REN, renamed="1\n2\n3\n")
case("git-rename-dry-run", "--dry-run -p1", stdin=G_REN, a="1\n2\n3\n")

# Operand, option, and error edge cases
case("patch-file-is-directory", "-i pd t", dirs=["pd"], t=BASE)
case("patch-file-unreadable", "-i pu t", t=BASE, pu=PU, chmod=[("000", "pu")])
case("output-file-in-missing-directory", "-o nodir/out t pu", t=BASE, pu=PU)
case("output-file-in-unwritable-directory", "-o ro/out t pu", dirs=["ro"], t=BASE, pu=PU, chmod=[("555", "ro")])
case("reject-file-in-unwritable-directory", "-r ro/rej t pu", dirs=["ro"], t=lines("1", "2", "3", "zz", "5", "6", "7", "8", "yy", "10", "11", "12"), pu=PU, chmod=[("555", "ro")])
case("fuzz-negative", "-F -1 t pu", t=BASE, pu=PU)
case("fuzz-huge", "-F 999 t pu", t=BASE, pu=PU)
case("strip-huge", "-p 99999999999999999999 t pu", t=BASE, pu=PU)
case("repeated-options", "-s -s -b -b -V numbered -V simple t pu", t=BASE, pu=PU)
case("operand-dash-is-a-file", "- pu", pu=PU)
case("patch-ends-mid-line", "t p", t=BASE, p="--- f\n+++ f\n@@ -1 +1 @@\n-1\n+2")
case("append-after-final-line-without-newline", "t p", t="1\n2", p="--- f\n+++ f\n@@ -2 +2,2 @@\n-2\n\\ No newline at end of file\n+2\n+3\n")
case("loose-whitespace-final-newline", "-l t p", t="a\nb", p="--- f\n+++ f\n@@ -1,2 +1,2 @@\n a\n-b\n+B\n")
case("unreadable-target", "t pu", t=BASE, pu=PU, chmod=[("000", "t")])
case("ed-script-change-range", "t pe", t=BASE, pe="3,5c\nreplaced\n.\n")
case("ed-script-delete-range-and-append-end", "t pe", t=BASE, pe="10,12d\n3a\nafter three\n.\n$a\nlast\n.\n")
case("ed-script-insert", "t pe", t=BASE, pe="1i\nfirst\n.\n")
case("ed-script-bad-command", "t pe", t=BASE, pe="1,2x\n")
case("ed-script-dry-run", "--dry-run t pe", t=BASE, pe="1d\n")
case("ed-script-backup", "-b t pe", t=BASE, pe="1d\n")
case("ed-script-dollar-address-ignored", "t pe", t=BASE, pe="10,12d\n3a\nafter three\n.\n$a\nlast\n.\n")
case("ed-script-substitute-ignored", "t pe", t=BASE, pe="3a\n..\n.\ns/^\\.//\n")
case("ed-script-not-a-script", "t pe", t=BASE, pe="2s/b/BEE/\n")
