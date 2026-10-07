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

    def test_xsh_forms_capture_aliases_and_argument_arity(self):
        source = '''
proc main(...argv: List[Str]) {
  let opts = cli.applet(argv, {
    gnu: {status: 1},
    all: {form: "-A --all", default: false},
    output: {form: "-o --output FILE"},
    help: {form: "--help", default: false},
  })?
}
'''
        self.assertEqual(
            surface.parse_xsh_source(source),
            {"-A": 0, "--all": 0, "-o": 1, "--output": 1, "--help": 0},
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

    def test_unknown_uutils_option_action_fails_closed(self):
        source = '''
pub fn uu_app() -> Command {
    Command::new("cat")
        .arg(Arg::new("color").short('c').action(ArgAction::Append))
}
'''
        self.assertRaises(surface.SurfaceParseError, surface.parse_uutils_source, source)


if __name__ == "__main__":
    unittest.main()
