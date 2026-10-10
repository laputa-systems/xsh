"""Differential cases for the sed applet: fixtures and the case table.

Every case runs `sed ARGS` in a fresh copy of the fixtures, with the extra
links, modes and directories that run_script() in regenerate.py creates, and
records status, stdout, stderr and the state of the files named in `show`.
"""

import re

FIXTURES = {
    "a.txt": b"alpha\nbeta\ngamma\ndelta\nepsilon\n",
    "n.txt": b"".join(b"%d\n" % i for i in range(1, 11)),
    "nonl.txt": b"x\ny\nz",
    "empty.txt": b"",
    "one.txt": b"only\n",
    "onenl.txt": b"only",
    "blank.txt": b"\n\n\n",
    "crlf.txt": b"a\r\nb\r\nc\r\n",
    "bin.bin": b"a\x00b\nc\x00\x00d\n\xff\xfe\n",
    "latin1.txt": b"caf\xe9\nna\xefve\n",
    "utf8.txt": "café\nnaïve\nüber\nstraße\n".encode(),
    "long.txt": b"abcdefghij" * 30 + b"\nshort\n",
    "words.txt": b"hello world\nfoo bar baz\nThe Quick brown FOX\nfoo\n",
    "dup.txt": b"aaa\nabab\nabcabc\n\nx  y\nfoo\n",
    "multi.txt": b"a b\nc d\ne f\ng h\n",
    "ctl.txt": b"tab\there\nback\\slash\nbell\x07\x1b\x7f\nend\n",
    "re.txt": b"a.b\na*b\n[x]\n(y)\na+b\nab\naab\naaab\n^a$\nfoo|bar\n",
    "csv.txt": b"name,age,city\nann,30,paris\nbob,25,rome\n",
    "path.txt": b"/usr/local/bin\n/etc/passwd\nrelative/path\n",
    "rfile.txt": b"R1\nR2\n",
    "cmds.txt": b"echo one\nprintf 'a\\nb\\n'\ntrue\n",
    "rnonl.txt": b"RR",
    "d/f.txt": b"in dir\n",
    "rodir/f.txt": b"read only dir\n",
    "noaccess.txt": b"secret\n",
    "ro.txt": b"readonly\n",
    "s_p.sed": b"p\n",
    "s_nonl.sed": b"s/a/A/",
    "s_n.sed": b"#n\n2p\n",
    "s_n2.sed": b"#no\n2p\n",
    "s_nx.sed": b"#n\np\n",
    "s_text.sed": b"1i\\\nfirst\n2a\\\nsecond\\\nthird\n3c\\\nchanged\n$a end\n",
    "s_cmt.sed": b"# comment\n/a/ { # trailing\n  s/a/A/ # x\n  p\n}\n",
    "s_multi.sed": b"s/a/b/\ns/b/c/;s/c/d/\n\n  p\n",
    "s_blk.sed": b"/e/{\n  n\n  d\n}\n",
    "s_crlf.sed": b"p\r\n",
    "s_label.sed": b":a\ns/aa/a/\nta\n",
    "s_bad.sed": b"p\nk\n",
    "s_bad2.sed": b"s/a/b\n",
    "s_open.sed": b"1{\np\n",
    "s_y.sed": b"y/abc/xyz/\n",
    "s_w.sed": b"w out.txt\n",
    "s_r.sed": b"r rfile.txt\n",
    "s_q.sed": b"2q5\n",
    "s_sub.sed": b"s/\\(a\\)\\(l\\)/\\2\\1/\n",
}

CASES = []
_seen = {}


def add(group, args, stdin=None, show=(), env=None, setup=()):
    n = _seen.get(group, 0) + 1
    _seen[group] = n
    case = {"name": "%s-%03d" % (group, n), "args": list(args)}
    if stdin:
        case["stdin"] = stdin
    if show:
        case["show"] = list(show)
    if env:
        case["env"] = env
    if setup:
        case["setup"] = list(setup)
    CASES.append(case)


# Substitution escapes and delimiters that interact with brackets, plus the
# append queue across D restarts.
for script in ["s/.*/\\u\\L&/", "s/.*/\\l\\U&/", "s/\\(.\\)\\(.*\\)/\\u\\1\\U\\2/", "s/\\x5e/X/", "s/a\\x2e/X/", "s/\\x2a/X/", "s/[\\/]/X/", "s/[/]/X/", "s/[^/]/X/", "s/[]/]/X/", "s/[a\\]/X/", "s/[[:alpha:]/]/X/", "s/[[:alpha:]]/X/", "s/\\./X/", "s/\\[x\\]/X/", "s/a\\{1,\\}/X/g", "s/\\(foo\\|bar\\)/[\\1]/"]:
    add("esc", [script, "re.txt"])
add("esc", ["N;s/[\\n]/X/", "a.txt"])
add("esc", ["N;s/[^\\n]*\\n//", "a.txt"])
for script in ["$!N;a foo\nP;D", "N;a X\nP;D", "N;i\\\nI\nP;D", "N;r rfile.txt\nP;D", "N;=;P;D", "$!N;P;D;a x", "2{a foo\nD}", "1{N;a foo\nD}"]:
    add("queue", [script, "a.txt"])
add("exit", ["q5", "missing.txt", "one.txt"])
add("exit", ["-n", "$p", "missing.txt", "one.txt"])
add("exit", ["2q", "a.txt", "missing.txt"])
add("exit", ["-s", "2q7", "a.txt", "missing.txt"])
add("ip", ["-ibk_*", "s/i/I/", "d/f.txt"], show=["d/f.txt", "bk_d/f.txt", "d/bk_f.txt", "bk_f.txt"])

# Option parsing and operand handling.
for args in [
    ["-n", "2p", "a.txt"],
    ["--quiet", "2p", "a.txt"],
    ["--silent", "2p", "a.txt"],
    ["--quie", "2p", "a.txt"],
    ["-n", "-e", "2p", "-e", "4p", "a.txt"],
    ["-ne", "2p", "a.txt"],
    ["-n", "--expression=3p", "a.txt"],
    ["-n", "--expression", "3p", "a.txt"],
    ["-n", "--expr=3p", "a.txt"],
    ["-n", "-f", "s_p.sed", "a.txt"],
    ["-n", "--file=s_p.sed", "a.txt"],
    ["-n", "--file", "s_p.sed", "a.txt"],
    ["-nf", "s_p.sed", "a.txt"],
    ["-f", "s_nonl.sed", "a.txt"],
    ["-f", "s_n.sed", "a.txt"],
    ["-f", "s_n2.sed", "a.txt"],
    ["-f", "s_nx.sed", "a.txt"],
    ["-e", "#n", "-e", "p", "a.txt"],
    ["#n\np", "a.txt"],
    ["#n", "a.txt"],
    ["-f", "s_text.sed", "a.txt"],
    ["-f", "s_cmt.sed", "a.txt"],
    ["-f", "s_multi.sed", "a.txt"],
    ["-f", "s_blk.sed", "a.txt"],
    ["-f", "s_crlf.sed", "a.txt"],
    ["-f", "s_label.sed", "dup.txt"],
    ["-f", "s_y.sed", "a.txt"],
    ["-f", "s_sub.sed", "a.txt"],
    ["-f", "s_p.sed", "-e", "s/a/X/", "a.txt"],
    ["-e", "s/a/X/", "-f", "s_p.sed", "a.txt"],
    ["-E", "s/(a|e)+/<&>/g", "a.txt"],
    ["-r", "s/(a|e)+/<&>/g", "a.txt"],
    ["--regexp-extended", "s/(a|e)+/<&>/g", "a.txt"],
    ["-n", "-E", "/^(al|be)/p", "a.txt"],
    ["s/a\\+/<&>/g", "a.txt"],
    ["--posix", "s/a\\+/<&>/g", "a.txt"],
    ["-E", "s/a{2}/X/", "dup.txt"],
    ["s/a\\{2\\}/X/", "dup.txt"],
    ["-s", "-n", "$p", "a.txt", "n.txt"],
    ["--separate", "-n", "$p", "a.txt", "n.txt"],
    ["-n", "$p", "a.txt", "n.txt"],
    ["-s", "-n", "1p", "a.txt", "n.txt"],
    ["-s", "-n", "F;=", "a.txt", "n.txt"],
    ["-n", "F;1=", "a.txt", "n.txt"],
    ["-s", "2,3d", "a.txt", "n.txt"],
    ["-s", "/gamma/,/3/d", "a.txt", "n.txt"],
    ["/gamma/,/3/d", "a.txt", "n.txt"],
    ["-z", "s/\\n/,/g", "a.txt"],
    ["--null-data", "s/\\n/,/g", "a.txt"],
    ["-z", "s/^/>/", "bin.bin"],
    ["-z", "-n", "l", "bin.bin"],
    ["-z", "=", "bin.bin"],
    ["-z", "$!N;P;D", "a.txt"],
    ["-z", "p", "nonl.txt"],
    ["-z", "G", "a.txt"],
    ["-z", "-s", "-n", "$=", "a.txt", "n.txt"],
    ["-u", "2q", "a.txt"],
    ["--unbuffered", "2q", "a.txt"],
    ["-n", "l", "long.txt"],
    ["-n", "-l", "20", "l", "long.txt"],
    ["-n", "--line-length=20", "l", "long.txt"],
    ["-n", "-l", "1", "l", "long.txt"],
    ["-n", "-l", "0", "l", "long.txt"],
    ["-n", "-l", "2", "l", "a.txt"],
    ["-n", "l 5", "a.txt"],
    ["-n", "l 0", "long.txt"],
    ["-n", "l 1", "a.txt"],
    ["-n", "l;l 3", "ctl.txt"],
    ["-n", "-l", "x", "l", "a.txt"],
    ["-b", "p", "a.txt"],
    ["--binary", "p", "a.txt"],
    ["--follow-symlinks", "p", "a.txt"],
    ["--sandbox", "p", "a.txt"],
    ["--sandbox", "w out.txt", "a.txt"],
    ["--sandbox", "r rfile.txt", "a.txt"],
    ["--sandbox", "s/a/b/w out.txt", "a.txt"],
    ["--sandbox", "e echo hi", "a.txt"],
    ["--sandbox", "s/a/b/e", "a.txt"],
    ["p", "-n", "a.txt"],
    ["--", "p", "a.txt"],
    ["-n", "--", "p", "a.txt"],
    ["-n", "-s", "-e", "p", "--", "a.txt"],
    ["-n", "--", "-p", "a.txt"],
    ["-x", "p", "a.txt"],
    ["--bogus", "p", "a.txt"],
    ["-e"],
    ["-f"],
    ["--expression"],
    ["-l"],
    ["--line-length"],
    ["-n", "--s", "p", "a.txt"],
    ["--in", "p", "a.txt"],
    [],
    ["-n"],
    ["-e", "p"],
    ["-s"],
    ["-E"],
    ["--debug", "-n", "p", "a.txt"],
    ["p", "a.txt", "-s", "n.txt"],
    ["-nsE", "$p", "a.txt", "n.txt"],
]:
    add("opt", args, stdin="a.txt" if "a.txt" not in args or args == [] else None)

for args in [
    ["--version"],
    ["--help"],
    ["-n", "--version"],
    ["--ver"],
    ["--he"],
]:
    add("opt-info", args)

# Operand handling: stdin, "-", missing and unusual files.
for args, stdin in [
    (["p"], "a.txt"),
    (["p", "-"], "a.txt"),
    (["-n", "$="], "n.txt"),
    (["p", "-", "one.txt"], "a.txt"),
    (["p", "one.txt", "-"], "a.txt"),
    (["-s", "-n", "F", "-", "one.txt"], "a.txt"),
    (["-n", "F", "-"], "a.txt"),
    (["-n", "F", "a.txt"], None),
    (["p", "-", "-"], "a.txt"),
    (["p"], "empty.txt"),
    (["$!d"], "nonl.txt"),
    (["p", "missing.txt"], None),
    (["p", "missing.txt", "one.txt"], None),
    (["p", "one.txt", "missing.txt"], None),
    (["-n", "$p", "one.txt", "missing.txt"], None),
    (["-n", "$p", "missing.txt", "one.txt", "empty.txt"], None),
    (["-n", "$p", "a.txt", "empty.txt"], None),
    (["-n", "$p", "empty.txt", "a.txt", "empty.txt"], None),
    (["-n", "$p", "a.txt", "empty.txt", "missing.txt"], None),
    (["p", "d"], None),
    (["p", "d", "one.txt"], None),
    (["p", "noaccess.txt"], None),
    (["p", "noaccess.txt", "one.txt"], None),
    (["p", "link.txt"], None),
    (["p", "dangling"], None),
    (["p", "dirlink"], None),
    (["p", ""], None),
    (["-n", "p", "one.txt", "one.txt"], None),
    (["p", "nonl.txt", "nonl.txt"], None),
    (["-s", "p", "nonl.txt", "nonl.txt"], None),
    (["-n", "$p", "nonl.txt", "one.txt"], None),
    (["$a end", "nonl.txt"], None),
    (["$a end", "nonl.txt", "onenl.txt"], None),
    (["p", "crlf.txt"], None),
    (["-n", "l", "crlf.txt"], None),
    (["s/$/X/", "crlf.txt"], None),
    (["s/.$//", "crlf.txt"], None),
    (["p", "bin.bin"], None),
    (["-n", "l", "bin.bin"], None),
    (["s/a/X/", "bin.bin"], None),
    (["s/\\xff/FF/", "bin.bin"], None),
    (["s/b/B/", "latin1.txt", "utf8.txt"], None),
    (["-n", "l", "latin1.txt"], None),
    (["-n", "l", "utf8.txt"], None),
    (["s/./X/g", "latin1.txt"], None),
    (["s/./X/g", "utf8.txt"], None),
    (["y/é/e/", "utf8.txt"], None),
    (["-n", "$=", "a.txt", "n.txt", "nonl.txt"], None),
    (["-n", "$=", "blank.txt"], None),
    (["s/^$/E/", "blank.txt"], None),
    (["N;N;s/\\n/+/g", "blank.txt"], None),
    (["-n", "l", "long.txt"], None),
    (["s/j/J/3", "long.txt"], None),
]:
    add("io", args, stdin=stdin)

# Each command against input shapes.
INPUTS = [["a.txt"], ["nonl.txt"], ["empty.txt"], ["one.txt"], ["a.txt", "nonl.txt"], ["nonl.txt", "a.txt"], ["bin.bin"], ["crlf.txt"]]
SCRIPTS = [
    "p", "-n;p", "d", "2d", "$d", "2q", "2Q", "q", "Q", "q7", "2Q9", "$!N", "N", "$!N;P;D", "N;P;D", "N;N;s/\\n/+/g",
    "n", "n;d", "$!n;s/./X/", "2n;p", "G", "H;$!d;x", "x", "x;G", "h;G", "g", "1h;2g", "1!G;h;$!d", "H;x", "2{h;d};${G}",
    "=", "-n;=", "-n;l", "l", "l 3", "z", "z;G", "F", "2F", "s/./X/", "s/$/!/", "s/^/>/", "y/abt/ABT/", "1i\\\nins", "$a\\\napp",
    "2c\\\nchg", "$c\\\nchg", "1,2c\\\nchg", "$!{$!N};s/\\n/-/", "2,3p", "2,3!p", "/a/,/e/{p;p}", "$p", "1~2p", "w out.txt",
    "r rfile.txt", "R rfile.txt", "2r rnonl.txt", "$r rnonl.txt", "W out.txt", "N;W out.txt", "a x", "i x", "c x", "2!d",
    "1!G;h;$!d", ":a;N;$!ba;s/\\n/,/g", "$!{h;d};H;x", "/a/{N;N;d}", "2{N;D}", "s/a/&&/;P;D",
]
for script in SCRIPTS:
    quiet = script.startswith("-n;")
    body = script[3:] if quiet else script
    for inputs in INPUTS:
        args = (["-n"] if quiet else []) + [body] + inputs
        show = ["out.txt"] if "out.txt" in body else []
        add("cmd", args, show=show)

# Commands with special argument shapes.
for args, show in [
    (["1r rfile.txt", "a.txt"], []),
    (["r missing.txt", "a.txt"], []),
    (["2R rfile.txt", "a.txt"], []),
    (["R rfile.txt", "a.txt"], []),
    (["R missing.txt", "a.txt"], []),
    (["r rfile.txt\np", "a.txt"], []),
    (["-e", "r rfile.txt", "-e", "p", "a.txt"], []),
    (["1R rfile.txt\n1R rfile.txt\n1R rfile.txt", "a.txt"], []),
    (["R rfile.txt\nR rfile.txt", "a.txt"], []),
    (["2{r rfile.txt\nd}", "a.txt"], []),
    (["2{a foo\nd}", "a.txt"], []),
    (["2{a foo\nn}", "a.txt"], []),
    (["2{a foo\nN}", "a.txt"], []),
    (["2{a foo\nq}", "a.txt"], []),
    (["2{r rfile.txt\nq}", "a.txt"], []),
    (["2{a foo\nQ}", "a.txt"], []),
    (["$!{a foo\nn}", "a.txt"], []),
    (["-n", "$!{a foo\nn}", "a.txt"], []),
    (["a foo\ni bar", "a.txt"], []),
    (["1a\\\nfoo\\\nbar", "a.txt"], []),
    (["1a\\foo", "a.txt"], []),
    (["1a\\  foo", "a.txt"], []),
    (["1a   foo", "a.txt"], []),
    (["1a\\\n  foo", "a.txt"], []),
    (["1a foo\\\nbar", "a.txt"], []),
    (["1a foo\\nbar", "a.txt"], []),
    (["1a foo\\tbar", "a.txt"], []),
    (["1a foo \\\\ bar", "a.txt"], []),
    (["1a\\", "a.txt"], []),
    (["1a", "a.txt"], []),
    (["1i\\", "a.txt"], []),
    (["$!{1a foo\n}", "a.txt"], []),
    (["1{a foo\n}", "a.txt"], []),
    (["1{a foo}", "a.txt"], []),
    (["1{i foo;p}", "a.txt"], []),
    (["1a foo;p", "a.txt"], []),
    (["1a\\\nfoo;p", "a.txt"], []),
    (["1c\\\nfoo\\\nbar", "a.txt"], []),
    (["2,4c foo", "a.txt"], []),
    (["2,4c foo", "a.txt", "n.txt"], []),
    (["-n", "2,4c foo", "a.txt"], []),
    (["2,4!c foo", "a.txt"], []),
    (["/beta/,/delta/c foo", "a.txt"], []),
    (["/beta/,/nomatch/c foo", "a.txt"], []),
    (["$!N;2,3c foo", "a.txt"], []),
    (["2{c foo\n}", "a.txt"], []),
    (["2,3{c foo\n}", "a.txt"], []),
    (["2c foo\n3d", "a.txt"], []),
    (["c foo", "a.txt"], []),
    (["i\\\n", "a.txt"], []),
    (["$i foo", "empty.txt"], []),
    (["1i foo", "empty.txt"], []),
    (["a foo", "empty.txt"], []),
    (["$!N;$!D", "a.txt"], []),
    (["N;N;D", "a.txt"], []),
    (["$!N;P;D", "n.txt"], []),
    (["D", "a.txt"], []),
    (["N;D", "a.txt"], []),
    (["N;N;N;N;N;N", "a.txt"], []),
    (["N;N;N;N;N;N", "a.txt"], []),
    (["--posix", "N;N;N;N;N;N", "a.txt"], []),
    (["-n", "N;N;N;N;N;N;p", "a.txt"], []),
    (["$!N;s/\\n/ /", "a.txt"], []),
    (["N;N;N;l;d", "a.txt"], []),
    (["x;p;x", "a.txt"], []),
    (["G;G", "a.txt"], []),
    (["1h;2,3H;$G", "a.txt"], []),
    (["$!d;g", "a.txt"], []),
    (["q;p", "a.txt", "n.txt"], []),
    (["2q;p", "nonl.txt"], []),
    (["3q", "nonl.txt"], []),
    (["q256", "a.txt"], []),
    (["q 3", "a.txt"], []),
    (["Q 3", "a.txt"], []),
    (["q3;p", "a.txt"], []),
    (["2{q4}", "a.txt"], []),
    (["2,3q", "a.txt"], []),
    (["=", "nonl.txt"], []),
    (["-n", "=;p", "nonl.txt"], []),
    (["1!G;h;$!d", "nonl.txt"], []),
    (["G", "nonl.txt"], []),
    (["$!N;P;D", "nonl.txt"], []),
    (["s/x/X/;$!d", "nonl.txt"], []),
    (["w /dev/stdout", "a.txt"], []),
    (["-n", "w /dev/stdout", "a.txt"], []),
    (["w /dev/stderr", "one.txt"], []),
    (["-n", "s/a/X/w /dev/stdout", "a.txt"], []),
    (["s/a/X/w /dev/stderr", "one.txt"], []),
    (["-n", "/a/w out.txt", "a.txt"], ["out.txt"]),
    (["-n", "w out.txt\nw out.txt", "one.txt"], ["out.txt"]),
    (["-n", "/e/w out.txt", "nonl.txt"], ["out.txt"]),
    (["-n", "$w out.txt", "nonl.txt"], ["out.txt"]),
    (["-n", "$w out.txt", "nonl.txt", "a.txt"], ["out.txt"]),
    (["w out.txt;p", "one.txt"], ["out.txt"]),
    (["-n", "w out.txt\np", "one.txt"], ["out.txt"]),
    (["-n", "w nodir/out.txt", "one.txt"], ["nodir/out.txt"]),
    (["-n", "w rodir/out.txt", "one.txt"], []),
    (["-n", "w d", "one.txt"], []),
    (["-n", "w a.txt", "one.txt"], ["a.txt"]),
    (["-n", "w ", "one.txt"], []),
    (["-n", "W out.txt", "a.txt"], ["out.txt"]),
    (["-n", "N;N;W out.txt", "a.txt"], ["out.txt"]),
    (["-n", "N;W /dev/stdout", "a.txt"], []),
    (["r /dev/stdin", "one.txt"], []),
    (["R /dev/stdin", "a.txt"], []),
    (["-n", "$=", "a.txt"], []),
    (["-n", "2{=;p}", "a.txt"], []),
    (["-n", "/a/=", "a.txt"], []),
    (["y/abc/\\n\\t\\\\/", "a.txt"], []),
    (["y/a\\nb/xyz/", "a.txt"], []),
    (["N;y/\\n/,/", "a.txt"], []),
    (["y,a\\,b,xyz,", "a.txt"], []),
    (["y/a/\\//", "a.txt"], []),
    (["y/\\//a/", "path.txt"], []),
    (["y|/|\\||", "path.txt"], []),
    (["y/abc/xyz/g", "a.txt"], []),
    (["y/abc/xy/", "a.txt"], []),
    (["y/ab/x/", "a.txt"], []),
    (["y/abc/xyz", "a.txt"], []),
    (["y/a-c/x-z/", "a.txt"], []),
    (["y/[a]/(b)/", "re.txt"], []),
    (["y/aa/xy/", "a.txt"], []),
    (["y/\\n/X/", "a.txt"], []),
    (["y/a/\\x/", "a.txt"], []),
    (["y/\\a/x/", "a.txt"], []),
    ([":a;s/^\\(.\\{1,5\\}\\)$/ \\1/;ta", "a.txt"], []),
    ([":a;N;$!ba;s/\\n/ /g", "a.txt"], []),
    (["-n", ":a;N;$!ba;s/\\n/ /gp", "a.txt"], []),
    ([":a;$!{N;ba};s/\\n/,/g", "a.txt"], []),
    (["b end;s/a/X/;:end", "a.txt"], []),
    (["b;s/a/X/", "a.txt"], []),
    ([":a\nb a", "empty.txt"], []),
    (["/a/b skip;s/./X/;:skip", "a.txt"], []),
    (["s/a/X/;t done;s/./Y/;:done", "a.txt"], []),
    (["s/a/X/;T done;s/./Y/;:done", "a.txt"], []),
    (["s/a/X/;t;s/./Y/", "a.txt"], []),
    (["s/a/X/;T;s/./Y/", "a.txt"], []),
    (["s/./X/;n;t done;s/$/!/;:done", "a.txt"], []),
    (["s/p/P/;s/a/A/;t;s/$/ none/", "a.txt"], []),
    (["t;s/a/A/", "a.txt"], []),
    (["s/a/A/;ta;s/$/ no/;:a", "a.txt"], []),
    (["s/a/A/\nta\ns/$/ no/\n:a", "a.txt"], []),
    ([": a;s/a/A/;t a", "a.txt"], []),
    (["b a ; s/x/y/ ; : a", "a.txt"], []),
    ([":a;s/aa/a/;ta;", "dup.txt"], []),
    (["b a;p;:a x", "a.txt"], []),
    (["b a;p;:a;p", "a.txt"], []),
    ([":a b", "a.txt"], []),
    ([":a;:a", "a.txt"], []),
    ([":a;:b;b a", "empty.txt"], []),
    (["{p;p}", "one.txt"], []),
    (["{p;p", "one.txt"], []),
    (["p;p}", "one.txt"], []),
    (["{}", "one.txt"], []),
    (["{;}", "one.txt"], []),
    (["{p};p", "one.txt"], []),
    (["{{p}}", "one.txt"], []),
    (["1{p;{p}}", "a.txt"], []),
    (["1!{p}", "one.txt"], []),
    (["1{p}p", "one.txt"], []),
    (["1{p} p", "one.txt"], []),
    (["1{p};;p", "one.txt"], []),
    (["1,3{/b/!d}", "a.txt"], []),
    (["/a/{/e/p}", "a.txt"], []),
    (["2{p;q}", "a.txt"], []),
    (["2{\np\n}", "a.txt"], []),
    (["2{ p ; p }", "a.txt"], []),
    ([" 2 p", "a.txt"], []),
    (["2 ! p", "a.txt"], []),
    (["2!!p", "a.txt"], []),
    (["2! !p", "a.txt"], []),
    (["1 , 3 p", "a.txt"], []),
    (["1,\n3p", "a.txt"], []),
    ([";;p;;", "one.txt"], []),
    (["p;", "one.txt"], []),
    (["p ; p", "one.txt"], []),
    (["p # comment", "one.txt"], []),
    (["p # comment\np", "one.txt"], []),
    ([" # comment\np", "one.txt"], []),
    (["#\np", "one.txt"], []),
    (["1#p", "one.txt"], []),
    (["2,3#p", "a.txt"], []),
    (["s/a/b/ # c", "a.txt"], []),
    (["y/a/b/ # c", "a.txt"], []),
    (["p p", "one.txt"], []),
    (["p;p x", "one.txt"], []),
    (["dp", "one.txt"], []),
    (["d}", "one.txt"], []),
    (["=p", "one.txt"], []),
    (["z p", "one.txt"], []),
    (["l p", "one.txt"], []),
    (["l3 ", "a.txt"], []),
    (["-n", "l3;l 3 ;l\n", "a.txt"], []),
    (["F x", "one.txt"], []),
    (["v", "a.txt"], []),
    (["v 4.2", "a.txt"], []),
    (["v 9.0", "a.txt"], []),
    (["v;p", "one.txt"], []),
    (["v 4.2\np", "one.txt"], []),
    (["k", "one.txt"], []),
    (["2k", "one.txt"], []),
    (["!", "one.txt"], []),
    (["1,2", "one.txt"], []),
    (["1,", "one.txt"], []),
    (["1", "one.txt"], []),
    (["/a/", "one.txt"], []),
    ([":", "one.txt"], []),
    ([": ", "one.txt"], []),
    (["1:a", "one.txt"], []),
    (["1,2:a", "one.txt"], []),
    (["1}", "one.txt"], []),
    (["1{p;1}", "one.txt"], []),
    (["{p;!}", "one.txt"], []),
    (["b nolabel", "one.txt"], []),
    (["t nolabel", "one.txt"], []),
    (["T nolabel", "one.txt"], []),
    (["b nolabel;:other", "one.txt"], []),
    (["1,2=", "a.txt"], []),
    (["1,2a x", "a.txt"], []),
    (["1,2i x", "a.txt"], []),
    (["1,2r rfile.txt", "a.txt"], []),
    (["1,2q", "a.txt"], []),
    (["1,2Q", "a.txt"], []),
    (["1,2:a", "a.txt"], []),
    (["1,2}", "a.txt"], []),
    (["1,2#c", "a.txt"], []),
    (["1,2l", "a.txt"], []),
    (["1,2F", "a.txt"], []),
    (["1,2z", "a.txt"], []),
    (["1,2v", "a.txt"], []),
    (["r", "a.txt"], []),
    (["R", "a.txt"], []),
    (["w", "a.txt"], []),
    (["W", "a.txt"], []),
    (["r  ", "a.txt"], []),
    (["rrfile.txt", "a.txt"], []),
    (["2rrfile.txt", "a.txt"], []),
    (["2r\trfile.txt", "a.txt"], []),
    (["2r  rfile.txt ", "a.txt"], []),
]:
    add("syn", args, show=show)

# Addresses.
ADDR_FILES = ["n.txt"]
for script in [
    "3p", "$p", "0~3p", "1~3p", "2~3p", "3~0p", "0~0p", "10~5p", "11~3p", "5~2p", "2~1p", "1~1p",
    "2,4p", "4,2p", "2,2p", "2,+2p", "2,+0p", "4,~4p", "3,~4p", "4,~0p", "5,~0p", "8,~4p", "2,~2p", "1,~3p", "9,~3p", "0,~3p",
    "/3/p", "/3/,/5/p", "/3/,5p", "5,/3/p", "/3/,+1p", "/1/,+2p", "/1/,~4p", "/^1/,/^3/p", "/3/,$p", "$,3p", "$,/3/p", "$,+1p",
    "/[0-9]/,/[0-9]/p", "/1/,/1/p", "/5/,/1/p", "2,/2/p", "0,/1/p", "1,/1/p", "0,/3/p", "0,/x/p", "0,/^/p", "0,/1/,p",
    "/1/!p", "2!p", "2,4!p", "$!p", "1!p", "1,$!p", "/1/,/3/!p", "0,/2/!p", "1~2!p", "/3/I,/5/p", "/3/Ip", "/X/Ip", "/x/I!p",
    "\\%3%p", "\\,3,p", "\\|3|,\\#5#p", "\\n3np", "\\x3xp", "/3/,\\%5%p", "\\;3;p",
    "/1/p;/1/p", "/1/{p;p}", "2{/3/p}", "1,3{2,4p}", "2,4{3p}", "2,4{/3/!p}", "2,4{/3/,/4/p}",
    "/2/,/4/{/3/d}", "/2/,/4/d;/5/,/6/d", "2,/4/d;/5/,/6/d", "/2/,4d;/3/,5d",
    "/1/,/2/{p;n}", "/2/,/4/{n;p}", "2,4{N;p}", "/2/,/3/{N;N}", "1,3{n;n;n}", "2{N;N;N}", "/5/,/7/{N;s/\\n/+/}",
    "$!N;/2/,/3/p", "2,3{h;d};${G}", "2,3H;${x;p}",
    "3,1p", "3,3p", "3,+0p", "/3/,2p", "/3/,3p", "4,/4/p", "4,/3/p",
    "10p", "11p", "1,10p", "1,11p", "10,11p", "0~10p", "1~0p", "$~2p", "1,3p;3,5p", "2,3d;3,4p", "1d;1,3p", "2d;2,1p", "4d;2,5p",
    "2,5{3d}", "2,5{2,3d}", "1,3{p;d}", "3,5!{p}", "/2/,/4/{/3/!p}", "/10/,$p", "/10/,/nomatch/p", "/0/p", "/1/Mp", "/^1$/Mp",
    "//p", "/1/{//p}", "/1/p;//p", "/1/,//p", "/1/s//X/", "/1/s//X/g", "s/1/X/;s//Y/", "/2/{s//[&]/;//d}",
    "1,2p;2,3p", "2,3{=}", "$!{=}", "/9/,/9/{=}", "2q", "2,3q", "2!q", "/3/{p;q}", "/5/Q", "$Q", "1Q", "0~2d",
]:
    add("addr", ["-n", script, "n.txt"] if script[0] not in "2" or True else [script, "n.txt"])
    add("addr", [script, "n.txt"])

# Addresses over several files, with -s and -i.
for script in ["$p", "1p", "$!d", "2,4p", "/3/,/b/p", "/gamma/,/3/p", "$=", "F", "1F", "/a/,/e/p", "0,/a/p", "0,/1/p", "2~2p", "$!N;P;D"]:
    add("addr-files", ["-n", script, "a.txt", "n.txt"])
    add("addr-files", ["-n", "-s", script, "a.txt", "n.txt"])
    add("addr-files", ["-n", script, "nonl.txt", "a.txt"])
    add("addr-files", ["-s", script, "nonl.txt", "a.txt"])
    add("addr-files", ["-n", "-s", script, "a.txt", "empty.txt", "n.txt"])

# Address syntax errors.
for script in [
    "0p", "0,5p", "0,/x/p", "0~3p", "+1p", "~2p", "1,+p", "1,~p", "1,p", ",3p", "/a", "/a/", "\\", "\\a", "\\ab", "/a/Z", "/a/Ip;p",
    "/a/I,/b/Ip", "/a/MI p", "/a/Mp", "/\\n/p", "1,2,3p", "1;2p", "$$p", "1~p", "1~2~3p", "a", "\\%a\\%b%p", "/a\\/b/p", "/a[/]b/p",
    "/[/p", "/a\\(b/p", "/a\\)/p", "/\\(a/p", "/*/p", "/\\(*\\)/p", "/a\\{1/p", "/a\\{2,1\\}/p", "/[b-a]/p", "/[[:foo:]]/p",
    "/\\1/p", "/\\(a\\)\\2/p", "//p",
]:
    add("addr-err", ["-n", script, "a.txt"])

# Substitution.
SUBS = [
    "s/a/X/", "s/a/X/g", "s/a/X/2", "s/a/X/2g", "s/A/X/", "s/A/X/i", "s/A/X/I", "s/a/X/gI", "s/a/X/Ig", "s/a/X/p", "s/a/X/gp", "s/a/X/pg",
    "s/a/X/3", "s/a/X/9", "s/a/X/10", "s/a/X/100", "s/a/X/1", "s/a/X/w out.txt", "s/a/X/gw out.txt", "s/a/X/pw out.txt", "s/a/X/w /dev/stdout",
    "s/./X/", "s/./X/g", "s/.*/X/", "s/.*/X/g", "s/x*/-/g", "s/x*/-/2", "s/a*/-/g", "s/a*/-/2", "s/a*/-/3g", "s/b*/-/g", "s/\\(a\\)*/[\\1]/g",
    "s/^/>/", "s/$/</", "s/^/>/g", "s/$/</g", "s/^a/X/g", "s/a$/X/g", "s/a\\|e/X/g", "s/\\<./X/g", "s/.\\>/X/g", "s/\\b/|/g", "s/\\B/|/g",
    "s/\\w\\+/[&]/g", "s/\\W/_/g", "s/\\s/_/g", "s/\\S\\+/<&>/", "s/[[:alpha:]]\\+/W/", "s/[^[:alpha:]]/_/g", "s/[[:upper:]]/u/g", "s/[a-c]/X/g",
    "s/[]a]/X/g", "s/[^]a]/X/g", "s/[a-]/X/g", "s/[\\n]/X/g", "s/[\\.]/X/g", "s/[.]/X/g", "s/[*]/X/g", "s/\\./X/g", "s/\\*/X/g", "s/a\\{2\\}/X/",
    "s/a\\{1,\\}/X/g", "s/a\\{,2\\}/X/g", "s/a\\{0\\}/X/g", "s/\\(a\\)\\(.\\)/\\2\\1/", "s/\\(a\\)\\(.\\)/\\2\\1\\0/", "s/\\(.\\)\\1/<\\1>/g",
    "s/\\(a\\)\\|b/[\\1]/g", "s/\\(\\(a\\)\\(b\\)\\)/\\3\\2\\1/", "s/a/\\n/", "s/a/\\t/", "s/a/\\\\/", "s/a/\\//", "s/a/&&/", "s/a/\\&/", "s/a/[&]/g",
    "s/a/\\x41/", "s/a/\\x26/", "s/a/\\d065/", "s/a/\\o101/", "s/a/\\cA/", "s/a/\\a\\f\\v\\r/", "s/a/\\q/", "s/a/\\1/", "s/\\(a\\)/\\2/",
    "s/a/\\U&/", "s/a/\\U&x/g", "s/.*/\\U&/", "s/.*/\\L&/", "s/\\(.\\)\\(.*\\)/\\U\\1\\E\\2/", "s/\\(.\\)\\(.*\\)/\\u\\1\\2/", "s/.*/\\u&/",
    "s/.*/\\l&/", "s/\\w\\+/\\u&/g", "s/\\w\\+/\\U\\l&/g", "s/.*/\\L\\u&/", "s/\\(a\\)\\(l\\)/\\U\\1\\E\\2/", "s/a/\\Ux\\Ey/", "s/a/\\ux\\ly/",
    "s/\\(a\\)\\(l\\)/\\u\\1\\u\\2/", "s/.*/\\U&\\E-\\L&/", "s/.\\{3\\}/\\U&\\n/", "s/a/x\\\ny/", "s/\\(a\\)/\\U\\n\\1/",
    "s,a,X,", "s|a|X|", "s#a#X#", "s a X ", "s:a:X:g", "s_a_X_", "s.a.X.", "s.a\\.b.X.", "s*a*X*", "s*a\\*b*X*", "s+a+X+", "s+a\\+b+X+", "s|a\\|b|X|",
    "s/a\\/b/X/", "s/a/\\//", "s,a\\,b,X,", "sxaxXx", "s1a1X1", "s!a!X!", "s&a&X&", "s&a&\\&&", "s/a/b/;s//c/", "s/b/B/;s//C/g",
    "s/\\n/X/", "N;s/\\n/X/", "N;s/^/X/g", "N;s/^/X/mg", "N;s/$/X/mg", "N;s/$/X/g", "N;s/^b/X/", "N;s/^b/X/m", "N;s/^b/X/M", "N;s/a$/X/M",
    "N;s/\\`a/X/Mg", "N;s/\\'/X/Mg", "N;s/a\\'/X/M", "N;s/.*/X/", "N;s/.*/X/M", "N;s/.*/X/Mg", "N;s/[^x]*/X/", "N;s/a.b/X/", "N;s/a\\nb/X/",
    "N;N;s/\\n/+/2", "N;N;s/\\n/+/g", "G;s/\\n/+/", "G;s/^$/E/", "x;s/^$/E/;x", "H;x;s/\\n/,/g;x",
    "s/a/X/;s/e/Y/", "s/a/X/;ta;s/$/ !/;:a", "s/a/X/ ; s/e/Y/", "s/a/X/\ns/e/Y/", "s/a/X/}", "{s/a/X/}", "{s/a/X/;}", "s/a/X/;;s/e/Y/",
    "s/a/X/ p", "s/a/X/g p", "s/a/X/gg", "s/a/X/pp", "s/a/X/2 3", "s/a/X/2p3", "s/a/X/0", "s/a/X/0g", "s/a/X/q", "s/a/X/w", "s/a/X/gw",
    "s/a/X/mM", "s/a/X/ii", "s/a/X/gIm", "s/a/X", "s/a", "s/", "s", "s//X/", "s/\\(/X/", "s/\\)/X/", "s/a\\{/X/", "s/a\\{1/X/",
    "s/[/X/", "s/[a/X/", "s/a/\\", "s/a/X\\", "s/a/X\\\n", "s/*/X/", "s/\\(*\\)/X/", "s/a**/X/", "s/^*/X/", "s/\\(^a\\)/X/", "s/a\\|*b/X/",
    "s/\\+/X/", "s/a\\?/X/", "s/\\?/X/", "s/a\\{1,2\\}\\{2\\}/X/", "s/\\(a\\)\\{2\\}/X/", "s/[[:alpha:]/X/", "s/[[.a.]]/X/", "s/[[=a=]]/X/",
    "s/a/\\0/", "s/a/\\00/", "s/a/\\1\\2/", "s/\\(a\\)\\(b\\)\\(c\\)\\(d\\)\\(e\\)\\(f\\)\\(g\\)\\(h\\)\\(i\\)/\\9/", "s/a/b/I;s//c/",
]
SUB_INPUTS = ["a.txt"]
for script in SUBS:
    show = ["out.txt"] if "out.txt" in script else []
    add("sub", [script, "a.txt"], show=show)
    add("sub", ["-n", script, "a.txt"], show=show)

# Substitution over inputs that stress matching.
for script in [
    "s/a*b/X/g", "s/a*/X/g", "s/\\(a*\\)*/X/g", "s/a\\+/X/g", "s/a\\?b/X/g", "s/\\(ab\\)\\+/X/g", "s/\\(a\\|b\\)*/X/", "s/^a*$/X/",
    "s/a.*b/X/", "s/a.*\\?b/X/", "s/\\(a\\)\\1/X/", "s/\\(a*\\)b\\1/X/", "s/./X/2", "s/./X/3g", "s/ \\+/ /g", "s/  */ /g", "s/^ *//", "s/ *$//",
    "s/[a-z]*/(&)/g", "s/[a-z]\\+/(&)/2", "s/o/0/g;s/e/3/g", "s/\\(.*\\) \\(.*\\)/\\2 \\1/", "s/\\([^ ]*\\) \\([^ ]*\\)/\\2 \\1/g",
    "s/foo\\|bar/X/g", "s/\\(foo\\|bar\\)\\+/X/g", "s/.*/\"&\"/", "s/'/\"/g", "s/\"/'/g", "s/\\//|/g", "s/\\\\/\\//g", "s/\t/<TAB>/g", "s/\\t/<TAB>/g",
]:
    for f in ["words.txt", "dup.txt", "ctl.txt", "re.txt", "csv.txt", "path.txt"]:
        add("sub2", [script, f])

# Extended regular expressions.
for script in [
    "s/(a|b)+/X/g", "s/a{2}/X/g", "s/a{2,}/X/g", "s/a{,2}/X/g", "s/a+/X/g", "s/a?b/X/g", "s/(ab)*c/X/g", "s/(a)(b)/\\2\\1/", "s/(a|b)\\1/X/",
    "s/a|b/X/g", "s/^(a|b)/X/", "s/(a|b)$/X/", "s/\\(a\\)/X/", "s/a\\{2\\}/X/", "s/\\(/X/", "s/(/X/", "s/)/X/", "s/(a/X/", "s/a)/X/", "s/()/X/", "s/a||b/X/",
    "s/a{/X/", "s/a{1/X/", "s/a{x}/X/", "s/{/X/", "s/+/X/", "s/?/X/", "s/*/X/", "s/|a/X/", "s/a|/X/", "s/^*/X/", "s/a**/X/", "s/a+*/X/", "s/(*a)/X/",
    "s/[[:digit:]]+/<&>/g", "s/\\./X/g", "s/\\+/X/g", "s/\\|/X/g", "s/\\{/X/g", "s/\\(/X/g", "s/\\)/X/g", "s/a\\b/X/g", "s/\\w+/W/g", "s/\\<a/X/g",
    "s/.*/\\U&/", "s/(.)(.*)/\\u\\1\\2/", "/^(a|b)/p", "/a{2}/p", "/(foo|bar)/!d", "/a+b/,/b/p", "s/(^a|b$)/X/g", "s/a$|^b/X/g", "s/(a|^b)/X/g",
    "s/(a)|(b)/[\\1\\2]/g", "s/((a)|(b))+/[\\1\\2\\3]/", "s/(a*)*/X/", "s/(a*)+/X/", "s/(a|ab)(c|bcd)(d*)/[\\1,\\2,\\3]/", "s/[a|b]/X/g", "s/a\\|b/X/g",
]:
    add("ere", ["-E", script, "re.txt"])
    add("ere", ["-E", "-n", script, "dup.txt"])

# UTF-8 and locale handling.
for env in [{"LC_ALL": "C.UTF-8"}, {"LC_ALL": "C"}]:
    for script in ["s/./X/g", "s/é/E/", "y/éï/ei/", "s/[é]/E/g", "s/\\w\\+/W/g", "l", "s/.\\{3\\}/&|/", "s/[[:alpha:]]/A/g", "s/ß/ss/", "s/./\\U&/g", "s/.*/\\U&/", "/ü/p", "s/^.//", "s/.$//"]:
        add("loc", ["-n" if script == "l" else "-e", script if script != "l" else "l", "utf8.txt"], env=env)
    add("loc", ["s/./X/g", "latin1.txt"], env=env)
    add("loc", ["s/\\xe9/E/", "latin1.txt"], env=env)
    add("loc", ["-n", "l", "latin1.txt"], env=env)
    add("loc", ["s/a/b/", "bin.bin"], env=env)

# In-place editing.
add("ip", ["-i", "s/a/X/", "a.txt"], show=["a.txt"])
add("ip", ["-i", "s/a/X/", "a.txt", "one.txt"], show=["a.txt", "one.txt"])
add("ip", ["-i.bak", "s/a/X/", "a.txt"], show=["a.txt", "a.txt.bak"])
add("ip", ["--in-place=.bak", "s/a/X/", "a.txt"], show=["a.txt", "a.txt.bak"])
add("ip", ["--in-place", "s/a/X/", "a.txt"], show=["a.txt", "a.txt.bak"])
add("ip", ["-i", ".bak", "s/a/X/", "a.txt"], show=["a.txt", "a.txt.bak", ".bak"])
add("ip", ["-i", "-e", "s/a/X/", "a.txt"], show=["a.txt"])
add("ip", ["-ie", "s/a/X/", "a.txt"], show=["a.txt", "a.txte"])
add("ip", ["-i", "-n", "2p", "a.txt"], show=["a.txt"])
add("ip", ["-ni", "2p", "a.txt"], show=["a.txt"])
add("ip", ["-ni.bak", "2p", "a.txt"], show=["a.txt", "a.txt.bak"])
add("ip", ["-in", "2p", "a.txt"], show=["a.txt", "a.txtn"])
add("ip", ["-Ei", "s/(a|e)+/X/", "a.txt"], show=["a.txt"])
add("ip", ["-i", "-E", "s/(a|e)+/X/", "a.txt"], show=["a.txt"])
add("ip", ["-i", "-s", "s/a/X/", "a.txt"], show=["a.txt"])
add("ip", ["-s", "-i", "s/a/X/", "a.txt"], show=["a.txt"])
add("ip", ["-i", "$d", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "1d", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "2q", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "2Q", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "-n", "$=", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "F", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "1i\\\nfirst", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "$a end", "a.txt", "nonl.txt"], show=["a.txt", "nonl.txt"])
add("ip", ["-i", "s/x/X/", "nonl.txt"], show=["nonl.txt"])
add("ip", ["-i", "p", "nonl.txt"], show=["nonl.txt"])
add("ip", ["-i", "$!N;P;D", "nonl.txt"], show=["nonl.txt"])
add("ip", ["-i", "p", "empty.txt"], show=["empty.txt"])
add("ip", ["-i.bak", "p", "empty.txt"], show=["empty.txt", "empty.txt.bak"])
add("ip", ["-i", "w out.txt", "a.txt"], show=["a.txt", "out.txt"])
add("ip", ["-i", "w /dev/stdout", "a.txt"], show=["a.txt"])
add("ip", ["-i", "r rfile.txt", "one.txt"], show=["one.txt"])
add("ip", ["-i", "1R rfile.txt", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "N;N;s/\\n/+/g", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "$!N;s/\\n/+/", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "/gamma/,/3/d", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "0,/a/d", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "2,3c foo", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "p", "missing.txt"], show=["missing.txt"])
add("ip", ["-i", "p", "missing.txt", "one.txt"], show=["missing.txt", "one.txt"])
add("ip", ["-i", "p", "one.txt", "missing.txt"], show=["missing.txt", "one.txt"])
add("ip", ["-i", "p", "d"], show=["d"])
add("ip", ["-i", "p", "d", "one.txt"], show=["d", "one.txt"])
add("ip", ["-i", "p"], show=[])
add("ip", ["-i", "p", "-"], show=[])
add("ip", ["-i", "p", "noaccess.txt"], show=["noaccess.txt"])
add("ip", ["-i", "s/r/R/", "ro.txt"], show=["ro.txt"])
add("ip", ["-i", "s/r/R/", "rodir/f.txt"], show=["rodir/f.txt"])
add("ip", ["-i", "s/a/X/", "link.txt"], show=["link.txt", "a.txt"])
add("ip", ["-i", "--follow-symlinks", "s/a/X/", "link.txt"], show=["link.txt", "a.txt"])
add("ip", ["-i.bak", "s/a/X/", "link.txt"], show=["link.txt", "a.txt", "link.txt.bak"])
add("ip", ["-i.bak", "--follow-symlinks", "s/a/X/", "link.txt"], show=["link.txt", "a.txt", "link.txt.bak", "a.txt.bak"])
add("ip", ["-i", "p", "dangling"], show=["dangling"])
add("ip", ["-i", "--follow-symlinks", "p", "dangling"], show=["dangling"])
add("ip", ["-i", "p", "dirlink"], show=["dirlink"])
add("ip", ["-i", "p", "link.txt", "a.txt"], show=["link.txt", "a.txt"])
add("ip", ["-i", "p", "one.txt", "one.txt"], show=["one.txt"])
add("ip", ["-ibak/*", "s/a/X/", "a.txt"], show=["a.txt", "bak/a.txt"], setup=["mkdir bak"])
add("ip", ["-ibak/*", "s/a/X/", "a.txt"], show=["a.txt", "bak/a.txt"])
add("ip", ["-ibk_*", "s/a/X/", "a.txt"], show=["a.txt", "bk_a.txt"])
add("ip", ["-i*.orig", "s/a/X/", "a.txt"], show=["a.txt", "a.txt.orig"])
add("ip", ["-i*_*", "s/a/X/", "a.txt"], show=["a.txt", "a.txt_a.txt"])
add("ip", ["-i", "s/a/X/", "d/f.txt"], show=["d/f.txt"])
add("ip", ["-i.bak", "s/i/I/", "d/f.txt"], show=["d/f.txt", "d/f.txt.bak"])
add("ip", ["-ibk/*", "s/i/I/", "d/f.txt"], show=["d/f.txt", "d/bk/f.txt", "bk/f.txt", "bk/d/f.txt"])
add("ip", ["-i", "s/a/X/", "a.txt"], show=["a.txt"], setup=["chmod 600 a.txt"])
add("ip", ["-i", "s/a/X/", "a.txt"], show=["a.txt"], setup=["chmod 755 a.txt"])
add("ip", ["-i", "--follow-symlinks", "s/a/X/", "link2"], show=["link2", "link.txt", "a.txt"], setup=["ln -s link.txt link2"])
add("ip", ["-i", "s/a/X/", "link2"], show=["link2", "link.txt", "a.txt"], setup=["ln -s link.txt link2"])
add("ip", ["-i.bak", "--follow-symlinks", "s/a/X/", "link2"], show=["link2", "link.txt", "a.txt", "a.txt.bak", "link.txt.bak", "link2.bak"], setup=["ln -s link.txt link2"])
add("ip", ["--follow-symlinks", "-n", "p", "link2"], setup=["ln -s link.txt link2"])
add("ip", ["-i", "--debug", "s/a/X/", "a.txt"], show=["a.txt"])
add("ip", ["-n", "-i", "-e", "p", "-e", "p", "one.txt"], show=["one.txt"])
add("ip", ["-s", "-n", "p", "a.txt", "one.txt"], show=["a.txt"])
add("ip", ["-i", "q5", "a.txt"], show=["a.txt"])
add("ip", ["-i", "-f", "s_p.sed", "one.txt"], show=["one.txt"])
add("ip", ["-i", "-z", "s/\\n/,/g", "a.txt"], show=["a.txt"])
add("ip", ["--in-place=", "p", "one.txt"], show=["one.txt"])
add("ip", ["--in-place=.b", "p", "one.txt"], show=["one.txt", "one.txt.b"])
add("ip", ["-i", "-u", "p", "one.txt"], show=["one.txt"])
add("ip", ["-i", "-l", "5", "l", "one.txt"], show=["one.txt"])
add("ip", ["-i", "=", "one.txt"], show=["one.txt"])
add("ip", ["-i", "l", "one.txt"], show=["one.txt"])
add("ip", ["-i", "i\\\nfoo", "empty.txt"], show=["empty.txt"])
add("ip", ["-i", "a foo", "empty.txt"], show=["empty.txt"])
add("ip", ["-i", "s/./X/", "onenl.txt", "a.txt"], show=["onenl.txt", "a.txt"])

# NUL-delimited records: the delimiter replaces the newline in every record boundary.
for script in ["p", "=", "l", "F", "$!N;P;D", "G", "H;x", "N;s/\\n/+/", "s/^/>/", "a txt", "i txt", "c txt", "r rfile.txt", "R rfile.txt", "w out.txt", "y/a\\n/AN/", "n;d", "$d", "1d", "2q", "x;G"]:
    show = ["out.txt"] if "out.txt" in script else []
    add("zero", ["-z", script, "bin.bin"], show=show)
    add("zero", ["-z", "-n", script, "nonl.txt", "bin.bin"], show=show)
add("zero", ["-z", "s/\\n/,/g", "a.txt", "n.txt"])
add("zero", ["-s", "-z", "-n", "$p", "bin.bin", "a.txt"])
add("zero", ["-z", "$!d", "a.txt", "bin.bin"])

# Separate files: line numbers, ranges, $, hold space, and R/w state per file.
for script in ["=", "F", "$p", "1~2p", "2,4p", "/a/,/e/p", "0,/a/p", "x", "G", "h;$G", "$!N;s/\\n/+/", "N;N", "n;n", "R rfile.txt", "1R rfile.txt", "w out.txt", "$r rfile.txt", "q", "2q", "$a end", "1i top", "$!d"]:
    show = ["out.txt"] if "out.txt" in script else []
    add("sep", ["-s", "-n", script, "a.txt", "n.txt", "nonl.txt"], show=show)
    add("sep", ["-s", script, "a.txt", "one.txt", "empty.txt", "nonl.txt"], show=show)
add("sep", ["-s", "p", "missing.txt", "a.txt"])
add("sep", ["-s", "-n", "$p", "a.txt", "missing.txt"])
add("sep", ["--separate", "-n", "$=", "n.txt", "a.txt", "-"], stdin="one.txt")

# Regular expression semantics that the host matcher must reproduce.
for script in [
    "s/a.b/X/", "s/a\\.b/X/", "s/^*a/X/", "s/\\(^\\|x\\)a/X/", "s/a\\{2,\\}/X/", "s/\\(a\\)\\1/X/", "s/.*/<&>/", "s/[[:space:]]\\+/_/g",
    "s/[a-z]*$/X/", "s/\\(foo\\|bar\\)*/X/", "s/x*$/X/", "s/^x*/X/", "s/a*b*c*/X/g", "s/[0-9]\\+/<&>/g", "s/\\([^ ]*\\) \\(.*\\)/\\2 \\1/",
    "s/\\w\\+/W/2", "s/\\s/_/g", "s/\\S\\+$/X/", "s/\\W/_/g", "s/[[:digit:]]/D/g", "s/[[:punct:]]/P/g", "s/\\(.\\)\\(.\\)\\(.\\)/\\3\\2\\1/",
    "s/.\\{2\\}/[&]/g", "s/\\(ab\\)\\{2\\}/X/", "s/a\\?b/X/", "s/b\\+/X/", "s/\\(a\\|b\\)\\+/X/g", "s/^\\(.*\\)\\n\\1$/dup/", "s/\\n//", "s/$/\\n/",
    "s/\\//|/g", "s/|/\\//g", "s/\\^/X/", "s/\\$/X/", "s/\\[/X/", "s/\\]/X/", "s/[\\]]/X/", "s/\\*/X/", "s/\\.\\*/X/",
]:
    for f in ["re.txt", "words.txt", "path.txt"]:
        add("rx", [script, f])
add("rx", ["N;s/a.b/X/", "a.txt"])
add("rx", ["N;s/a$/X/", "a.txt"])
add("rx", ["N;s/^b/X/", "a.txt"])
add("rx", ["N;N;s/\\n/-/2", "a.txt"])
add("rx", ["$!N;s/\\n.*//", "a.txt"])
add("rx", ["/a/s//X/", "a.txt"])
add("rx", ["/a/I s//X/", "a.txt"])
add("rx", ["s/A/X/I;s//Y/", "a.txt"])
add("rx", ["/b/s//[&]/;//d", "a.txt"])
add("rx", ["s/a/X/;s//Y/g", "a.txt"])
add("rx", ["//p", "a.txt"])
add("rx", ["s//X/I", "a.txt"])
add("rx", ["/a/Id", "a.txt"])
add("rx", ["/ALPHA/Id", "a.txt"])
add("rx", ["/A/,/E/Ip", "a.txt"])
add("rx", ["-n", "N;/^b/Mp", "a.txt"])
add("rx", ["-n", "N;/a$/Mp", "a.txt"])
add("rx", ["-n", "N;/a\\nb/p", "a.txt"])
add("rx", ["-n", "N;N;s/^/>/Mgp", "a.txt"])
add("rx", ["-n", "N;N;s/$/</Mgp", "a.txt"])
add("rx", ["-n", "N;N;s/^./X/Mgp", "a.txt"])
add("rx", ["-n", "N;N;s/.$/X/Mgp", "a.txt"])
add("rx", ["-n", "N;N;s/a$\\n/X/p", "a.txt"])
add("rx", ["-n", "N;N;s/\\`a/X/Mp", "a.txt"])
add("rx", ["-n", "N;N;s/a\\'/X/Mp", "a.txt"])
add("rx", ["-n", "N;N;s/[^\\n]*$/X/Mp", "a.txt"])
add("rx", ["-E", "-n", "N;N;s/(^|\\n)b/<&>/Mgp", "a.txt"])
add("rx", ["-n", "s/\\(a\\)\\(b\\)\\?/[\\1|\\2]/p", "re.txt"])
add("rx", ["-E", "-n", "s/(a)(b)?/[\\1|\\2]/p", "re.txt"])
add("rx", ["-n", "s/a*/X/gp", "re.txt"])
add("rx", ["-n", "s/\\(x\\)*/[\\1]/gp", "words.txt"])
add("rx", ["s/b*/X/3", "a.txt"])
add("rx", ["s/b*/X/2g", "a.txt"])
add("rx", ["s/\\(\\)/X/g", "a.txt"])
add("rx", ["-E", "s/()/X/g", "a.txt"])
add("rx", ["s/$/X/2", "a.txt"])
add("rx", ["s/^/X/2", "a.txt"])

# Commands in less common positions and combinations.
for args in [
    ["$!N;$!D", "n.txt"], ["N;N;N;$!D", "n.txt"], ["1!G;h;$!d", "n.txt"], ["-n", "1!G;h;$p", "n.txt"], ["G;h", "a.txt"],
    ["-n", "/b/{n;p}", "a.txt"], ["-n", "/b/{N;p}", "a.txt"], ["/b/{N;N;D}", "a.txt"], ["/b/!d", "a.txt"], ["/b/,/d/!d", "a.txt"],
    ["2,3{N;s/\\n/+/}", "n.txt"], ["2,3{$!N;s/\\n/+/}", "n.txt"], ["2{h;d};4{G}", "n.txt"], ["3{x;p;x}", "n.txt"], ["2{x;p;x;p}", "a.txt"],
    ["/a/{s//X/;t;s/$/ no/}", "a.txt"], ["/b/{s/b/B/;n;s/g/G/}", "a.txt"], ["2{p;d};p", "n.txt"], ["n;n;s/./X/", "n.txt"], ["N;P;P;D", "a.txt"],
    ["-n", "$!{N;P};D", "a.txt"], ["-n", "x;n;x;p", "a.txt"], ["=;=", "one.txt"], ["l;l 2", "ctl.txt"], ["a\\\none\\\ntwo\n$a\\\nthree", "one.txt"],
    ["i\\\n  indented", "one.txt"], ["a\\\n\tTabbed", "one.txt"], ["1c\\\nA\\\nB\n2c\\\nC", "a.txt"], ["/b/c\\\nBEE", "a.txt"], ["$!{N;N;c\\\ngrp\n}", "a.txt"],
    ["2r rfile.txt\n2a after", "a.txt"], ["2a after\n2r rfile.txt", "a.txt"], ["2i before\n2a after\n2c change", "a.txt"], ["r rnonl.txt", "a.txt"],
    ["R rnonl.txt", "a.txt"], ["r rfile.txt", "nonl.txt"], ["$r rnonl.txt", "nonl.txt"], ["$R rnonl.txt", "nonl.txt"], ["1r rfile.txt\n1R rfile.txt\n1r rnonl.txt", "one.txt"],
    ["w /dev/stdout\ns/o/0/w /dev/stdout", "one.txt"], ["-n", "p;w /dev/stdout", "one.txt"], ["p;w /dev/stdout", "nonl.txt"], ["w /dev/stdout", "nonl.txt"],
    ["-n", "s/./X/pw /dev/stdout", "nonl.txt"], ["$!N;l", "nonl.txt"], ["N;N;l;d", "nonl.txt"], ["x;l;x", "nonl.txt"], ["-n", "z;l", "nonl.txt"],
    ["y/xyz/XYZ/;$!N;y/\\n/_/", "nonl.txt"], ["s/z/&\\n/", "nonl.txt"], ["$!N;s/\\n/ /;P;D", "nonl.txt"], ["N;N;N;P", "nonl.txt"], ["$s/$/!/", "nonl.txt"],
    ["F;=", "nonl.txt", "one.txt"], ["-n", "$F", "a.txt", "one.txt"], ["a\\", "one.txt"], ["a\\\n", "one.txt"], ["a\\\n\n", "one.txt"], ["a   ", "one.txt"],
    ["a\\\ttext", "one.txt"], ["a\\\\ttext", "one.txt"], ["a x\\\\y", "one.txt"], ["a x\\ny", "one.txt"], ["a \\ x", "one.txt"], ["a\\  \n x", "one.txt"],
    ["s/o/\\\n/", "one.txt"], ["s/o/a\\\nb/", "one.txt"], ["s/o/\\\\\\n/", "one.txt"], ["s/n/\\t/", "one.txt"], ["s/n/\\x41\\x42/", "one.txt"],
    ["s/n/\\o101/", "one.txt"], ["s/n/\\d066/", "one.txt"], ["s/n/\\cA/", "one.txt"], ["s/n/\\c[/", "one.txt"], ["s/n/\\cz/", "one.txt"], ["s/n/\\x/", "one.txt"],
    ["s/n/\\xZ/", "one.txt"], ["s/n/\\x4/", "one.txt"], ["s/n/\\x414/", "one.txt"], ["s/n/\\u&/", "one.txt"], ["s/n/\\U&x/;s/$/Y/", "one.txt"],
    ["s/o\\(n\\)/\\U\\1\\Lx\\1/", "one.txt"], ["s/\\(o\\)\\(n\\)/\\u\\1\\l\\2/", "one.txt"], ["s/\\(o\\)\\(n\\)/\\U\\1\\E\\2/", "one.txt"], ["s/.*/\\L\\u&/", "words.txt"],
    ["s/\\(.\\)\\(.*\\)/\\U\\1\\L\\2/", "words.txt"], ["s/\\w\\+/\\u&/g", "words.txt"], ["s/\\(\\w\\)\\(\\w*\\)/\\U\\1\\E\\2/g", "words.txt"], ["s/\\b./\\u&/g", "words.txt"],
]:
    add("mix", args)

# Option syntax corners and empty or degenerate scripts.
for args in [
    ["-n", "-n", "p", "a.txt"], ["-E", "-E", "s/(a)/<\\1>/", "a.txt"], ["--expression=", "a.txt"], ["-e", "", "a.txt"], ["", "a.txt"], [";", "a.txt"],
    ["#c", "a.txt"], ["-n", "#c", "a.txt"], ["-l5", "-n", "l", "a.txt"], ["-l", "5x", "-n", "l", "a.txt"], ["--line-length=abc", "-n", "l", "a.txt"],
    ["-nl", "5", "l", "a.txt"], ["-ne", "p", "--", "-n", "a.txt"], ["--version=x"], ["--help=x"], ["--quiet=1", "p", "a.txt"], ["--quiet", "--quiet", "p", "a.txt"],
    ["--null-data", "-z", "p", "nonl.txt"], ["--zero-terminated", "p", "nonl.txt"], ["--zero", "p", "nonl.txt"], ["--line", "5", "-n", "l", "a.txt"],
    ["--in-place=.bak", "-n", "p", "a.txt"], ["--in-place", ".bak", "p", "a.txt"], ["--regexp", "s/(a)/<\\1>/", "a.txt"], ["--regexp-e", "s/(a)/<\\1>/", "a.txt"],
    ["--follow", "p", "a.txt"], ["--unbuffered", "p", "a.txt"], ["--unb", "p", "a.txt"], ["--sep", "-n", "$p", "a.txt", "n.txt"], ["--si", "p", "a.txt"],
    ["--expression", "p", "--expression", "p", "-n", "a.txt"], ["-f", "s_p.sed", "-n", "-f", "s_p.sed", "a.txt"], ["-f", "noaccess.txt", "a.txt"],
    ["-f", "s_p.sed", "-e", "p;p", "-n", "a.txt"], ["-n", "-e", "p", "-f", "s_p.sed", "-e", "p", "a.txt"], ["-e", "1{", "-e", "p", "-e", "}", "a.txt"],
    ["-e", "1!{", "-e", "d", "-e", "}", "a.txt"], ["-e", "s/a/", "-e", "b/", "a.txt"], ["-e", "y/a/", "-e", "b/", "a.txt"], ["-e", "a\\", "-e", "x", "-e", "a\\", "-e", "y", "one.txt"],
    ["-e", "i\\", "-e", "multi\\", "-e", "line", "one.txt"], ["-e", "c\\", "-e", "changed", "a.txt"], ["-e", "$!N", "-e", "P;D", "a.txt"],
    ["-s", "-f", "s_p.sed", "-n", "a.txt", "one.txt"], ["--debug"], ["-i", "--follow-symlinks", "p"], ["-E", "-r", "-E", "s/(a)/\\1\\1/", "a.txt"],
    ["--posix", "-n", "p", "a.txt"], ["-nz", "p", "nonl.txt"], ["-zn", "p", "nonl.txt"], ["-sn", "$p", "a.txt", "one.txt"], ["-ns", "$p", "a.txt", "one.txt"],
    ["-nse", "$p", "a.txt", "one.txt"], ["-nEe", "s/(a)/\\1/p", "a.txt"], ["-nEf", "s_sub.sed", "a.txt"], ["-ni.bak", "p", "one.txt"],
    ["-n", "5q;p", "a.txt"], ["5q", "a.txt"], ["10q", "a.txt"], ["-n", "$!{p}", "a.txt"], ["-n", "$!{$!p}", "a.txt"],
]:
    add("cli", args)
for args in [
    ["-n", "p", "-", "-"], ["-", "-"], ["-s", "-n", "$p", "-", "a.txt"], ["--", "-"], ["-e", "p", "--", "-"], ["-n", "F", "--", "-"],
]:
    add("cli", args, stdin="one.txt")

# Text commands and their interaction with the last line of unterminated input.
for args in [
    ["$c\\\nfoo", "nonl.txt"], ["$a foo", "nonl.txt"], ["$i foo", "nonl.txt"], ["$r rfile.txt", "nonl.txt"], ["$R rfile.txt", "nonl.txt"],
    ["$!N;$a foo", "nonl.txt"], ["$=", "nonl.txt"], ["$l", "nonl.txt"], ["-n", "$p;$p", "nonl.txt"], ["$G", "nonl.txt"], ["$x", "nonl.txt"],
    ["$x;$G", "nonl.txt"], ["x", "nonl.txt"], ["h;G", "nonl.txt"], ["$!d;h;G", "nonl.txt"], ["$s/z/&\\n/", "nonl.txt"], ["s/z/\\n&/", "nonl.txt"],
    ["$d", "nonl.txt"], ["2d", "nonl.txt"], ["3d", "nonl.txt"], ["$!{$!d}", "nonl.txt"], ["N;N;P;P", "nonl.txt"], ["$!N;$!N;N", "nonl.txt"],
    ["2q", "nonl.txt", "a.txt"], ["3q", "nonl.txt", "a.txt"], ["4q", "nonl.txt", "a.txt"], ["3Q", "nonl.txt", "a.txt"], ["$q", "a.txt", "nonl.txt"],
    ["p;p", "onenl.txt"], ["$!N;P;D", "onenl.txt"], ["N", "onenl.txt"], ["n", "onenl.txt"], ["G", "onenl.txt"], ["a x", "onenl.txt"], ["i x", "onenl.txt"],
    ["r rfile.txt", "onenl.txt"], ["w /dev/stdout", "onenl.txt"], ["l", "onenl.txt"], ["=", "onenl.txt"], ["y/o/O/", "onenl.txt"], ["s/$/\\n/", "onenl.txt"],
]:
    add("tail", args)

# The e command and the s///e flag run the shell.
for args in [
    ["e echo hi", "a.txt"], ["1e echo hi", "a.txt"], ["-n", "e echo hi", "one.txt"], ["e", "cmds.txt"], ["-n", "e\np", "cmds.txt"],
    ["s/.*/echo &/e", "a.txt"], ["s/^/echo /e", "one.txt"], ["s/^/echo /ep", "one.txt"], ["s/^/echo /pe", "one.txt"],
    ["-n", "s/^/echo /pe", "one.txt"], ["-n", "s/^/echo /ep", "one.txt"], ["2s/.*/echo X&/e", "a.txt"], ["s/x/y/e", "one.txt"],
    ["$!N;s/\\n/ /;s/^/echo /e", "a.txt"], ["e printf x", "nonl.txt"], ["s/^/printf /e", "nonl.txt"], ["s/z/echo hi/e", "nonl.txt"],
    ["s/^/echo /e", "a.txt", "one.txt"], ["1{e echo first\n}", "a.txt"], ["e echo a;echo b", "one.txt"], ["e true", "one.txt"],
    ["s/^/printf 'a\\nb\\n'; echo /e", "one.txt"], ["--sandbox", "s/^/echo /e", "one.txt"], ["--sandbox", "e", "one.txt"],
]:
    add("eval", args)

# Syntax corners: terminators, blocks, labels, numbers, and whitespace.
for args in [
    ["01p", "n.txt"], ["1,03p", "n.txt"], ["s/1/X/02", "n.txt"], ["-n", "2 ~ 3p", "n.txt"], ["-n", "2,4 ! p", "n.txt"], ["-n", "$ p", "n.txt"],
    ["-n", "/1/ , /3/ p", "n.txt"], ["-n", "/1/I p", "n.txt"], ["-n", "/1/ I p", "n.txt"], ["-n", "/1/,+ 1p", "n.txt"], ["-n", "1,+1 p", "n.txt"],
    ["-n", "{p}", "one.txt"], ["{s/o/0/}", "one.txt"], ["{y/o/0/}", "one.txt"], ["-n", "{=}", "one.txt"], ["-n", "{l}", "one.txt"], ["{n}", "a.txt"],
    ["{q}", "a.txt"], ["{b}", "a.txt"], ["{bx};s/a/X/;:x", "a.txt"], ["{b x};s/a/X/;:x", "a.txt"], ["{t};s/a/X/", "a.txt"], ["s/a/X/;{t};s/e/Y/", "a.txt"],
    ["$!{N;b};s/\\n/+/", "a.txt"], ["{{p}}", "one.txt"], ["{ { p } }", "one.txt"], ["{p;};p", "one.txt"], ["{p}\n{p}", "one.txt"], ["{p} ; {p}", "one.txt"],
    ["p\n\np", "one.txt"], ["p ; ; p", "one.txt"], ["; p", "one.txt"], ["\np", "one.txt"], ["p\n", "one.txt"], ["p\r\n", "one.txt"], ["p;\r", "one.txt"],
    ["s/o/0/;p # note", "one.txt"], ["{p # note\n}", "one.txt"], ["s/a/b/ # note", "a.txt"], ["y/a/b/ # note", "a.txt"], ["a text # still text", "one.txt"],
    ["s/a//", "a.txt"], ["s/a/\\//", "a.txt"], ["s/a/\\&/", "a.txt"], ["s/a/\\\\&/", "a.txt"], ["s/\\(a\\)\\(b\\)\\?/<\\2>/", "a.txt"], ["s/a/\\n/;P;D", "a.txt"],
    ["y/\\n/X/", "a.txt"], ["N;y/\\n/X/", "a.txt"], ["s/l/L/;s//M/", "a.txt"], ["/a/s//X/g", "a.txt"], ["/a/,/b/s//X/", "a.txt"],
    ["99999999999p", "n.txt"], ["-n", "99999999999999999999p", "n.txt"], ["s/a/b/99999999999", "a.txt"], ["-n", "1~99999999999p", "n.txt"],
    ["-n", "w /dev/null", "a.txt"], ["w /dev/null", "a.txt"], ["-n", "w a.txt", "a.txt"], ["r d", "one.txt"], ["r noaccess.txt", "one.txt"], ["R d", "one.txt"],
    ["R noaccess.txt", "one.txt"], ["r ./rfile.txt", "one.txt"], ["r rfile.txt;p", "one.txt"], ["w out.txt}", "one.txt"], ["1{r rfile.txt\n}", "one.txt"],
    ["1{w out.txt\n}", "one.txt"],
]:
    show = ["out.txt", "out.txt}", "a.txt"] if any("w " in a for a in args) else []
    add("corner", args, show=show)

# `l` wrapping around multi-character escapes, and symlinks in other directories.
for width in ["2", "3", "4", "5", "6", "7", "8", "10"]:
    add("wrap", ["-n", "-l", width, "l", "ctl.txt"])
    add("wrap", ["-n", "-l", width, "l", "bin.bin"])
    add("wrap", ["-n", "-l", width, "l", "latin1.txt"])
add("wrap", ["-n", "l 1;l 2;l 3", "one.txt"])
add("wrap", ["-n", "l 100", "long.txt"])
add("wrap", ["-n", "l 70", "long.txt"])
add("wrap", ["-n", "l 71", "long.txt"])
add("wrap", ["-n", "l", "crlf.txt"])
add("wrap", ["-s", "-n", "l", "ctl.txt", "bin.bin"])
add("ip", ["-i", "--follow-symlinks", "s/i/I/", "d/lnk"], show=["d/lnk", "a.txt", "d/f.txt"], setup=["ln -s ../a.txt d/lnk"])
add("ip", ["-i", "s/a/X/", "d/lnk"], show=["d/lnk", "a.txt"], setup=["ln -s ../a.txt d/lnk"])
add("ip", ["-i.bak", "--follow-symlinks", "s/a/X/", "d/lnk"], show=["d/lnk", "a.txt", "a.txt.bak", "d/lnk.bak", "d/a.txt.bak"], setup=["ln -s ../a.txt d/lnk"])
add("ip", ["-i", "-n", "$p", "a.txt", "empty.txt", "one.txt"], show=["a.txt", "empty.txt", "one.txt"])
add("ip", ["-i", "1d", "empty.txt", "a.txt"], show=["empty.txt", "a.txt"])
add("ip", ["-i", "=", "nonl.txt"], show=["nonl.txt"])
add("ip", ["-i", "G", "nonl.txt"], show=["nonl.txt"])
add("ip", ["-i", "$d", "nonl.txt"], show=["nonl.txt"])
add("ip", ["-i", "$a end", "nonl.txt", "onenl.txt"], show=["nonl.txt", "onenl.txt"])
add("ip", ["-n", "-i", "$!N;P;D", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "-e", "1{h;d}", "-e", "$G", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "-s", "-n", "$p", "a.txt", "n.txt"], show=["a.txt", "n.txt"])
add("ip", ["-i", "s/^/> /", "crlf.txt"], show=["crlf.txt"])
add("ip", ["-i", "s/a/X/", "bin.bin"], show=["bin.bin"])
add("ip", ["-i", "-z", "s/^/>/", "bin.bin"], show=["bin.bin"])
add("ip", ["-i", "w out.txt", "a.txt", "one.txt"], show=["out.txt", "a.txt", "one.txt"])
add("ip", ["-i", "R rfile.txt", "a.txt", "one.txt"], show=["a.txt", "one.txt"])
add("ip", ["-i", "q", "a.txt", "one.txt"], show=["a.txt", "one.txt"])
add("ip", ["-i", "Q", "a.txt", "one.txt"], show=["a.txt", "one.txt"])

# Ranges whose start line was consumed before the range command ran, and
# `+N` / `~N` ends passed after lines were consumed by n or N.
for script in ["1d;1,+1p", "1d;1,+0p", "2d;2,+1p", "2d;2,~2p", "2d;2,~4p", "1d;1,~3p", "2d;2,4p", "3d;2,4p", "2d;2,2p", "2,4d;2,6p",
               "1,3d;2,5p", "$!d;1,3p", "1,4d;3,6p", "1,5d;2,3p", "2,+1s/^/R/;2n", "2,~3s/^/R/;2n", "2,3s/^/R/;2n", "/2/,+1s/^/R/;2n",
               "/2/,3s/^/R/;2n", "/2/,/3/s/^/R/;2n", "2,/3/s/^/R/;2n", "/2/,~3s/^/R/;2n", "2,+1s/^/R/;2{N;N;N}", "2,3p;2n;3n", "/3/,+1p;/3/n"]:
    add("passed", ["-n", script, "n.txt"])
    add("passed", [script, "n.txt"])
add("passed", ["-n", "1{N;N;d};1p;2,3p;3p;4p", "multi.txt"])
add("passed", ["1{N;N;d};1p;2,3p;3p;4p", "multi.txt"])
add("zero", ["-z", "-n", "s/$/</Mgp", "nonl.txt"])
add("zero", ["-z", "-n", "s/^/>/Mgp", "nonl.txt"])
add("zero", ["-z", "-n", "s/^./X/Mgp", "nonl.txt"])
add("zero", ["-z", "-n", "l 5", "nonl.txt"])
add("zero", ["-z", "-n", "/b/Mp", "nonl.txt", "a.txt"])

# Exit statuses and diagnostics that depend on runtime state.
for args, stdin in [
    (["q"], "a.txt"),
    (["q0"], "a.txt"),
    (["q1"], "a.txt"),
    (["q255"], "a.txt"),
    (["Q2"], "a.txt"),
    (["$q9"], "a.txt"),
    (["3q4"], "a.txt"),
    (["p", "missing.txt", "q5"], None),
    (["-n", "2{p;q3}", "a.txt"], None),
    (["-n", "w /dev/full", "one.txt"], None),
    (["w /dev/full", "one.txt"], None),
]:
    add("exit", args, stdin=stdin)

# Script file diagnostics.
for args in [
    ["-f", "s_bad.sed", "a.txt"],
    ["-f", "s_bad2.sed", "a.txt"],
    ["-f", "s_open.sed", "a.txt"],
    ["-f", "missing.sed", "a.txt"],
    ["-f", "d", "a.txt"],
    ["-e", "p", "-f", "s_bad.sed", "a.txt"],
    ["-e", "p", "-e", "k", "-e", "p", "a.txt"],
    ["-e", "p;p", "-e", "s/a/b", "a.txt"],
    ["-e", "1{", "-e", "p", "a.txt"],
    ["-e", "1{", "a.txt"],
    ["-e", "s/a/b/;k", "a.txt"],
    ["-e", "p", "-e", "}", "a.txt"],
    ["-e", "a\\", "-e", "foo", "a.txt"],
    ["-e", "a\\", "a.txt"],
    ["-e", "a foo\\", "-e", "bar", "a.txt"],
    ["-e", "s/a/b/;s/c/\\", "-e", "d/", "a.txt"],
    ["-e", "1i\\", "-e", "x", "-e", "2d", "a.txt"],
    ["-e", "y/a/", "-e", "b/", "a.txt"],
    ["-e", "b x", "-e", ":x", "a.txt"],
    ["-n", "-f", "s_p.sed", "-f", "s_q.sed", "a.txt"],
    ["-f", "s_q.sed", "a.txt"],
    ["-f", "s_w.sed", "a.txt"],
    ["-f", "s_r.sed", "one.txt"],
    ["-f", "-", "a.txt"],
]:
    add("script", args, stdin="s_p.sed" if args[-2:-1] == ["-"] else None, show=["out.txt"] if "s_w.sed" in args else [])


# GNU 4.10 selects only the lines up to the end line when a numeric start was
# consumed by an earlier command (`2d;2,1p` selects nothing at line 3); the
# pinned BusyBox suite ("sed 2d;2,1p (gnu compat)", "sed with N skipping lines
# past ranges on next cmds") requires a once-only match on the first line past.
BUSYBOX_CONFLICTS = {"addr-215", "addr-216", "passed-017", "passed-018", "passed-023", "passed-024", "passed-027", "passed-028", "passed-051", "passed-052"}


def _skip(case):
    args = case["args"]
    if "--posix" in args:
        return "posix mode is refused"
    if "--debug" in args:
        return "debug annotation is refused"
    if args and args[-1] in ("--version", "--ver") or args == ["-n", "--version"]:
        return "the pinned BusyBox suite requires a `GNU sed version` first line"
    if case["name"] in BUSYBOX_CONFLICTS:
        return "the pinned BusyBox suite requires a different result"
    for a in args:
        if re.search(r"\\x[89a-fA-F][0-9a-fA-F]", a):
            return "the regex module takes UTF-8 patterns only"
    if case.get("env", {}).get("LC_ALL") == "C.UTF-8":
        return "the host matcher is not multibyte aware"
    for a in args:
        if any(w in a for w in ("\\b", "\\B", "\\<", "\\>")):
            return "word-boundary escapes are rejected by the regex module"
    if tuple(args) in ENGINE_LIMITS:
        return ENGINE_LIMITS[tuple(args)]
    return None


# Matcher behavior of the host POSIX engine that differs from GNU's and needs a
# change in the regex module rather than in the applet.
ENGINE_LIMITS = {
    ("s/\\(a*\\)b\\1/X/", "re.txt"): "backreference to an empty group is not matched",
    ("-E", "s/(a|b)\\1/X/", "re.txt"): "extended syntax has no back-references in the host matcher",
    ("-E", "s/((a)|(b))+/[\\1\\2\\3]/", "re.txt"): "repeated group capture differs from GNU's",
}


for _case in CASES:
    _reason = _skip(_case)
    if _reason:
        _case["skip"] = _reason
