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


if __name__ == "__main__":
    unittest.main()
