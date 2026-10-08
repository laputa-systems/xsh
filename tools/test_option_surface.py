import unittest

from dev.compat import check_option_surface as surface


class OptionSurfaceTests(unittest.TestCase):
    def test_uutils_cat_includes_declared_and_generated_options(self):
        source = '''
mod options {
    pub static ALL: &str = "all";
}

pub fn uu_app() -> Command {
    Command::new("cat")
        .version("1.0")
        .arg(
            Arg::new(options::ALL)
                .short('A')
                .long(options::ALL)
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("file")
                .action(ArgAction::Append),
        )
}

fn run(matches: &ArgMatches) {
    matches.get_flag(options::ALL);
}
'''
        self.assertEqual(
            surface.parse_uutils_source(source),
            {"-A": 0, "--all": 0, "-h": 0, "--help": 0, "-V": 0, "--version": 0},
        )
        declaration = surface.parse_uutils_declarations(source)["-A"]
        self.assertEqual(declaration["disposition"], "implemented")

    def test_uutils_resolves_top_level_constants_and_long_aliases(self):
        source = '''
const KERNEL_NAME: &str = "kernel-name";

pub fn uu_app() -> Command {
    Command::new("uname")
        .arg(
            Arg::new(KERNEL_NAME)
                .short('s')
                .long(KERNEL_NAME)
                .alias("sysname")
                .action(ArgAction::SetTrue),
        )
}

fn run(matches: &ArgMatches) {
    matches.get_flag(KERNEL_NAME);
}
'''
        self.assertEqual(
            surface.parse_uutils_source(source),
            {"-s": 0, "--kernel-name": 0, "--sysname": 0, "-h": 0, "--help": 0},
        )

    def test_uutils_value_options_report_fixed_arity(self):
        source = '''
mod options {
    pub static FILES0_FROM: &str = "files0-from";
    pub static PAIR: &str = "pair";
}

pub fn uu_app() -> Command {
    Command::new("wc")
        .arg(Arg::new(options::FILES0_FROM).long(options::FILES0_FROM))
        .arg(
            Arg::new(options::PAIR)
                .long(options::PAIR)
                .action(ArgAction::Set)
                .num_args(2),
        )
}
'''
        self.assertEqual(
            surface.parse_uutils_source(source),
            {"--files0-from": 1, "--pair": 2, "-h": 0, "--help": 0},
        )

    def test_uutils_explicit_help_and_version_actions_do_not_add_short_forms(self):
        source = '''
pub fn uu_app() -> Command {
    Command::new("true")
        .version("1.0")
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(Arg::new("help").long("help").action(ArgAction::Help))
        .arg(Arg::new("version").long("version").action(ArgAction::Version))
}
'''
        self.assertEqual(surface.parse_uutils_source(source), {"--help": 0, "--version": 0})

    def test_xsh_forms_capture_aliases_and_argument_arity(self):
        source = '''
proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {status: 1},
    all: {form: "-A --all", default: false},
    output: {form: "-o --output FILE"},
    pair: {form: "-p --pair FIRST SECOND"},
    help: {form: "--help", default: false},
  })?
}
'''
        self.assertEqual(
            surface.parse_xsh_source(source),
            {"-A": 0, "--all": 0, "-o": 1, "--output": 1, "-p": 2, "--pair": 2, "--help": 0},
        )

    def test_cat_unbuffered_option_is_declared_but_not_consumed(self):
        source = (surface.REPO / "core/cat.xsh").read_text()
        declaration = surface.parse_xsh_declarations(source)["-u"]
        self.assertEqual(declaration["field"], "unbuffered")
        self.assertEqual(declaration["disposition"], "parsed-but-unused")

    def test_uutils_named_noop_option_is_parsed_but_not_consumed(self):
        source = '''
mod options {
    pub static IGNORED_U: &str = "ignored-u";
}

pub fn uu_app() -> Command {
    Command::new("cat")
        .arg(Arg::new(options::IGNORED_U).short('u').action(ArgAction::SetTrue))
}
'''
        declaration = surface.parse_uutils_declarations(source)["-u"]
        self.assertEqual(declaration["field"], "IGNORED_U")
        self.assertEqual(declaration["disposition"], "parsed-but-unused")

    def test_comparison_reports_missing_extra_and_arity_changes(self):
        result = surface.compare(
            {"-h": 0, "--count": 1},
            {"--count": 0, "--extra": 0},
        )
        self.assertEqual(result["uutils_only"], {"-h": 0})
        self.assertEqual(result["xsh_only"], {"--extra": 0})
        self.assertEqual(result["arity_mismatches"], {"--count": (1, 0)})

    def test_optional_uutils_value_arity_fails_closed(self):
        source = '''
pub fn uu_app() -> Command {
    Command::new("demo")
        .arg(Arg::new("color").short('c').action(ArgAction::Set).num_args(0..=1))
}
'''
        self.assertRaises(surface.SurfaceParseError, surface.parse_uutils_source, source)

    def test_optional_xsh_value_arity_fails_closed(self):
        source = '''
proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {format: {form: "--format[=STYLE]"}})?
}
'''
        self.assertRaises(surface.SurfaceParseError, surface.parse_xsh_source, source)

    def test_manual_xsh_true_and_false_help_version_are_declared(self):
        source = '''
proc main(...argv: List[Str]) {
  return when argv.len() != 1
  if argv[0] == "--help" { print "help" }
  if argv[0] == "--version" { print "version" }
}
'''
        for utility in ("true", "false"):
            with self.subTest(utility=utility):
                declarations = surface.parse_xsh_declarations(source, utility)
                self.assertEqual(set(declarations), {"--help", "--version"})

    def test_sum_is_registered_with_its_xsh_option_schema(self):
        self.assertIn("sum", surface.SUPPORTED_UTILITIES)
        _uutils_source, xsh_source = surface.SOURCE_PATHS["sum"]
        declarations = surface.parse_xsh_declarations(
            (surface.REPO / "core" / xsh_source).read_text(), "sum"
        )
        self.assertEqual(
            {spelling: int(entry["arity"]) for spelling, entry in declarations.items()},
            {"-r": 0, "-s": 0, "--sysv": 0, "--help": 0, "--version": 0},
        )
        self.assertTrue(
            all(
                entry["disposition"] == "implemented"
                for spelling, entry in declarations.items()
                if spelling != "-r"
            )
        )
        self.assertEqual(declarations["-r"]["disposition"], "parsed-but-unused")

    def test_seq_registers_its_option_schema_and_help_difference(self):
        self.assertIn("seq", surface.SUPPORTED_UTILITIES)
        uutils_source, xsh_source = surface.SOURCE_PATHS["seq"]
        uutils_root = surface.REPO.parent / "ref" / "uutils-coreutils"
        uutils = surface.parse_uutils_declarations(
            (uutils_root / "src" / "uu" / uutils_source).read_text()
        )
        xsh = surface.parse_xsh_declarations(
            (surface.REPO / "core" / xsh_source).read_text(), "seq"
        )
        self.assertEqual(
            {spelling: int(entry["arity"]) for spelling, entry in xsh.items()},
            {
                "-f": 1,
                "--format": 1,
                "-s": 1,
                "--separator": 1,
                "-t": 1,
                "--terminator": 1,
                "-w": 0,
                "--equal-width": 0,
                "--help": 0,
                "--version": 0,
            },
        )
        self.assertEqual(
            surface.compare(
                {spelling: int(entry["arity"]) for spelling, entry in uutils.items()},
                {spelling: int(entry["arity"]) for spelling, entry in xsh.items()},
            ),
            {"uutils_only": {"-V": 0, "-h": 0}, "xsh_only": {}, "arity_mismatches": {}},
        )

    def test_text_wrapping_utilities_register_their_option_differences(self):
        uutils_root = surface.REPO.parent / "ref" / "uutils-coreutils"
        expected_xsh = {
            "fold": {
                "-w": 1,
                "--width": 1,
                "-b": 0,
                "--bytes": 0,
                "-c": 0,
                "--characters": 0,
                "-s": 0,
                "--spaces": 0,
                "--help": 0,
                "--version": 0,
            },
            "expand": {
                "-i": 0,
                "--initial": 0,
                "-t": 1,
                "--tabs": 1,
                "--help": 0,
                "--version": 0,
            },
            "unexpand": {
                "-a": 0,
                "--all": 0,
                "-f": 0,
                "--first-only": 0,
                "-t": 1,
                "--tabs": 1,
                "--help": 0,
                "--version": 0,
            },
        }
        expected_uutils_only = {
            "fold": {"-h": 0, "-V": 0},
            "expand": {"-h": 0, "-U": 0, "-V": 0, "--no-utf8": 0},
            "unexpand": {"-h": 0, "-U": 0, "-V": 0, "--no-utf8": 0},
        }

        for utility, xsh_options in expected_xsh.items():
            with self.subTest(utility=utility):
                self.assertIn(utility, surface.SUPPORTED_UTILITIES)
                uutils_source, xsh_source = surface.SOURCE_PATHS[utility]
                uutils = surface.parse_uutils_declarations(
                    (uutils_root / "src" / "uu" / uutils_source).read_text()
                )
                xsh = surface.parse_xsh_declarations(
                    (surface.REPO / "core" / xsh_source).read_text(), utility
                )
                xsh_options_actual = {
                    spelling: int(entry["arity"]) for spelling, entry in xsh.items()
                }
                self.assertEqual(xsh_options_actual, xsh_options)
                self.assertEqual(
                    surface.compare(
                        {spelling: int(entry["arity"]) for spelling, entry in uutils.items()},
                        xsh_options_actual,
                    ),
                    {
                        "uutils_only": expected_uutils_only[utility],
                        "xsh_only": {},
                        "arity_mismatches": {},
                    },
                )

    def test_additional_registered_option_surfaces_match_pinned_sources(self):
        utilities = (
            "arch",
            "chroot",
            "comm",
            "factor",
            "groups",
            "hostname",
            "join",
            "link",
            "more",
            "nice",
            "nl",
            "nohup",
            "paste",
            "pathchk",
            "pinky",
            "ptx",
            "readlink",
            "realpath",
            "rmdir",
            "stat",
            "sync",
            "tac",
            "timeout",
            "truncate",
            "tsort",
            "unlink",
            "uptime",
            "users",
        )
        uutils_only = {
            utility: {"-h": 0, "-V": 0}
            for utility in utilities
            if utility not in {"factor", "nl", "pinky", "tsort"}
        }
        uutils_only.update(
            {"factor": {"-V": 0}, "nl": {"-V": 0}, "pinky": {"-V": 0}, "tsort": {}}
        )
        uutils_root = surface.REPO.parent / "ref" / "uutils-coreutils"

        for utility in utilities:
            with self.subTest(utility=utility):
                self.assertIn(utility, surface.SUPPORTED_UTILITIES)
                uutils_source, xsh_source = surface.SOURCE_PATHS[utility]
                uutils = surface.parse_uutils_declarations(
                    (uutils_root / "src" / "uu" / uutils_source).read_text()
                )
                xsh = surface.parse_xsh_declarations(
                    (surface.REPO / "core" / xsh_source).read_text(), utility
                )
                self.assertEqual(
                    surface.compare(
                        {spelling: int(entry["arity"]) for spelling, entry in uutils.items()},
                        {spelling: int(entry["arity"]) for spelling, entry in xsh.items()},
                    ),
                    {
                        "uutils_only": uutils_only[utility],
                        "xsh_only": {},
                        "arity_mismatches": {},
                    },
                )

    def test_registered_surfaces_capture_additional_uutils_options(self):
        expected_uutils_only = {
            "fmt": {
                "--preserve-headers": 0,
                "--tab-width": 1,
                "-T": 1,
                "-V": 0,
                "-h": 0,
                "-m": 0,
            },
            "id": {
                "--ignore": 0,
                "-A": 0,
                "-P": 0,
                "-V": 0,
                "-h": 0,
                "-p": 0,
            },
            "kill": {"-L": 0, "-V": 0, "-h": 0},
            "mkdir": {"--context": 1, "-V": 0, "-Z": 0, "-h": 0},
            "shuf": {"--random-seed": 1, "-V": 0, "-h": 0},
        }
        uutils_root = surface.REPO.parent / "ref" / "uutils-coreutils"

        for utility, expected in expected_uutils_only.items():
            with self.subTest(utility=utility):
                self.assertIn(utility, surface.SUPPORTED_UTILITIES)
                uutils_source, xsh_source = surface.SOURCE_PATHS[utility]
                uutils = surface.parse_uutils_declarations(
                    (uutils_root / "src" / "uu" / uutils_source).read_text()
                )
                xsh = surface.parse_xsh_declarations(
                    (surface.REPO / "core" / xsh_source).read_text(), utility
                )
                self.assertEqual(
                    surface.compare(
                        {spelling: int(entry["arity"]) for spelling, entry in uutils.items()},
                        {spelling: int(entry["arity"]) for spelling, entry in xsh.items()},
                    ),
                    {
                        "uutils_only": expected,
                        "xsh_only": {},
                        "arity_mismatches": {},
                    },
                )


if __name__ == "__main__":
    unittest.main()
