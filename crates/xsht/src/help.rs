use crate::xsht::commands::{COMMANDS, CommandSpec, OptionSpec, find};
use std::fmt::Write as _;

pub(crate) fn root_help() -> String {
    let mut help = String::new();
    writeln!(help, "xsht {}", env!("CARGO_PKG_VERSION")).expect("write help heading");
    help.push_str(
        r#"
Usage:
  xsht <COMMAND> [OPTIONS]
  xsht -h | --help
  xsht <COMMAND> -h | --help
  xsht help [COMMAND]

Start here:
"#,
    );

    for command in COMMANDS {
        writeln!(
            help,
            "  {:<18} xsht {} {}",
            command.quick_label, command.name, command.args
        )
        .expect("write help quick start");
    }

    help.push_str("\nCommand reference:\n\n");
    for (index, command) in COMMANDS.iter().enumerate() {
        render_command(&mut help, command, false);
        if index + 1 < COMMANDS.len() {
            help.push('\n');
        }
    }

    help.push_str(
        r#"
Common workflows:

  xsht check .
  xsht fmt --check .
  xsht lint --fix .
  xsht test --cov
"#,
    );
    help
}

pub(crate) fn command_help(name: &str) -> Option<String> {
    let command = find(name)?;
    let mut help = String::new();
    render_command(&mut help, command, true);
    Some(help)
}

fn render_command(help: &mut String, command: &CommandSpec, standalone: bool) {
    if standalone {
        writeln!(
            help,
            "xsht {} — {}\n\nUsage:",
            command.name, command.summary
        )
        .expect("write help command heading");
    } else {
        writeln!(help, "{} — {}", command.name, command.summary)
            .expect("write help command heading");
    }

    for usage in command.usage_lines() {
        writeln!(help, "  {usage}").expect("write help usage");
    }

    if !command.notes.is_empty() {
        help.push('\n');
        for note in command.notes {
            writeln!(help, "  {note}").expect("write help note");
        }
    }

    if !command.options.is_empty() {
        help.push('\n');
        render_options(help, command.options);
    }

    if !command.examples.is_empty() {
        help.push_str("\n  Examples:\n");
        for example in command.examples {
            writeln!(help, "    {example}").expect("write help example");
        }
    }
}

fn render_options(help: &mut String, options: &[OptionSpec]) {
    let syntaxes: Vec<String> = options.iter().map(OptionSpec::syntax).collect();
    let width = syntaxes.iter().map(String::len).max().unwrap_or(0);
    for (option, syntax) in options.iter().zip(&syntaxes) {
        writeln!(help, "  {syntax:width$}  {}", option.description).expect("write help option");
    }
}
