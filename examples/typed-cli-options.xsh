type BuildOptions = {root: Path, jobs: Int, define: List[Str], verbose: Bool}

type Cli = {command: Str, action: Str, root: Path, raw: List[Str]}

let opts: BuildOptions = cli.parse(
  args,
  {
    root: {form: "--root PATH", default: p"dest"},
    jobs: {form: "-j --jobs N", default: cpu.count()},
    define: {form: "-D --define NAME=VALUE", repeated: true},
    verbose: {form: "-v --verbose", default: false},
  },
)?

let line = "WARN build.rs: unused value"
let word_re = regex.compile("unused|missing")?
let capture_re = regex.compile("^(\\w+) ([^:]+): (.*)$")?
let whitespace_re = regex.compile("\\s+")?
let warn_re = regex.compile("WARN.*unused")?
let matches = word_re.find(line)
let captures = capture_re.captures(line)
let rewritten = whitespace_re.replace(line, "|")

let command_specs = {
  build: {positionals: ["root"], types: {root: "Path"}, rest: "raw"},
  clean: {positionals: ["root"], types: {root: "Path"}, rest: "raw"},
}

let parsed_cli: Cli = cli.commands(
  ["deploy", "target/demo", "--dry-run"],
  rootless_default: "build",
  commands: command_specs,
  fallback_command: {positionals: ["action", "root"], types: {root: "Path"}, rest: "raw", command_like: true},
)?

print $opts.root.name $opts.jobs opts.define.len() $opts.verbose
print warn_re.matches(line) captures[1] captures[2] matches[0].text $rewritten
print $parsed_cli.command $parsed_cli.root.name parsed_cli.raw.len() $parsed_cli.action
