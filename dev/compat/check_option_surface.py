#!/usr/bin/env python3
"""Compare pinned uutils option declarations with XSH applet declarations."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path


REPO = Path(__file__).resolve().parents[2]
LOCK = REPO / "dev" / "compat" / "upstream.lock.json"
SUPPORTED_UTILITIES = (
    "arch",
    "b2sum",
    "base32",
    "base64",
    "basename",
    "basenc",
    "cat",
    "chgrp",
    "chmod",
    "chown",
    "chroot",
    "cksum",
    "comm",
    "csplit",
    "cut",
    "date",
    "df",
    "dircolors",
    "dirname",
    "du",
    "echo",
    "expand",
    "expr",
    "factor",
    "false",
    "fmt",
    "fold",
    "groups",
    "head",
    "hostid",
    "hostname",
    "id",
    "install",
    "join",
    "kill",
    "link",
    "ln",
    "logname",
    "md5sum",
    "mkdir",
    "mkfifo",
    "mknod",
    "mktemp",
    "more",
    "mv",
    "nice",
    "nl",
    "nohup",
    "nproc",
    "numfmt",
    "od",
    "paste",
    "pathchk",
    "pinky",
    "pr",
    "printenv",
    "ptx",
    "pwd",
    "readlink",
    "realpath",
    "rm",
    "rmdir",
    "seq",
    "sha1sum",
    "sha224sum",
    "sha256sum",
    "sha384sum",
    "sha512sum",
    "shred",
    "shuf",
    "sleep",
    "sort",
    "split",
    "stat",
    "stdbuf",
    "sum",
    "sync",
    "tac",
    "tail",
    "tee",
    "timeout",
    "touch",
    "tr",
    "true",
    "truncate",
    "tsort",
    "tty",
    "uname",
    "unexpand",
    "uniq",
    "unlink",
    "uptime",
    "users",
    "wc",
    "who",
    "whoami",
    "yes",
)
SOURCE_PATHS = {
    "arch": ("arch/src/arch.rs", "arch.xsh"),
    "basename": ("basename/src/basename.rs", "basename.xsh"),
    "b2sum": ("b2sum/src/b2sum.rs", "b2sum.xsh"),
    "base32": ("base32/src/base32.rs", "base32.xsh"),
    "base64": ("base64/src/base64.rs", "base64.xsh"),
    "basenc": ("basenc/src/basenc.rs", "basenc.xsh"),
    "cat": ("cat/src/cat.rs", "cat.xsh"),
    "chgrp": ("chgrp/src/chgrp.rs", "chgrp.xsh"),
    "chmod": ("chmod/src/chmod.rs", "chmod.xsh"),
    "chown": ("chown/src/chown.rs", "chown.xsh"),
    "chroot": ("chroot/src/chroot.rs", "chroot.xsh"),
    "comm": ("comm/src/comm.rs", "comm.xsh"),
    "csplit": ("csplit/src/csplit.rs", "csplit.xsh"),
    "cksum": ("cksum/src/cksum.rs", "cksum.xsh"),
    "cut": ("cut/src/cut.rs", "cut.xsh"),
    "date": ("date/src/date.rs", "date.xsh"),
    "df": ("df/src/df.rs", "df.xsh"),
    "dircolors": ("dircolors/src/dircolors.rs", "dircolors.xsh"),
    "dirname": ("dirname/src/dirname.rs", "dirname.xsh"),
    "du": ("du/src/du.rs", "du.xsh"),
    "echo": ("echo/src/echo.rs", "echo.xsh"),
    "expand": ("expand/src/expand.rs", "expand.xsh"),
    "expr": ("expr/src/expr.rs", "expr.xsh"),
    "factor": ("factor/src/factor.rs", "factor.xsh"),
    "false": ("false/src/false.rs", "false.xsh"),
    "fold": ("fold/src/fold.rs", "fold.xsh"),
    "fmt": ("fmt/src/fmt.rs", "fmt.xsh"),
    "groups": ("groups/src/groups.rs", "groups.xsh"),
    "head": ("head/src/cli.rs", "head.xsh"),
    "hostid": ("hostid/src/hostid.rs", "hostid.xsh"),
    "hostname": ("hostname/src/hostname.rs", "hostname.xsh"),
    "id": ("id/src/id.rs", "id.xsh"),
    "install": ("install/src/install.rs", "install.xsh"),
    "join": ("join/src/join.rs", "join.xsh"),
    "kill": ("kill/src/kill.rs", "kill.xsh"),
    "link": ("link/src/link.rs", "link.xsh"),
    "ln": ("ln/src/ln.rs", "ln.xsh"),
    "logname": ("logname/src/logname.rs", "logname.xsh"),
    "mkdir": ("mkdir/src/mkdir.rs", "mkdir.xsh"),
    "md5sum": ("md5sum/src/md5sum.rs", "md5sum.xsh"),
    "mkfifo": ("mkfifo/src/mkfifo.rs", "mkfifo.xsh"),
    "mknod": ("mknod/src/mknod.rs", "mknod.xsh"),
    "mktemp": ("mktemp/src/mktemp.rs", "mktemp.xsh"),
    "more": ("more/src/more.rs", "more.xsh"),
    "mv": ("mv/src/mv.rs", "mv.xsh"),
    "nice": ("nice/src/nice.rs", "nice.xsh"),
    "nl": ("nl/src/nl.rs", "nl.xsh"),
    "nohup": ("nohup/src/nohup.rs", "nohup.xsh"),
    "nproc": ("nproc/src/nproc.rs", "nproc.xsh"),
    "numfmt": ("numfmt/src/numfmt.rs", "numfmt.xsh"),
    "od": ("od/src/od.rs", "od.xsh"),
    "paste": ("paste/src/paste.rs", "paste.xsh"),
    "pathchk": ("pathchk/src/pathchk.rs", "pathchk.xsh"),
    "pinky": ("pinky/src/pinky.rs", "pinky.xsh"),
    "pr": ("pr/src/pr.rs", "pr.xsh"),
    "printenv": ("printenv/src/printenv.rs", "printenv.xsh"),
    "ptx": ("ptx/src/ptx.rs", "ptx.xsh"),
    "pwd": ("pwd/src/pwd.rs", "pwd.xsh"),
    "readlink": ("readlink/src/readlink.rs", "readlink.xsh"),
    "realpath": ("realpath/src/realpath.rs", "realpath.xsh"),
    "rm": ("rm/src/rm.rs", "rm.xsh"),
    "rmdir": ("rmdir/src/rmdir.rs", "rmdir.xsh"),
    "seq": ("seq/src/seq.rs", "seq.xsh"),
    "sha1sum": ("sha1sum/src/sha1sum.rs", "sha1sum.xsh"),
    "sha224sum": ("sha224sum/src/sha224sum.rs", "sha224sum.xsh"),
    "sha256sum": ("sha256sum/src/sha256sum.rs", "sha256sum.xsh"),
    "sha384sum": ("sha384sum/src/sha384sum.rs", "sha384sum.xsh"),
    "sha512sum": ("sha512sum/src/sha512sum.rs", "sha512sum.xsh"),
    "shred": ("shred/src/shred.rs", "shred.xsh"),
    "shuf": ("shuf/src/shuf.rs", "shuf.xsh"),
    "sleep": ("sleep/src/sleep.rs", "sleep.xsh"),
    "sort": ("sort/src/sort.rs", "sort.xsh"),
    "split": ("split/src/cli.rs", "split.xsh"),
    "stat": ("stat/src/stat.rs", "stat.xsh"),
    "stdbuf": ("stdbuf/src/stdbuf.rs", "stdbuf.xsh"),
    "sum": ("sum/src/sum.rs", "sum.xsh"),
    "sync": ("sync/src/sync.rs", "sync.xsh"),
    "tac": ("tac/src/tac.rs", "tac.xsh"),
    "tail": ("tail/src/args.rs", "tail.xsh"),
    "tee": ("tee/src/cli.rs", "tee.xsh"),
    "timeout": ("timeout/src/timeout.rs", "timeout.xsh"),
    "touch": ("touch/src/touch.rs", "touch.xsh"),
    "tr": ("tr/src/tr.rs", "tr.xsh"),
    "true": ("true/src/true.rs", "true.xsh"),
    "truncate": ("truncate/src/truncate.rs", "truncate.xsh"),
    "tsort": ("tsort/src/tsort.rs", "tsort.xsh"),
    "tty": ("tty/src/tty.rs", "tty.xsh"),
    "uname": ("uname/src/uname.rs", "uname.xsh"),
    "unexpand": ("unexpand/src/unexpand.rs", "unexpand.xsh"),
    "uniq": ("uniq/src/uniq.rs", "uniq.xsh"),
    "unlink": ("unlink/src/unlink.rs", "unlink.xsh"),
    "uptime": ("uptime/src/uptime.rs", "uptime.xsh"),
    "users": ("users/src/users.rs", "users.xsh"),
    "wc": ("wc/src/wc.rs", "wc.xsh"),
    "who": ("who/src/who.rs", "who.xsh"),
    "whoami": ("whoami/src/whoami.rs", "whoami.xsh"),
    "yes": ("yes/src/yes.rs", "yes.xsh"),
}
# These utilities read their Clap fields outside the source containing `uu_app`.
UUTILS_BEHAVIOR_PATHS = {
    "head": ("head/src/head.rs",),
    "split": ("split/src/split.rs",),
    "tail": ("tail/src/tail.rs",),
    "tee": ("tee/src/tee.rs",),
    "who": ("who/src/platform/unix.rs",),
}
# Additional sources provide shared argument builders or option constants.
UUTILS_ARGUMENT_PATHS = {
    "chgrp": ("uucore/src/lib/features/perms.rs",),
    "chmod": ("uucore/src/lib/features/perms.rs",),
    "chown": ("uucore/src/lib/features/perms.rs",),
    "install": ("uucore/src/lib/features/backup_control.rs",),
    "ln": ("uucore/src/lib/features/backup_control.rs",),
    "mv": (
        "uucore/src/lib/features/backup_control.rs",
        "uucore/src/lib/features/update_control.rs",
    ),
    "numfmt": ("uu/numfmt/src/options.rs",),
}
CHECKSUM_UTILITIES = (
    "b2sum",
    "cksum",
    "md5sum",
    "sha1sum",
    "sha224sum",
    "sha256sum",
    "sha384sum",
    "sha512sum",
)
CHECKSUM_ARGUMENT_PATHS = (
    "uu/checksum_common/src/lib.rs",
    "uu/checksum_common/src/cli.rs",
)
CHECKSUM_BEHAVIOR_PATHS = ("checksum_common/src/lib.rs",)
UUTILS_SHARED_COMMAND_BUILDERS = {
    "b2sum": "standalone_checksum_app_with_length",
    **{
        utility: "standalone_checksum_app"
        for utility in CHECKSUM_UTILITIES
        if utility not in ("b2sum", "cksum")
    },
}
for _utility in CHECKSUM_UTILITIES:
    UUTILS_ARGUMENT_PATHS[_utility] = CHECKSUM_ARGUMENT_PATHS
    UUTILS_BEHAVIOR_PATHS[_utility] = CHECKSUM_BEHAVIOR_PATHS
BASE_ENCODING_UTILITIES = ("base32", "base64", "basenc")
BASE_ENCODING_ARGUMENT_PATHS = ("uu/base32/src/base_common.rs",)
BASE_ENCODING_BEHAVIOR_PATHS = ("base32/src/base_common.rs",)
for _utility in BASE_ENCODING_UTILITIES:
    UUTILS_ARGUMENT_PATHS[_utility] = BASE_ENCODING_ARGUMENT_PATHS
    UUTILS_BEHAVIOR_PATHS[_utility] = BASE_ENCODING_BEHAVIOR_PATHS
    UUTILS_SHARED_COMMAND_BUILDERS[_utility] = "base_app"
# `sum` reads these algorithm selectors from raw byte argv instead of parser
# fields, so their source branches establish their disposition.
MANUAL_RAW_OPTION_USES = {
    "sum": {
        "-s": "if flag == 115",
        "--sysv": 'if arg == b"--sysv"',
    },
}
# These options are scanned manually or consumed through a shared helper,
# rather than through a direct source reference to their Clap argument ID.
MANUAL_UUTILS_OPTION_USES = {
    "echo": {
        "options::NO_NEWLINE": "b'n' => options_.trailing_newline = false",
        "options::ENABLE_BACKSLASH_ESCAPE": "b'e' => options_.escape = true",
        "options::DISABLE_BACKSLASH_ESCAPE": "b'E' => options_.escape = false",
    },
    "install": {
        "OPT_BACKUP": "backup_control::determine_backup_mode",
        "OPT_BACKUP_NO_ARG": "backup_control::determine_backup_mode",
        "OPT_SUFFIX": "backup_control::determine_backup_suffix",
    },
    "ln": {
        "OPT_BACKUP": "backup_control::determine_backup_mode",
        "OPT_BACKUP_NO_ARG": "backup_control::determine_backup_mode",
        "OPT_SUFFIX": "backup_control::determine_backup_suffix",
    },
    "mv": {
        "OPT_BACKUP": "backup_control::determine_backup_mode",
        "OPT_BACKUP_NO_ARG": "backup_control::determine_backup_mode",
        "OPT_SUFFIX": "backup_control::determine_backup_suffix",
        "OPT_UPDATE": "update_control::determine_update_mode",
        "OPT_UPDATE_NO_ARG": "update_control::determine_update_mode",
    },
    "mknod": {
        "options::MODE": 'matches.get_one::<String>("mode")',
    },
    **{
        utility: {
            "options::verbosity::CHANGES": "matches.get_flag(options::verbosity::CHANGES)",
            "options::verbosity::SILENT": "matches.get_flag(options::verbosity::SILENT)",
            "options::verbosity::QUIET": "matches.get_flag(options::verbosity::QUIET)",
            "options::verbosity::VERBOSE": "matches.get_flag(options::verbosity::VERBOSE)",
            "options::preserve_root::PRESERVE": "matches.get_flag(options::preserve_root::PRESERVE)",
            "options::preserve_root::NO_PRESERVE": "matches.get_flag(options::preserve_root::NO_PRESERVE)",
            "options::RECURSIVE": "matches.get_flag(options::RECURSIVE)",
            "options::dereference::DEREFERENCE": "matches.get_flag(options::dereference::DEREFERENCE)",
            "options::dereference::NO_DEREFERENCE": "matches.get_flag(options::dereference::NO_DEREFERENCE)",
            "traverse::TRAVERSE": 'matches.get_flag("H")',
            "traverse::EVERY": 'matches.get_flag("L")',
            "traverse::NO_TRAVERSE": 'matches.get_flag("P")',
        }
        for utility in ("chgrp", "chmod", "chown")
    },
}
CHECKSUM_MANUAL_UUTILS_OPTION_USES = {
    "options::CHECK": 'matches.get_flag("check")',
    "options::WARN": 'check_flag("warn")',
    "options::STATUS": 'check_flag("status")',
    "options::QUIET": 'check_flag("quiet")',
    "options::IGNORE_MISSING": 'check_flag("ignore-missing")',
    "options::STRICT": 'check_flag("strict")',
}
for _utility in CHECKSUM_UTILITIES:
    MANUAL_UUTILS_OPTION_USES[_utility] = CHECKSUM_MANUAL_UUTILS_OPTION_USES
PERMISSION_COMMON_ARGUMENTS = {
    "markers": (
        "uucore::perms::common_args()",
        "pub fn common_args() -> Vec<Arg>",
        "Arg::new(traverse::TRAVERSE)",
        ".short(traverse::TRAVERSE.chars().next().unwrap())",
        "Arg::new(traverse::EVERY)",
        ".short(traverse::EVERY.chars().next().unwrap())",
        "Arg::new(traverse::NO_TRAVERSE)",
        ".short(traverse::NO_TRAVERSE.chars().next().unwrap())",
        "Arg::new(options::dereference::DEREFERENCE)",
        ".long(options::dereference::DEREFERENCE)",
        "Arg::new(options::dereference::NO_DEREFERENCE)",
        ".short('h')",
        ".long(options::dereference::NO_DEREFERENCE)",
    ),
    "spellings": {
        "-H": 0,
        "-L": 0,
        "-P": 0,
        "--dereference": 0,
        "-h": 0,
        "--no-dereference": 0,
    },
}
MANUAL_UUTILS_OPTIONS = {
    **{
        utility: PERMISSION_COMMON_ARGUMENTS
        for utility in ("chgrp", "chmod", "chown")
    },
    "sort": {
        "markers": (
            "fn make_sort_mode_arg(",
            "make_sort_mode_arg(options::modes::HUMAN_NUMERIC,'h',",
            "make_sort_mode_arg(options::modes::MONTH,'M',",
            "make_sort_mode_arg(options::modes::NUMERIC,'n',",
            "make_sort_mode_arg(options::modes::GENERAL_NUMERIC,'g',",
            "make_sort_mode_arg(options::modes::VERSION,'V',",
            "make_sort_mode_arg(options::modes::RANDOM,'R',",
        ),
        "spellings": {
            "-h": 0,
            "--human-numeric-sort": 0,
            "-M": 0,
            "--month-sort": 0,
            "-n": 0,
            "--numeric-sort": 0,
            "-g": 0,
            "--general-numeric-sort": 0,
            "-V": 0,
            "--version-sort": 0,
            "-R": 0,
            "--random-sort": 0,
        },
    },
    "basenc": {
        "markers": (
            "fn get_encodings()",
            '("base64",Format::Base64,',
            '("base64url",Format::Base64Url,',
            '("base32",Format::Base32,',
            '("base32hex",Format::Base32Hex,',
            '("base16",Format::Base16,',
            '("base2lsbf",Format::Base2Lsbf,',
            '("base2msbf",Format::Base2Msbf,',
            '("z85",Format::Z85,',
            '("base58",Format::Base58,',
            "Arg::new(encoding.0)",
            ".long(encoding.0)",
            "matches.get_flag(encoding.0)",
        ),
        "spellings": {
            "--base64": 0,
            "--base64url": 0,
            "--base32": 0,
            "--base32hex": 0,
            "--base16": 0,
            "--base2lsbf": 0,
            "--base2msbf": 0,
            "--z85": 0,
            "--base58": 0,
        },
    },
}
PERMISSION_XSH_MARKERS = (
    "perm.options(",
    'let known = ["--recursive"',
    'word == "--help"',
    'word == "--version"',
    'word == "--recursive"',
    'word == "--reference"',
    'word == "--preserve-root"',
    'word == "--no-preserve-root"',
    'word == "--dereference"',
    'word == "--no-dereference"',
    'word == "--silent"',
    'word == "--quiet"',
    'word == "--verbose"',
    'word == "--changes"',
    '"R" => recursive = true',
    '"H" => traversal = "H"',
    '"L" => traversal = "L"',
    '"P" => traversal = "P"',
    '"h" => { dereference = false; explicit_dereference = true }',
)
PERMISSION_XSH_COMMON_OPTIONS = {
    "-H": 0,
    "-L": 0,
    "-P": 0,
    "--dereference": 0,
    "-h": 0,
    "--no-dereference": 0,
    "-R": 0,
    "--recursive": 0,
    "-c": 0,
    "--changes": 0,
    "-f": 0,
    "--silent": 0,
    "--quiet": 0,
    "-v": 0,
    "--verbose": 0,
    "--preserve-root": 0,
    "--no-preserve-root": 0,
    "--reference": 1,
    "--help": 0,
    "--version": 0,
}
MANUAL_XSH_OPTIONS = {
    "date": {
        "markers": (
            'arg == "--help"',
            'arg == "--version"',
            'arg == "--debug"',
            'arg == "--resolution"',
            'arg.starts_with("-I")',
            'arg.starts_with("--iso-8601")',
            'arg.starts_with("--rfc-3339")',
            'arg == "-d" or arg == "--date"',
            'arg == "-f" or arg == "--file"',
            'arg == "-r" or arg == "--reference"',
            'arg == "-s" or arg == "--set"',
            'arg == "--universal"',
            'arg == "--uct"',
            'arg == "--rfc-822"',
            'arg == "--rfc-2822"',
        ),
        "spellings": {
            "--help": 0,
            "--version": 0,
            "--debug": 0,
            "--resolution": 0,
            "-I": (0, 1),
            "--iso-8601": (0, 1),
            "--rfc-3339": 1,
            "-d": 1,
            "--date": 1,
            "-f": 1,
            "--file": 1,
            "-r": 1,
            "--reference": 1,
            "-s": 1,
            "--set": 1,
            "-u": 0,
            "--utc": 0,
            "--universal": 0,
            "--uct": 0,
            "-R": 0,
            "--rfc-email": 0,
            "--rfc-822": 0,
            "--rfc-2822": 0,
        },
    },
    "basenc": {
        "markers": (
            'let selectors = ["--base64", "--base64url", "--base32", "--base32hex", "--base16", "--base2msbf", "--base2lsbf", "--z85", "--base58"]',
            'arg in selectors and default_kind == ""',
        ),
        "spellings": {
            "--base64": 0,
            "--base64url": 0,
            "--base32": 0,
            "--base32hex": 0,
            "--base16": 0,
            "--base2msbf": 0,
            "--base2lsbf": 0,
            "--z85": 0,
            "--base58": 0,
        },
    },
    "dircolors": {
        "markers": (
            'arg == "--help"',
            'arg == "--version"',
            'arg.starts_with("--")',
            '"--sh" | "--bourne-shell"',
            '"--csh" | "--c-shell"',
            '"--print-database"',
            '"--print-ls-colors"',
            'arg.starts_with("-") and arg != "-"',
            'arg.byte_slice(1)',
            '"b" => shell = "b"',
            '"c" => shell = "c"',
            '"p" => database = true',
        ),
        "spellings": {
            "--help": 0,
            "--version": 0,
            "-b": 0,
            "--sh": 0,
            "--bourne-shell": 0,
            "-c": 0,
            "--csh": 0,
            "--c-shell": 0,
            "-p": 0,
            "--print-database": 0,
            "--print-ls-colors": 0,
        },
    },
    "expr": {
        "markers": (
            'argv.len() == 1 and argv[0] == b"--help"',
            'argv.len() == 1 and argv[0] == b"--version"',
        ),
        "spellings": {"--help": 0, "--version": 0},
    },
    "chgrp": {
        "markers": PERMISSION_XSH_MARKERS
        + ('! chmod and (word == "--from" or word.starts_with("--from="))',),
        "spellings": {**PERMISSION_XSH_COMMON_OPTIONS, "--from": 1},
    },
    "chmod": {
        "markers": PERMISSION_XSH_MARKERS,
        "spellings": PERMISSION_XSH_COMMON_OPTIONS,
    },
    "chown": {
        "markers": PERMISSION_XSH_MARKERS
        + ('! chmod and (word == "--from" or word.starts_with("--from="))',),
        "spellings": {**PERMISSION_XSH_COMMON_OPTIONS, "--from": 1},
    },
    "cat": {
        "markers": (
            "tio.without_presume_pipe(",
            'item == "---presume-input-pipe"',
        ),
        "spellings": {"---presume-input-pipe": 0},
    },
    "head": {
        "markers": (
            "tio.without_presume_pipe(",
            'item == "---presume-input-pipe"',
        ),
        "spellings": {"---presume-input-pipe": 0},
    },
    "od": {
        "markers": (
            '"abcdDfFhHiIlLOosxX".find(char) != null',
            'arg.starts_with("--format=")',
            'arg == "--format"',
            'arg == "-t"',
        ),
        "spellings": {
            **{f"-{character}": 0 for character in "abcdDfFhHiIlLOosxX"},
            "-t": 1,
            "--format": 1,
        },
    },
    "split": {
        "markers": (
            'item == "---io-blksize"',
            'item.starts_with("---io-blksize=")',
        ),
        "spellings": {"---io-blksize": 1},
    },
    "tail": {
        "markers": (
            "tio.without_presume_pipe(",
            'item == "---presume-input-pipe"',
        ),
        "spellings": {"---presume-input-pipe": 0},
    },
}
MANUAL_XSH_OPTION_PATHS = {
    "chgrp": ("lib/perm.xsh",),
    "chmod": ("lib/perm.xsh",),
    "chown": ("lib/perm.xsh",),
    "cat": ("lib/textio_a1.xsh",),
    "head": ("lib/textio_a1.xsh",),
    "tail": ("lib/textio_a1.xsh",),
}
for _utility in CHECKSUM_UTILITIES:
    MANUAL_XSH_OPTION_PATHS[_utility] = ("lib/checksums.xsh",)
for _utility in BASE_ENCODING_UTILITIES:
    MANUAL_XSH_OPTION_PATHS[_utility] = ("lib/bytes_enc.xsh",)


class SurfaceParseError(ValueError):
    pass


Arity = int | tuple[int, int | None]

def _arity(minimum: int, maximum: int | None) -> Arity:
    return minimum if maximum == minimum else (minimum, maximum)


def _matching_delimiter(source: str, start: int, opening: str, closing: str, language: str) -> int:
    depth = 0
    index = start
    line_comment = False
    block_comment = 0
    quoted = False
    escaped = False

    while index < len(source):
        char = source[index]
        following = source[index + 1] if index + 1 < len(source) else ""

        if line_comment:
            if char == "\n":
                line_comment = False
            index += 1
            continue

        if block_comment:
            if char == "/" and following == "*":
                block_comment += 1
                index += 2
            elif char == "*" and following == "/":
                block_comment -= 1
                index += 2
            else:
                index += 1
            continue

        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
            index += 1
            continue

        if language == "rust":
            if char == "/" and following == "/":
                line_comment = True
                index += 2
                continue
            if char == "/" and following == "*":
                block_comment = 1
                index += 2
                continue
            if char == "'":
                literal_end = index + 1
                if literal_end < len(source) and source[literal_end] == "\\":
                    literal_end += 2
                else:
                    literal_end += 1
                if literal_end < len(source) and source[literal_end] == "'":
                    index = literal_end + 1
                    continue
        elif char == "#":
            line_comment = True
            index += 1
            continue

        if char == '"':
            quoted = True
        elif char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return index
        index += 1

    raise SurfaceParseError(f"unclosed {opening!r} block")


def _rust_string_constants(source: str) -> dict[str, str]:
    return {
        name: value
        for name, value in re.findall(
            r"\b(?:pub\s+)?(?:static|const)\s+(\w+)\s*:\s*&str\s*=\s*\"([^\"]+)\"",
            source,
        )
    }


def _rust_char_constants(source: str) -> dict[str, str]:
    return {
        name: value
        for name, value in re.findall(
            r"\b(?:pub\s+)?const\s+(\w+)\s*:\s*char\s*=\s*'([^'\\])'",
            source,
        )
    }


def _function_body(source: str, signature: str, language: str) -> str:
    match = re.search(signature, source)
    if not match:
        raise SurfaceParseError(f"source is missing {signature!r}")
    opening = source.find("{", match.end())
    if opening < 0:
        raise SurfaceParseError("function body is missing")
    closing = _matching_delimiter(source, opening, "{", "}", language)
    return source[opening + 1 : closing]


def _shared_argument_bodies(body: str, helper_sources: tuple[str, ...]) -> list[str]:
    arguments = []
    for argument in _rust_argument_blocks(body):
        helper = re.fullmatch(
            r"\s*(\w+)::arguments::(\w+)\s*\(\s*\)\s*", argument
        )
        if not helper:
            continue
        namespace, name = helper.groups()
        signature = rf"\bpub\s+fn\s+{re.escape(name)}\s*\(\s*\)"
        for source in helper_sources:
            if re.search(signature, source):
                helper_body = _function_body(source, signature, "rust")
                if not re.search(r"\b(?:clap::)?Arg::new\s*\(", helper_body):
                    raise SurfaceParseError(
                        f"shared Clap argument builder {namespace}::{name} has no Arg::new call"
                    )
                arguments.append(helper_body)
                break
        else:
            raise SurfaceParseError(
                f"shared Clap argument builder {namespace}::{name} has no source"
            )

    # Checksum constructors add Clap arguments through this shared trait.
    method_sources = []
    method_names = set()
    for source in helper_sources:
        match = re.search(r"\bimpl\s+ChecksumCommand\s+for\s+Command\b", source)
        if match:
            impl_body = _function_body(
                source, r"\bimpl\s+ChecksumCommand\s+for\s+Command\b", "rust"
            )
            method_sources.append(impl_body)
            method_names.update(re.findall(r"\bfn\s+(\w+)\s*\(", impl_body))
    if method_names:
        called_methods = _rust_method_arguments(body, method_names)
        for method, _call_arguments in called_methods:
            for impl_body in method_sources:
                signature = rf"\bfn\s+{re.escape(method)}\s*\("
                if not re.search(signature, impl_body):
                    continue
                method_body = _function_body(impl_body, signature, "rust")
                for argument in _rust_argument_blocks(method_body):
                    variable = argument.strip()
                    if re.fullmatch(r"\w+", variable):
                        initialized = re.search(
                            rf"\blet\s+(?:mut\s+)?{re.escape(variable)}\s*=\s*(.*?);",
                            method_body,
                            re.S,
                        )
                        if initialized:
                            arguments.append(initialized.group(1))
                            continue
                    arguments.append(argument)
                break
            else:
                raise SurfaceParseError(
                    f"shared checksum argument method {method} has no source"
                )
    return arguments


def _rust_argument_blocks(body: str) -> list[str]:
    return [argument for _method, argument in _rust_method_arguments(body, {"arg"})]


def _rust_char_argument(body: str, method: str) -> str | None:
    match = re.search(
        rf"\.{method}\s*\(\s*(?:Some\s*\(\s*)?'([^'\\])'\s*\)?\s*\)",
        body,
    )
    return match.group(1) if match else None


def _rust_char_constant_argument(
    body: str, method: str, constants: dict[str, str]
) -> str | None:
    match = re.search(rf"\.{method}\s*\(\s*([\w:]+)\s*\)", body)
    if not match:
        return None
    name = match.group(1).removeprefix("options::")
    if name not in constants:
        name = name.rsplit("::", 1)[-1]
    return constants.get(name)


def _rust_method_arguments(body: str, methods: set[str]) -> list[tuple[str, str]]:
    calls = []
    delimiters = {"(": ")", "[": "]", "{": "}"}
    index = 0
    while index < len(body):
        char = body[index]
        following = body[index + 1] if index + 1 < len(body) else ""
        if char == "/" and following == "/":
            newline = body.find("\n", index + 2)
            index = len(body) if newline < 0 else newline + 1
            continue
        if char == "/" and following == "*":
            depth = 1
            index += 2
            while index < len(body) and depth:
                if body[index : index + 2] == "/*":
                    depth += 1
                    index += 2
                elif body[index : index + 2] == "*/":
                    depth -= 1
                    index += 2
                else:
                    index += 1
            continue
        if char in ('"', "'"):
            literal_end = index + 1
            while literal_end < len(body):
                if body[literal_end] == "\\":
                    literal_end += 2
                elif body[literal_end] == char:
                    literal_end += 1
                    break
                else:
                    literal_end += 1
            index = literal_end
            continue
        if char == ".":
            call = re.match(r"\.(\w+)\s*\(", body[index:])
            if call:
                opening = body.find("(", index)
                closing = _matching_delimiter(body, opening, "(", ")", "rust")
                if call.group(1) in methods:
                    calls.append((call.group(1), body[opening + 1 : closing]))
                index = closing + 1
                continue
        if char in delimiters:
            index = _matching_delimiter(body, index, char, delimiters[char], "rust") + 1
            continue
        index += 1
    return calls


def _rust_string_array_arguments(body: str, method: str) -> list[str]:
    values = []
    message = (
        "visible alias is not a literal string"
        if method == "visible_aliases"
        else f"{method} alias is not a literal string"
    )
    for _method, raw_argument in _rust_method_arguments(body, {method}):
        argument = raw_argument.strip()
        if not (argument.startswith("[") and argument.endswith("]")):
            raise SurfaceParseError(message)
        remaining = argument[1:-1].strip()
        while remaining:
            match = re.match(r'"([^"\\]*)"\s*(,|$)', remaining)
            if not match:
                raise SurfaceParseError(message)
            values.append(match.group(1))
            remaining = remaining[match.end() :].strip()
            if match.group(2) == "," and not remaining:
                break
    return values


def parse_uutils_declarations(
    source: str,
    utility: str | None = None,
    behavior_source: str = "",
    argument_sources: tuple[str, ...] = (),
    shared_command_builder: str | None = None,
) -> dict[str, dict[str, Arity | str]]:
    """Read literal Clap declarations, including generated help flags."""
    constants = _rust_string_constants(source)
    char_constants = _rust_char_constants(source)
    for helper_source in argument_sources:
        constants.update(_rust_string_constants(helper_source))
        char_constants.update(_rust_char_constants(helper_source))
    signature = r"\bpub\s+fn\s+uu_app\s*\(\)"
    function = re.search(signature, source)
    if not function and shared_command_builder is None:
        raise SurfaceParseError(f"source is missing {signature!r}")
    body_parts = []
    if function:
        body_parts.append(_function_body(source, signature, "rust"))
        function_open = source.find("{", function.end())
        function_close = _matching_delimiter(source, function_open, "{", "}", "rust")
        behavior = source[:function_open] + source[function_close + 1 :] + behavior_source
    else:
        macro = re.search(
            rf"uu_checksum_common::declare_standalone!\s*\(\s*\"{re.escape(utility or '')}\"\s*,",
            source,
        )
        if not macro:
            raise SurfaceParseError(
                f"source is missing {signature!r} and the expected standalone checksum macro"
            )
        behavior = source + behavior_source

    if shared_command_builder is not None:
        if function and shared_command_builder not in body_parts[0]:
            raise SurfaceParseError(
                f"source does not call shared command builder {shared_command_builder}"
            )
        builder_signature = rf"\bpub\s+fn\s+{re.escape(shared_command_builder)}\s*\("
        builder_source = next(
            (candidate for candidate in argument_sources if re.search(builder_signature, candidate)),
            None,
        )
        if builder_source is None:
            raise SurfaceParseError(
                f"shared command builder {shared_command_builder} has no source"
            )
        builder_body = _function_body(builder_source, builder_signature, "rust")
        body_parts.append(builder_body)
        if utility in CHECKSUM_UTILITIES:
            default_signature = r"\bpub\s+fn\s+default_checksum_app\s*\("
            default_body = _function_body(builder_source, default_signature, "rust")
            body_parts.append(default_body)

    body = "\n".join(body_parts)
    if shared_command_builder is None and "default_checksum_app(" in body:
        for helper_source in argument_sources:
            default_signature = r"\bpub\s+fn\s+default_checksum_app\s*\("
            if re.search(default_signature, helper_source):
                body += "\n" + _function_body(helper_source, default_signature, "rust")
                break
    declarations: dict[str, dict[str, Arity | str]] = {}

    aliases = r"\.(?:aliases|short_aliases|visible_short_aliases)\s*\("
    if re.search(aliases, body):
        raise SurfaceParseError(
            "this Clap alias form needs an explicit parser before the utility can be checked"
        )

    arguments = _rust_argument_blocks(body)
    arguments.extend(_shared_argument_bodies(body, argument_sources))
    for argument in arguments:
        if ".short(" not in argument and ".long(" not in argument:
            continue

        action = re.search(r"\.action\s*\(\s*ArgAction::(\w+)\s*\)", argument)
        action_name = action.group(1) if action else "Set"
        num_args = re.search(
            r"\.num_args\s*\(\s*(\d*)(?:(\.\.=|\.\.)(\d*))?\s*\)", argument
        )
        if ".num_args(" in argument and not num_args:
            raise SurfaceParseError("variable or optional argument counts need an explicit parser")
        declared_arity: Arity | None = None
        if num_args:
            minimum_text = num_args.group(1)
            range_operator = num_args.group(2)
            maximum_text = num_args.group(3)
            if not minimum_text and not range_operator:
                raise SurfaceParseError("argument count has no lower bound or range operator")
            minimum = int(minimum_text) if minimum_text else 0
            if range_operator:
                if range_operator == "..=" and not maximum_text:
                    raise SurfaceParseError("inclusive argument count has no upper bound")
                maximum = int(maximum_text) if maximum_text else None
                if range_operator == ".." and maximum is not None:
                    maximum -= 1
                if maximum is not None and maximum < minimum:
                    raise SurfaceParseError("argument count range is empty")
                declared_arity = _arity(minimum, maximum)
            else:
                declared_arity = minimum
        if action_name in ("SetTrue", "SetFalse", "Count", "Help", "HelpLong", "Version"):
            arity = 0
            if declared_arity not in (None, 0):
                raise SurfaceParseError("flag action declares value arguments")
        elif action_name in ("Set", "Append"):
            arity = declared_arity if declared_arity is not None else 1
        else:
            raise SurfaceParseError(f"option argument has unsupported ArgAction::{action_name}")

        arg_id = re.search(r"Arg::new\s*\(\s*([^)]+)\s*\)", argument)
        if not arg_id:
            raise SurfaceParseError("option argument has no readable Arg ID")
        field = arg_id.group(1).removeprefix("options::")
        manual_use = MANUAL_UUTILS_OPTION_USES.get(utility, {}).get(arg_id.group(1))
        disposition = (
            "implemented"
            if action_name in ("Help", "HelpLong", "Version")
            or re.search(rf"\b{re.escape(arg_id.group(1))}\b", behavior)
            or (
                manual_use is not None
                and manual_use in behavior + "\n" + "\n".join(argument_sources)
            )
            else "parsed-but-unused"
        )

        short = _rust_char_argument(argument, "short")
        if short is None:
            short = _rust_char_constant_argument(argument, "short", char_constants)
        if ".short(" in argument and short is None:
            raise SurfaceParseError("short option is not a literal character")
        short_alias_calls = _rust_method_arguments(argument, {"short_alias"})
        short_aliases = []
        for _method, raw_alias in short_alias_calls:
            match = re.fullmatch(r"\s*'([^'\\])'\s*", raw_alias)
            if match:
                short_aliases.append(match.group(1))
        if len(short_alias_calls) != len(short_aliases):
            raise SurfaceParseError("short alias is not a literal character")
        visible_short_alias_calls = _rust_method_arguments(
            argument, {"visible_short_alias"}
        )
        visible_short_aliases = []
        for _method, raw_alias in visible_short_alias_calls:
            match = re.fullmatch(r"\s*'([^'\\])'\s*", raw_alias)
            if match:
                visible_short_aliases.append(match.group(1))
        if len(visible_short_alias_calls) != len(visible_short_aliases):
            raise SurfaceParseError("visible short alias is not a literal character")
        long = re.search(r"\.long\s*\(\s*([^\s)]+)\s*\)", argument)
        if short:
            declarations[f"-{short}"] = {"arity": arity, "field": field, "disposition": disposition}
        if long:
            value = long.group(1)
            if value.startswith("options::"):
                name = value.removeprefix("options::")
                if name not in constants:
                    final_name = name.rsplit("::", 1)[-1]
                    if final_name not in constants:
                        raise SurfaceParseError(f"unknown uutils option constant {name}")
                    name = final_name
                value = constants[name]
            elif value in constants:
                value = constants[value]
            elif value.startswith('"') and value.endswith('"'):
                value = value[1:-1]
            else:
                raise SurfaceParseError(f"unsupported Clap long option expression {value!r}")
            declarations[f"--{value}"] = {"arity": arity, "field": field, "disposition": disposition}
        alias_calls = _rust_method_arguments(argument, {"alias", "visible_alias"})
        aliases = []
        for _method, raw_alias in alias_calls:
            value = raw_alias.strip()
            literal = re.fullmatch(r'"([^"\\]+)"', value)
            if literal:
                aliases.append(literal.group(1))
            elif value.startswith("options::"):
                name = value.removeprefix("options::")
                if name not in constants:
                    raise SurfaceParseError(f"unknown uutils option constant {name}")
                aliases.append(constants[name])
            elif value in constants:
                aliases.append(constants[value])
            else:
                raise SurfaceParseError("Clap alias is not a literal string")
        visible_aliases = _rust_string_array_arguments(argument, "visible_aliases")
        for alias in aliases + visible_aliases:
            declarations[f"--{alias}"] = {"arity": arity, "field": field, "disposition": disposition}
        for alias in short_aliases + visible_short_aliases:
            declarations[f"-{alias}"] = {"arity": arity, "field": field, "disposition": disposition}

    manual = MANUAL_UUTILS_OPTIONS.get(utility)
    if manual:
        manual_source = source + "\n" + "\n".join(argument_sources)
        normalized_source = re.sub(r"\s+", "", manual_source)
        if not all(
            re.sub(r"\s+", "", marker) in normalized_source
            for marker in manual["markers"]
        ):
            raise SurfaceParseError(f"manual {utility} option declarations are not recognized")
        for spelling, arity in manual["spellings"].items():
            previous = declarations.get(spelling)
            if previous is not None and previous["arity"] != arity:
                raise SurfaceParseError(f"{spelling} has conflicting argument counts")
            declarations[spelling] = {
                "arity": arity,
                "field": "manual",
                "disposition": "implemented",
            }

    if ".disable_help_flag(true)" not in body:
        help_short = _rust_char_argument(body, "help_short")
        if ".help_short(" in body and help_short is None:
            raise SurfaceParseError("custom help short option is not a literal character")
        help_short = help_short or "h"
        generated = {"arity": 0, "field": "clap-generated", "disposition": "generated"}
        declarations[f"-{help_short}"] = generated.copy()
        declarations["--help"] = generated.copy()
    if re.search(r"\.version\s*\(", body) and ".disable_version_flag(true)" not in body:
        version_short = _rust_char_argument(body, "version_short")
        if ".version_short(" in body and version_short is None:
            raise SurfaceParseError("custom version short option is not a literal character")
        version_short = version_short or "V"
        generated = {"arity": 0, "field": "clap-generated", "disposition": "generated"}
        declarations[f"-{version_short}"] = generated.copy()
        declarations["--version"] = generated.copy()

    return declarations


def parse_uutils_source(source: str) -> dict[str, Arity]:
    return {
        spelling: declaration["arity"]
        for spelling, declaration in parse_uutils_declarations(source).items()
    }


def _form_entries(form: str, field: str) -> list[tuple[str, Arity]]:
    tokens = [token.rstrip(",") for token in form.split()]
    entries = []
    index = 0
    while index < len(tokens):
        if not tokens[index].startswith("-") or tokens[index] == "--":
            index += 1
            continue

        aliases: list[tuple[str, Arity | None]] = []
        while index < len(tokens) and tokens[index].startswith("-") and tokens[index] != "--":
            token = tokens[index]
            optional = re.fullmatch(
                r"(?P<spelling>--?[A-Za-z0-9-]+)\[(?:=(?P<value>[^\]]+))?\]", token
            )
            if optional:
                aliases.append((optional.group("spelling"), (0, 1)))
            else:
                if "[" in token or "]" in token:
                    raise SurfaceParseError(
                        f"{field} uses an optional form that needs explicit arity handling"
                    )
                spelling, separator, _value = token.partition("=")
                aliases.append((spelling, 1 if separator else None))
            index += 1

        explicit_arities = {arity for _spelling, arity in aliases if arity is not None}
        if len(explicit_arities) > 1:
            raise SurfaceParseError(f"{field} declares aliases with different argument counts")
        if explicit_arities:
            arity = explicit_arities.pop()
        else:
            arity = 0
            while (
                index < len(tokens)
                and not tokens[index].startswith("-")
                and not tokens[index].startswith("...")
            ):
                arity += 1
                index += 1
        for spelling, _explicit_arity in aliases:
            entries.append((spelling, arity))
    return entries


def _manual_xsh_declarations(
    source: str, utility: str
) -> dict[str, dict[str, Arity | str]]:
    if utility not in ("true", "false", "echo"):
        raise SurfaceParseError("XSH applet has no cli.applet schema")
    if not re.search(r"argv\.len\(\)\s*(?:==|!=)\s*1", source):
        raise SurfaceParseError(f"manual {utility} help/version guard is not recognized")
    spellings = set(re.findall(r'argv\s*\[\s*0\s*\]\s*==\s*"(--[A-Za-z0-9-]+)"', source))
    if spellings != {"--help", "--version"}:
        raise SurfaceParseError(f"manual {utility} option branches are not recognized")
    declarations = {
        spelling: {"arity": 0, "field": "manual", "disposition": "implemented"}
        for spelling in sorted(spellings)
    }

    if utility == "echo":
        markers = (
            'rx"^-[neE]+$"',
            'flag == "e"',
            'flag == "E"',
            "newline = false",
        )
        if not all(marker in source for marker in markers):
            raise SurfaceParseError("manual echo option scan is not recognized")
        declarations.update(
            {
                spelling: {"arity": 0, "field": "manual", "disposition": "implemented"}
                for spelling in ("-n", "-e", "-E")
            }
        )

    return declarations


def parse_xsh_declarations(
    source: str, utility: str | None = None, helper_sources: tuple[str, ...] = ()
) -> dict[str, dict[str, Arity | str]]:
    """Read option forms from cli.applet or a narrowly supported manual parser."""
    schema_source = source
    applet = re.search(r"\bcli\.applet\s*\(", schema_source)
    if not applet:
        for helper_source in helper_sources:
            applet = re.search(r"\bcli\.applet\s*\(", helper_source)
            if applet:
                schema_source = helper_source
                break
    if not applet:
        manual = MANUAL_XSH_OPTIONS.get(utility)
        if manual:
            manual_source = source + "\n" + "\n".join(helper_sources)
            normalized_source = re.sub(r"\s+", "", manual_source)
            if not all(
                re.sub(r"\s+", "", marker) in normalized_source
                for marker in manual["markers"]
            ):
                raise SurfaceParseError(f"manual {utility} option scan is not recognized")
            return {
                spelling: {
                    "arity": arity,
                    "field": "manual",
                    "disposition": "implemented",
                }
                for spelling, arity in manual["spellings"].items()
            }
        if utility is None:
            raise SurfaceParseError("XSH applet has no cli.applet call")
        return _manual_xsh_declarations(source, utility)
    opening = schema_source.find("{", applet.end())
    if opening < 0:
        raise SurfaceParseError("cli.applet schema is missing")
    closing = _matching_delimiter(schema_source, opening, "{", "}", "xsh")
    schema = schema_source[opening + 1 : closing]
    declarations: dict[str, dict[str, Arity | str]] = {}
    behavior = source + "\n" + "\n".join(helper_sources)

    for field in re.finditer(r"\b([A-Za-z_]\w*)\s*:\s*\{", schema):
        field_opening = schema.find("{", field.start())
        field_closing = _matching_delimiter(schema, field_opening, "{", "}", "xsh")
        declaration = schema[field_opening + 1 : field_closing]
        form = re.search(r'\bform\s*:\s*"([^\"]*)"', declaration)
        if form:
            field_name = field.group(1)
            field_use = rf"\bopts\.{re.escape(field_name)}\b"
            manual_uses = MANUAL_RAW_OPTION_USES.get(utility, {})
            for spelling, arity in _form_entries(form.group(1), field_name):
                previous = declarations.get(spelling)
                if previous is not None and previous["arity"] != arity:
                    raise SurfaceParseError(f"{spelling} has conflicting argument counts")
                manual_use = manual_uses.get(spelling)
                if manual_use is not None and manual_use not in source:
                    raise SurfaceParseError(f"manual implementation of {spelling} is not recognized")
                disposition = (
                    "implemented"
                    if re.search(field_use, behavior) or manual_use is not None
                    else "parsed-but-unused"
                )
                declarations[spelling] = {
                    "arity": arity,
                    "field": field_name,
                    "disposition": disposition,
                }

    manual = MANUAL_XSH_OPTIONS.get(utility)
    if manual:
        manual_source = source + "\n" + "\n".join(helper_sources)
        normalized_source = re.sub(r"\s+", "", manual_source)
        if not all(
            re.sub(r"\s+", "", marker) in normalized_source
            for marker in manual["markers"]
        ):
            raise SurfaceParseError(f"manual {utility} option scan is not recognized")
        for spelling, arity in manual["spellings"].items():
            previous = declarations.get(spelling)
            if previous is not None and previous["arity"] != arity:
                raise SurfaceParseError(f"{spelling} has conflicting argument counts")
            declarations[spelling] = {
                "arity": arity,
                "field": "manual",
                "disposition": "implemented",
            }

    return declarations


def parse_xsh_source(source: str, utility: str | None = None) -> dict[str, Arity]:
    return {
        spelling: declaration["arity"]
        for spelling, declaration in parse_xsh_declarations(source, utility).items()
    }


def compare(reference: dict[str, Arity], xsh: dict[str, Arity]) -> dict[str, dict]:
    shared = reference.keys() & xsh.keys()
    return {
        "uutils_only": {key: reference[key] for key in sorted(reference.keys() - xsh.keys())},
        "xsh_only": {key: xsh[key] for key in sorted(xsh.keys() - reference.keys())},
        "arity_mismatches": {
            key: (reference[key], xsh[key])
            for key in sorted(shared)
            if reference[key] != xsh[key]
        },
    }


def _uutils_root() -> tuple[Path, str]:
    lock = json.loads(LOCK.read_text())
    wanted = lock["uutils"]["commit"]
    root = Path(os.environ.get("UUTILS_ROOT", REPO.parent / "ref" / "uutils-coreutils"))
    try:
        actual = subprocess.check_output(
            ["git", "-C", str(root), "rev-parse", "HEAD"], text=True, stderr=subprocess.PIPE
        ).strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise SurfaceParseError(f"cannot read uutils checkout at {root}: {error}") from error
    if actual != wanted:
        raise SurfaceParseError(
            f"uutils checkout at {actual}, but upstream.lock.json pins {wanted}"
        )
    return root, wanted


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--util", choices=SUPPORTED_UTILITIES, required=True)
    args = parser.parse_args()

    try:
        root, revision = _uutils_root()
        uutils_source, xsh_source = SOURCE_PATHS[args.util]
        behavior_source = "\n".join(
            (root / "src/uu" / path).read_text()
            for path in UUTILS_BEHAVIOR_PATHS.get(args.util, ())
        )
        argument_sources = tuple(
            (root / "src" / path).read_text()
            for path in UUTILS_ARGUMENT_PATHS.get(args.util, ())
        )
        uutils_declarations = parse_uutils_declarations(
            (root / "src/uu" / uutils_source).read_text(),
            args.util,
            behavior_source,
            argument_sources,
            UUTILS_SHARED_COMMAND_BUILDERS.get(args.util),
        )
        xsh_declarations = parse_xsh_declarations(
            (REPO / "core" / xsh_source).read_text(),
            args.util,
            tuple(
                (REPO / "core" / path).read_text()
                for path in MANUAL_XSH_OPTION_PATHS.get(args.util, ())
            ),
        )
    except (OSError, SurfaceParseError) as error:
        print(f"option surface check: {error}", file=sys.stderr)
        return 2

    reference = {
        spelling: declaration["arity"]
        for spelling, declaration in uutils_declarations.items()
    }
    xsh = {
        spelling: declaration["arity"]
        for spelling, declaration in xsh_declarations.items()
    }
    result = compare(reference, xsh)
    print(
        f"{args.util}: pinned uutils {revision[:12]} has {len(reference)} option spellings; "
        f"XSH has {len(xsh)}"
    )
    for name, declarations in (("uutils", uutils_declarations), ("XSH", xsh_declarations)):
        unused = {
            spelling: declaration["field"]
            for spelling, declaration in declarations.items()
            if declaration["disposition"] == "parsed-but-unused"
        }
        if unused:
            print(f"{name} parsed-but-unused spellings:")
            for spelling, field in sorted(unused.items()):
                print(f"  {spelling}: field {field}")
    for key, title in (
        ("uutils_only", "uutils-only spellings"),
        ("xsh_only", "XSH-only spellings"),
        ("arity_mismatches", "argument-count mismatches"),
    ):
        values = result[key]
        if values:
            print(f"{title}:")
            for spelling, detail in values.items():
                print(f"  {spelling}: {detail}")
    if any(result.values()):
        return 1
    print("option surface matches")
    return 0


if __name__ == "__main__":
    sys.exit(main())
