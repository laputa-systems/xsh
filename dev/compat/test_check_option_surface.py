import unittest

from check_option_surface import (
    SurfaceParseError,
    compare,
    parse_uutils_declarations,
    parse_uutils_source,
    parse_xsh_declarations,
    parse_xsh_source,
)


class VisibleAliasParsingTests(unittest.TestCase):
    def test_visible_short_alias_and_alias_list_are_declared_options(self):
        source = '''\
pub fn uu_app() {
    Command::new("who")
        // .arg(Arg::new(options::STALE).long("stale"))
        .arg(
            Arg::new(options::MESG)
                .short('T')
                .long("mesg")
                .visible_short_alias('w')
                .visible_aliases(["message", "writable"])
                .action(ArgAction::SetTrue),
        )
}
'''

        declarations = parse_uutils_declarations(source, "who")

        self.assertEqual(int(declarations["-T"]["arity"]), 0)
        self.assertEqual(int(declarations["-w"]["arity"]), 0)
        self.assertEqual(int(declarations["--mesg"]["arity"]), 0)
        self.assertEqual(int(declarations["--message"]["arity"]), 0)
        self.assertEqual(int(declarations["--writable"]["arity"]), 0)
        self.assertNotIn("--stale", declarations)

    def test_behavior_in_a_related_source_marks_the_option_as_used(self):
        source = '''\
pub fn uu_app() {
    Command::new("who")
        .arg(Arg::new(options::MESG).short('T').visible_short_alias('w'))
}
'''

        declarations = parse_uutils_declarations(
            source, "who", "matches.get_flag(options::MESG)"
        )

        self.assertEqual(declarations["-w"]["disposition"], "implemented")

    def test_nested_option_constant_resolves_long_spelling(self):
        source = '''\
mod options {
    pub mod verbosity {
        pub const QUIET: &str = "quiet";
    }
}
pub fn uu_app() {
    Command::new("tail")
        .arg(Arg::new(options::verbosity::QUIET)
            .long(options::verbosity::QUIET)
            .action(ArgAction::SetTrue))
}
'''

        declarations = parse_uutils_declarations(source, "tail")

        self.assertEqual(declarations["--quiet"]["arity"], 0)

    def test_help_long_action_is_a_zero_argument_help_option(self):
        source = '''\
pub fn uu_app() {
    Command::new("tee")
        .disable_help_flag(true)
        .arg(Arg::new("--help")
            .short('h')
            .long("help")
            .action(ArgAction::HelpLong))
}
'''

        declarations = parse_uutils_declarations(source, "tee")

        self.assertEqual(declarations["-h"]["arity"], 0)
        self.assertEqual(declarations["--help"]["disposition"], "implemented")

    def test_some_wrapped_short_option_is_read(self):
        source = '''\
pub fn uu_app() {
    Command::new("stdbuf")
        .arg(Arg::new("input").short(Some('i')).long("input"))
}
'''

        declarations = parse_uutils_declarations(source, "stdbuf")

        self.assertEqual(declarations["-i"]["arity"], 1)

    def test_short_option_character_constant_is_read(self):
        source = '''\
mod options {
    pub const INPUT_SHORT: char = 'i';
}
pub fn uu_app() {
    Command::new("stdbuf")
        .arg(Arg::new(options::INPUT).short(options::INPUT_SHORT).long("input"))
}
'''

        declarations = parse_uutils_declarations(source, "stdbuf")

        self.assertEqual(declarations["-i"]["arity"], 1)

    def test_permission_common_arguments_are_included_from_helper(self):
        source = '''\
pub fn uu_app() {
    Command::new("chmod").args(uucore::perms::common_args())
}
'''
        helper = '''\
pub fn common_args() -> Vec<Arg> {
    vec![
        Arg::new(traverse::TRAVERSE).short(traverse::TRAVERSE.chars().next().unwrap()).action(ArgAction::SetTrue),
        Arg::new(traverse::EVERY).short(traverse::EVERY.chars().next().unwrap()).action(ArgAction::SetTrue),
        Arg::new(traverse::NO_TRAVERSE).short(traverse::NO_TRAVERSE.chars().next().unwrap()).action(ArgAction::SetTrue),
        Arg::new(options::dereference::DEREFERENCE).long(options::dereference::DEREFERENCE).action(ArgAction::SetTrue),
        Arg::new(options::dereference::NO_DEREFERENCE).short('h').long(options::dereference::NO_DEREFERENCE).action(ArgAction::SetTrue),
    ]
}
'''

        declarations = parse_uutils_declarations(
            source, "chmod", argument_sources=(helper,)
        )

        self.assertEqual(declarations["-H"]["arity"], 0)
        self.assertEqual(declarations["-L"]["arity"], 0)
        self.assertEqual(declarations["-P"]["arity"], 0)
        self.assertEqual(declarations["--dereference"]["arity"], 0)
        self.assertEqual(declarations["--no-dereference"]["arity"], 0)

    def test_permission_shared_parser_options_are_included(self):
        source = '''\
proc main(...argv: List[Bytes]) {
    let opts = perm.options(argv)?
}
'''
        helper = '''\
let known = ["--recursive", "--silent", "--quiet", "--verbose", "--changes", "--dereference", "--no-dereference", "--preserve-root", "--no-preserve-root", "--reference", "--help", "--version"]
if word == "--help" { return }
if word == "--version" { return }
} else if word == "--recursive" { recursive = true }
} else if word == "--silent" or word == "--quiet" { quiet = true }
} else if word == "--verbose" { verbosity = "verbose" }
} else if word == "--changes" { verbosity = "changes" }
} else if word == "--dereference" { dereference = true }
} else if word == "--no-dereference" { dereference = false }
} else if word == "--preserve-root" { preserve_root = true }
} else if word == "--no-preserve-root" { preserve_root = false }
} else if word == "--reference" { reference = value }
} else if ! chmod and (word == "--from" or word.starts_with("--from=")) { from = value }
match ch { "R" => recursive = true, "f" => quiet = true, "v" => verbosity = "verbose", "c" => verbosity = "changes", "H" => traversal = "H", "L" => traversal = "L", "P" => traversal = "P", "h" => { dereference = false; explicit_dereference = true } }
'''

        declarations = parse_xsh_declarations(source, "chown", (helper,))

        self.assertEqual(declarations["--reference"]["arity"], 1)
        self.assertEqual(declarations["--from"]["arity"], 1)
        self.assertEqual(declarations["-R"]["arity"], 0)
        self.assertEqual(declarations["-h"]["arity"], 0)

    def test_empty_visible_alias_list_is_valid(self):
        source = '''\
pub fn uu_app() {
    Command::new("who")
        .arg(Arg::new(options::MESG).long("mesg").visible_aliases([]))
}
'''

        declarations = parse_uutils_declarations(source, "who")

        self.assertIn("--mesg", declarations)

    def test_option_alias_constants_and_value_aliases_are_distinguished(self):
        source = '''\
mod options {
    pub const PRESUME_INPUT_TTY: &str = "-presume-input-tty";
}
pub fn uu_app() {
    Command::new("rm")
        .arg(
            Arg::new(options::PRESUME_INPUT_TTY)
                .long("presume-input-tty")
                .alias(options::PRESUME_INPUT_TTY)
                .value_parser([PossibleValue::new("always").alias("yes")]),
        )
}
'''

        declarations = parse_uutils_declarations(source, "rm")

        self.assertIn("--presume-input-tty", declarations)
        self.assertIn("---presume-input-tty", declarations)
        self.assertNotIn("--yes", declarations)

    def test_optional_and_variable_arity_ranges_are_preserved(self):
        uutils_source = '''\
pub fn uu_app() {
    Command::new("du")
        .arg(Arg::new(options::TIME).long("time").num_args(0..))
        .arg(Arg::new(options::STYLE).long("time-style").num_args(0..=1))
        .arg(Arg::new(options::LIMIT).long("limit").num_args(1..=2))
        .arg(Arg::new(options::EXCLUSIVE).long("exclusive").num_args(1..2))
        .arg(Arg::new(options::FROM_START).long("from-start").num_args(..=1))
}
'''
        xsh_source = '''\
let opts = cli.applet(argv, {
    time: {form: "--time[=WORD]"},
    time_style: {form: "--time-style[=STYLE]"},
    limit: {form: "--limit FIRST SECOND"},
})?
'''

        uutils = parse_uutils_source(uutils_source)
        xsh = parse_xsh_source(xsh_source)

        self.assertEqual(uutils["--time"], (0, None))
        self.assertEqual(uutils["--time-style"], (0, 1))
        self.assertEqual(uutils["--limit"], (1, 2))
        self.assertEqual(uutils["--exclusive"], 1)
        self.assertEqual(uutils["--from-start"], (0, 1))
        self.assertEqual(xsh["--time"], (0, 1))
        self.assertEqual(xsh["--time-style"], (0, 1))
        self.assertEqual(xsh["--limit"], 2)
        difference = compare(uutils, xsh)
        self.assertEqual(
            difference["arity_mismatches"],
            {"--limit": ((1, 2), 2), "--time": ((0, None), (0, 1))},
        )

    def test_referenced_shared_argument_builders_are_read(self):
        source = '''\
pub fn uu_app() {
    Command::new("install")
        // .arg(backup_control::arguments::missing())
        .arg(backup_control::arguments::backup())
}
backup_control::determine_backup_mode(&matches);
'''
        helper = '''\
pub static OPT_BACKUP: &str = "backupopt_backup";
pub fn backup() -> clap::Arg {
    clap::Arg::new(OPT_BACKUP)
        .long("backup")
        .num_args(0..=1)
}
'''

        declarations = parse_uutils_declarations(
            source, "install", argument_sources=(helper,)
        )

        self.assertEqual(declarations["--backup"]["arity"], (0, 1))
        self.assertEqual(declarations["--backup"]["disposition"], "implemented")

    def test_additional_source_constants_resolve_cli_spellings(self):
        source = '''\
pub fn uu_app() {
    Command::new("numfmt")
        .arg(Arg::new(DEBUG).long(DEBUG).action(ArgAction::SetTrue))
}
'''
        options = '''\
pub const DEBUG: &str = "debug";
'''

        declarations = parse_uutils_declarations(
            source, "numfmt", argument_sources=(options,)
        )

        self.assertEqual(declarations["--debug"]["arity"], 0)

    def test_manual_od_format_options_are_included(self):
        source = '''\
proc main(...argv: List[Str]) {
    if "abcdDfFhHiIlLOosxX".find(char) != null { requested += ["x"] }
    if arg.starts_with("--format=") { requested += ["x"] }
    if arg == "--format" { requested += ["x"] }
    if arg == "-t" { requested += ["x"] }
    let opts = cli.applet(args, { format: {form: "--format TYPE"} })?
}
'''

        declarations = parse_xsh_declarations(source, "od")

        self.assertEqual(declarations["-a"]["arity"], 0)
        self.assertEqual(declarations["-h"]["arity"], 0)
        self.assertEqual(declarations["-t"]["arity"], 1)
        self.assertEqual(declarations["--format"]["arity"], 1)

    def test_manual_split_io_block_size_option_is_included(self):
        source = '''\
proc modernize(argv: List[Str]) {
    if item == "---io-blksize" { pending = true }
    if item.starts_with("---io-blksize=") { blksize = item.byte_slice(14) }
    let opts = cli.applet(args, { unbuffered: {form: "-u --unbuffered"} })?
}
'''

        declarations = parse_xsh_declarations(source, "split")

        self.assertEqual(declarations["---io-blksize"]["arity"], 1)

    def test_sort_mode_helper_declarations_are_included(self):
        source = '''\
pub fn uu_app() {
    Command::new("sort")
        .arg(make_sort_mode_arg(options::modes::HUMAN_NUMERIC, 'h', help))
        .arg(make_sort_mode_arg(options::modes::MONTH, 'M', help))
        .arg(make_sort_mode_arg(options::modes::NUMERIC, 'n', help))
        .arg(make_sort_mode_arg(options::modes::GENERAL_NUMERIC, 'g', help))
        .arg(make_sort_mode_arg(options::modes::VERSION, 'V', help))
        .arg(make_sort_mode_arg(options::modes::RANDOM, 'R', help))
}
fn make_sort_mode_arg(mode: &'static str, short: char, help: String) -> Arg {
    Arg::new(mode).short(short).long(mode).action(ArgAction::SetTrue)
}
'''

        declarations = parse_uutils_declarations(source, "sort")

        self.assertEqual(declarations["-h"]["arity"], 0)
        self.assertEqual(declarations["--human-numeric-sort"]["arity"], 0)
        self.assertEqual(declarations["-R"]["arity"], 0)
        self.assertEqual(declarations["--random-sort"]["arity"], 0)

    def test_shared_presume_pipe_filter_is_included_when_called_by_applet(self):
        source = '''\
proc main(...argv: List[Str]) {
    let opts = cli.applet(tio.without_presume_pipe(argv), {
        help: {form: "--help", stop: true},
    })?
}
'''
        helper = '''\
export pure without_presume_pipe(argv: List[Str]) {
    yield item unless options and item == "---presume-input-pipe"
}
'''

        declarations = parse_xsh_declarations(source, "head", (helper,))

        self.assertEqual(declarations["---presume-input-pipe"]["arity"], 0)

    def test_visible_alias_list_requires_literal_strings(self):
        source = '''\
pub fn uu_app() {
    Command::new("who")
        .arg(Arg::new(options::MESG).long("mesg").visible_aliases(ALIASES))
}
'''

        with self.assertRaisesRegex(SurfaceParseError, "visible alias is not a literal string"):
            parse_uutils_declarations(source, "who")

    def test_visible_short_alias_requires_a_literal_character(self):
        source = '''\
pub fn uu_app() {
    Command::new("who")
        .arg(Arg::new(options::MESG).short('m').visible_short_alias (ALIAS))
}
'''

        with self.assertRaisesRegex(SurfaceParseError, "visible short alias is not a literal character"):
            parse_uutils_declarations(source, "who")

    def test_shared_checksum_builder_methods_are_included(self):
        source = '''\
pub fn uu_app() {
    standalone_checksum_app().name("hash")
}
'''
        helpers = '''\
pub fn default_checksum_app() -> Command {
    Command::new("").version("1")
}
pub fn standalone_checksum_app() -> Command {
    default_checksum_app().with_binary().with_check_and_opts().with_tag(false)
}
mod options {
    pub const BINARY: &str = "binary";
    pub const CHECK: &str = "check";
    pub const TAG: &str = "tag";
}
impl ChecksumCommand for Command {
    fn with_binary(self) -> Self {
        self.arg(Arg::new(options::BINARY).long(options::BINARY).short('b').action(ArgAction::SetTrue))
    }
    fn with_check_and_opts(self) -> Self {
        self.arg(Arg::new(options::CHECK).long(options::CHECK).short('c').action(ArgAction::SetTrue))
    }
    fn with_tag(self, default: bool) -> Self {
        let mut arg = Arg::new(options::TAG).long(options::TAG).action(ArgAction::SetTrue);
        arg = if default { arg.help("default") } else { arg.help("tag") };
        self.arg(arg)
    }
}
'''

        declarations = parse_uutils_declarations(
            source,
            "md5sum",
            behavior_source="matches.get_flag(options::BINARY); matches.get_flag(options::CHECK); matches.get_flag(options::TAG);",
            argument_sources=(helpers,),
            shared_command_builder="standalone_checksum_app",
        )

        self.assertEqual(declarations["-b"]["arity"], 0)
        self.assertEqual(declarations["--check"]["disposition"], "implemented")
        self.assertEqual(declarations["--tag"]["disposition"], "implemented")
        self.assertIn("-V", declarations)

    def test_standalone_checksum_macro_uses_shared_command_builder(self):
        source = '''\
uu_checksum_common::declare_standalone!("sha256sum", AlgoKind::Sha256);
'''
        helpers = '''\
pub fn default_checksum_app() -> Command {
    Command::new("").version("1")
}
pub fn standalone_checksum_app() -> Command {
    default_checksum_app().with_binary()
}
mod options {
    pub const BINARY: &str = "binary";
}
impl ChecksumCommand for Command {
    fn with_binary(self) -> Self {
        self.arg(Arg::new(options::BINARY).long(options::BINARY).short('b').action(ArgAction::SetTrue))
    }
}
'''

        declarations = parse_uutils_declarations(
            source,
            "sha256sum",
            behavior_source="matches.get_flag(options::BINARY);",
            argument_sources=(helpers,),
            shared_command_builder="standalone_checksum_app",
        )

        self.assertEqual(declarations["--binary"]["disposition"], "implemented")
        self.assertIn("-V", declarations)

    def test_shared_xsh_checksum_schema_is_used_by_wrapper(self):
        wrapper = '''\
proc main(...argv: List[Str]) {
    checksums.execute(argv, "sha256")
}
'''
        helper = '''\
let parsed = cli.applet(argv, {
    binary: {form: "-b --binary", default: false},
})
let opts = parsed?
if opts.binary { run_binary() }
'''

        declarations = parse_xsh_declarations(wrapper, "sha256sum", (helper,))

        self.assertEqual(declarations["-b"]["arity"], 0)
        self.assertEqual(declarations["--binary"]["disposition"], "implemented")

    def test_shared_base_encoding_builder_methods_are_included(self):
        source = '''\
pub fn uu_app() {
    base_common::base_app(about, usage).name("base32")
}
'''
        helper = '''\
mod options {
    pub static DECODE: &str = "decode";
}
pub fn base_app() -> Command {
    Command::new("").version("1")
        .arg(Arg::new(options::DECODE).short('d').visible_short_alias('D').long(options::DECODE).action(ArgAction::SetTrue))
}
'''

        declarations = parse_uutils_declarations(
            source,
            "base32",
            behavior_source="matches.get_flag(options::DECODE)",
            argument_sources=(helper,),
            shared_command_builder="base_app",
        )

        self.assertEqual(declarations["-D"]["arity"], 0)
        self.assertEqual(declarations["--decode"]["disposition"], "implemented")
        self.assertIn("-V", declarations)

    def test_basenc_selectors_are_included_from_raw_scanner(self):
        wrapper = '''\
proc main(...argv: List[Str]) {
    bytes_enc.execute(argv, "")
}
'''
        helper = '''\
let selectors = ["--base64", "--base64url", "--base32", "--base32hex", "--base16", "--base2msbf", "--base2lsbf", "--z85", "--base58"]
if options and arg in selectors and default_kind == "" { kind = arg }
let parsed = cli.applet(args, { help: {form: "--help"} })
'''

        declarations = parse_xsh_declarations(wrapper, "basenc", (helper,))

        self.assertEqual(declarations["--base64"]["arity"], 0)
        self.assertEqual(declarations["--base58"]["arity"], 0)

    def test_manual_dircolors_parser_surface_includes_aliases(self):
        source = '''\
if ! operands and arg == "--help" { return }
if ! operands and arg == "--version" { return }
if ! operands and arg.starts_with("--") {
    match arg {
        "--sh" | "--bourne-shell" => shell = "b"
        "--csh" | "--c-shell" => shell = "c"
        "--print-database" => database = true
        "--print-ls-colors" => display = true
    }
} else if ! operands and arg.starts_with("-") and arg != "-" {
    for flag in arg.byte_slice(1) {
        match flag {
            "b" => shell = "b"
            "c" => shell = "c"
            "p" => database = true
        }
    }
}
'''

        declarations = parse_xsh_declarations(source, "dircolors")

        self.assertEqual(declarations["--bourne-shell"]["arity"], 0)
        self.assertEqual(declarations["-b"]["arity"], 0)
        self.assertEqual(declarations["--print-ls-colors"]["arity"], 0)
        self.assertEqual(declarations["--help"]["arity"], 0)

    def test_manual_expr_parser_surface_has_explicit_help_and_version(self):
        source = '''\
if argv.len() == 1 and argv[0] == b"--help" {
    gnu.help(USAGE)
}
if argv.len() == 1 and argv[0] == b"--version" {
    gnu.version("expr")
}
'''

        declarations = parse_xsh_declarations(source, "expr")

        self.assertEqual(declarations["--help"]["arity"], 0)
        self.assertEqual(declarations["--version"]["arity"], 0)

    def test_uutils_basenc_encoding_builder_is_included(self):
        source = '''\
pub fn uu_app() {
    base_common::base_app(about, usage).name("basenc")
}
fn get_encodings() {
    vec![
        ("base64", Format::Base64, help),
        ("base64url", Format::Base64Url, help),
        ("base32", Format::Base32, help),
        ("base32hex", Format::Base32Hex, help),
        ("base16", Format::Base16, help),
        ("base2lsbf", Format::Base2Lsbf, help),
        ("base2msbf", Format::Base2Msbf, help),
        ("z85", Format::Z85, help),
        ("base58", Format::Base58, help),
    ]
}
fn parse_cmd_args() {
    let raw_arg = Arg::new(encoding.0).long(encoding.0).action(ArgAction::SetTrue);
    command = command.arg(overriding_arg);
    matches.get_flag(encoding.0)
}
'''
        helper = '''\
pub fn base_app() -> Command {
    Command::new("").version("1")
}
'''

        declarations = parse_uutils_declarations(
            source,
            "basenc",
            argument_sources=(helper,),
            shared_command_builder="base_app",
        )

        self.assertEqual(declarations["--base32hex"]["disposition"], "implemented")
        self.assertEqual(declarations["--base58"]["arity"], 0)

    def test_date_manual_options_preserve_optional_and_required_values(self):
        source = '''\
if arg == "--help" { return }
if arg == "--version" { return }
if arg == "--debug" { debug = true }
if arg == "--resolution" { resolution = true }
if arg == "-u" or arg == "--utc" or arg == "--universal" or arg == "--uct" { utc = true }
if arg == "-R" or arg == "--rfc-email" or arg == "--rfc-822" or arg == "--rfc-2822" { format = "rfc" }
if arg.starts_with("-I") or arg.starts_with("--iso-8601") { parse_iso(arg) }
if arg.starts_with("--rfc-3339") { parse_rfc(arg) }
if arg == "-d" or arg == "--date" or arg.starts_with("--date=") { parse_date(arg) }
if arg == "-f" or arg == "--file" { parse_file(arg) }
if arg == "-r" or arg == "--reference" { parse_reference(arg) }
if arg == "-s" or arg == "--set" { parse_set(arg) }
'''

        declarations = parse_xsh_declarations(source, "date")

        self.assertEqual(declarations["-I"]["arity"], (0, 1))
        self.assertEqual(declarations["--rfc-3339"]["arity"], 1)
        self.assertEqual(declarations["--date"]["arity"], 1)


if __name__ == "__main__":
    unittest.main()
