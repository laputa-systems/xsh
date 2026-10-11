#![allow(clippy::single_call_fn)]

use super::{RedirectionKind as ShellRedirectionKind, SimpleCommand};
use crate::xshi::interactive::app::xsh_word;

/// A readable rendering of the commands that ran, for job notices and history.
pub(crate) fn shell_line_source(commands: &[SimpleCommand]) -> String {
    let mut out = String::new();
    for (index, command) in commands.iter().enumerate() {
        if index > 0 {
            out.push_str(" | ");
        }
        if index == 0 {
            out.push_str("run.status ");
        } else {
            out.push_str("run ");
        }
        append_command_source(command, &mut out);
    }
    out
}

fn append_command_source(command: &SimpleCommand, out: &mut String) {
    for (index, word) in command.words.iter().enumerate() {
        if index > 0 {
            out.push(' ');
        }
        out.push_str(&xsh_word(&word.text()));
    }
    for redirection in &command.redirections {
        out.push(' ');
        out.push_str(match redirection.kind {
            ShellRedirectionKind::Stdin => "<",
            ShellRedirectionKind::StdoutWrite => ">",
            ShellRedirectionKind::StdoutAppend => ">>",
            ShellRedirectionKind::StderrWrite => "2>",
            ShellRedirectionKind::StderrAppend => "2>>",
            ShellRedirectionKind::StdoutToStderr => ">&",
            ShellRedirectionKind::StderrToStdout => "2>&",
            ShellRedirectionKind::BothWrite => "&>",
            ShellRedirectionKind::BothAppend => "&>>",
        });
        out.push(' ');
        out.push_str(&xsh_word(&redirection.target.text()));
    }
}
