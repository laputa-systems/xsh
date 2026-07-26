#![allow(clippy::single_call_fn)]

use super::{
    Redirection as ShellRedirection, RedirectionKind as ShellRedirectionKind, SimpleCommand,
};
use crate::xshi::interactive::app::{
    ExpansionError, expand_word, expand_word_to_string, parse_env_assignment, xsh_word,
};
use crate::xshi::interactive::session::Session;
use std::sync::Arc;
use xsh::source::Span;
use xsh::symbol::Name;
use xsh::syntax::arena::{
    ArenaCommandArg, ArenaEnvAssignmentValue, ArenaProgram, ArenaProgramBuilder,
    ArenaRedirectionTarget,
};
use xsh::syntax::node::{RedirectionKind, RunKind};

/// Lower an expanded shell pipeline into a single-statement arena program
/// containing a `run` command. Shell word/variable expansion happens here; the
/// resulting words are cooked literal arguments in the arena (they are not
/// present in any source text, so word parts are interned as cooked text).
pub(crate) fn lower_run_program(
    session: &Session,
    commands: &[SimpleCommand],
    first_kind: RunKind,
    span: Span,
) -> Result<ArenaProgram, ExpansionError> {
    let mut builder = ArenaProgramBuilder::with_token_capacity(0);
    let symbols = builder.symbol_owner().clone();
    symbols.with_current(|| {
        builder.begin_run_segments();
        for (index, command) in commands.iter().enumerate() {
            let kind = if index == 0 {
                first_kind
            } else {
                RunKind::Plain
            };
            lower_run_segment(session, command, kind, span, &mut builder)?;
        }
        let run = builder.finish_run_form(false, span);
        builder.push_run_command_statement(run, false, span);
        Ok(builder.finish())
    })
}

fn lower_run_segment(
    session: &Session,
    command: &SimpleCommand,
    kind: RunKind,
    span: Span,
    builder: &mut ArenaProgramBuilder<'_>,
) -> Result<(), ExpansionError> {
    let mut words = command.words.iter();
    builder.begin_env_assignments();
    while let Some(word) = words.clone().next() {
        let text = word.text();
        let Some((name, _value)) = parse_env_assignment(&text) else {
            break;
        };
        let expanded = expand_word_to_string(session, word)?;
        let Some((_, value)) = parse_env_assignment(&expanded) else {
            return Err(ExpansionError::usage(format!(
                "invalid environment assignment '{expanded}'"
            )));
        };
        let arg = cooked_word_arg(builder, value, span);
        builder.push_env_assignment_input(
            Name::intern(name),
            ArenaEnvAssignmentValue::CommandArg(arg),
            span,
        );
        words.next();
    }
    let env = builder.finish_env_assignments();

    let mut argv = Vec::new();
    for word in words {
        argv.extend(expand_word(session, word)?);
    }
    let Some(target) = argv.first() else {
        return Err(ExpansionError::usage("expected command"));
    };
    let target = cooked_word_arg(builder, target, span);

    builder.begin_command_args();
    for arg in argv.iter().skip(1) {
        let arg = cooked_word_arg(builder, arg, span);
        builder.push_command_arg_input(arg);
    }
    let args = builder.finish_command_args();

    builder.begin_redirections();
    for redirection in &command.redirections {
        lower_redirection(session, redirection, span, builder)?;
    }
    let redirections = builder.finish_redirections();

    builder.push_run_segment_parts(
        kind,
        false,
        None,
        None,
        env,
        false,
        target,
        args,
        redirections,
        span,
    );
    Ok(())
}

fn lower_redirection(
    session: &Session,
    redirection: &ShellRedirection,
    span: Span,
    builder: &mut ArenaProgramBuilder<'_>,
) -> Result<(), ExpansionError> {
    let target = expand_word_to_string(session, &redirection.target)?;
    let (kind, target) = match redirection.kind {
        ShellRedirectionKind::Stdin => (
            RedirectionKind::StdinRead,
            ArenaRedirectionTarget::Path(cooked_word_arg(builder, &target, span)),
        ),
        ShellRedirectionKind::StdoutWrite => (
            RedirectionKind::StdoutWrite,
            ArenaRedirectionTarget::Path(cooked_word_arg(builder, &target, span)),
        ),
        ShellRedirectionKind::StdoutAppend => (
            RedirectionKind::StdoutAppend,
            ArenaRedirectionTarget::Path(cooked_word_arg(builder, &target, span)),
        ),
        ShellRedirectionKind::StderrWrite => (
            RedirectionKind::StderrWrite,
            ArenaRedirectionTarget::Path(cooked_word_arg(builder, &target, span)),
        ),
        ShellRedirectionKind::StderrAppend => (
            RedirectionKind::StderrAppend,
            ArenaRedirectionTarget::Path(cooked_word_arg(builder, &target, span)),
        ),
        ShellRedirectionKind::StdoutToStderr => (
            RedirectionKind::StdoutDup,
            ArenaRedirectionTarget::Fd(cooked_word_arg(builder, "2", span)),
        ),
        ShellRedirectionKind::StderrToStdout => (
            RedirectionKind::StdoutDup,
            ArenaRedirectionTarget::Fd(cooked_word_arg(builder, "1", span)),
        ),
    };
    builder.push_redirection_input(kind, target, span);
    Ok(())
}

/// Build a word command-argument holding a single cooked (owned) quoted literal.
/// `search_end = 0` guarantees the value is never matched against source text, so
/// it is interned as cooked text regardless of the builder's source.
fn cooked_word_arg(
    builder: &mut ArenaProgramBuilder<'_>,
    text: &str,
    span: Span,
) -> ArenaCommandArg {
    builder.begin_word_parts();
    let value: Arc<str> = Arc::from(text);
    let mut search_from = 0usize;
    builder.push_quoted_word_part_text(&value, span, &mut search_from, 0);
    let parts = builder.finish_word_parts();
    builder.word_command_arg(parts, span)
}

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
        });
        out.push(' ');
        out.push_str(&xsh_word(&redirection.target.text()));
    }
}
