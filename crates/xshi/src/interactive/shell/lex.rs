use super::syntax::{RedirectionKind, ShellToken, ShellWord, ShellWordPart};

pub(crate) fn lex_shell(source: &str) -> Result<Vec<ShellToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = source.char_indices().peekable();
    while let Some((_, ch)) = chars.peek().copied() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        match ch {
            ';' => {
                chars.next();
                tokens.push(ShellToken::Semi);
            }
            '&' => {
                chars.next();
                if chars.peek().is_some_and(|(_, ch)| *ch == '&') {
                    chars.next();
                    tokens.push(ShellToken::And);
                } else if chars.peek().is_some_and(|(_, ch)| *ch == '>') {
                    return Err(
                        "combined stdout/stderr redirection is not supported in this tranche"
                            .to_string(),
                    );
                } else {
                    tokens.push(ShellToken::Background);
                }
            }
            '|' => {
                chars.next();
                if chars.peek().is_some_and(|(_, ch)| *ch == '|') {
                    chars.next();
                    tokens.push(ShellToken::Or);
                } else if chars.peek().is_some_and(|(_, ch)| *ch == '&') {
                    chars.next();
                    tokens.push(ShellToken::PipeErr);
                } else {
                    tokens.push(ShellToken::Pipe);
                }
            }
            '<' => {
                chars.next();
                if chars.peek().is_some_and(|(_, ch)| *ch == '<') {
                    return Err("here-docs are not implemented in this build".to_string());
                }
                tokens.push(ShellToken::Redir(RedirectionKind::Stdin));
            }
            '>' => {
                chars.next();
                if chars.peek().is_some_and(|(_, ch)| *ch == '>') {
                    chars.next();
                    tokens.push(ShellToken::Redir(RedirectionKind::StdoutAppend));
                } else {
                    tokens.push(ShellToken::Redir(RedirectionKind::StdoutWrite));
                }
            }
            '1' | '2' => {
                let save = chars.clone();
                let fd = ch;
                chars.next();
                if chars.peek().is_some_and(|(_, ch)| *ch == '>') {
                    chars.next();
                    let append = if chars.peek().is_some_and(|(_, ch)| *ch == '>') {
                        chars.next();
                        true
                    } else {
                        false
                    };
                    if chars.peek().is_some_and(|(_, ch)| *ch == '&') {
                        chars.next();
                        let Some((_, target_fd)) = chars.next() else {
                            return Err("fd duplication requires a target fd".to_string());
                        };
                        match (fd, target_fd) {
                            ('1', '2') => {
                                tokens.push(ShellToken::Redir(RedirectionKind::StdoutToStderr))
                            }
                            ('2', '1') => {
                                tokens.push(ShellToken::Redir(RedirectionKind::StderrToStdout))
                            }
                            _ => return Err("only 2>&1 and 1>&2 are supported".to_string()),
                        }
                    } else if fd == '2' {
                        tokens.push(ShellToken::Redir(if append {
                            RedirectionKind::StderrAppend
                        } else {
                            RedirectionKind::StderrWrite
                        }));
                    } else {
                        tokens.push(ShellToken::Redir(if append {
                            RedirectionKind::StdoutAppend
                        } else {
                            RedirectionKind::StdoutWrite
                        }));
                    }
                } else {
                    chars = save;
                    tokens.push(ShellToken::Word(read_word(&mut chars)?));
                }
            }
            '#' => return Err("shell comments are not supported in this tranche".to_string()),
            _ => tokens.push(ShellToken::Word(read_word(&mut chars)?)),
        }
    }
    Ok(tokens)
}

fn read_word(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Result<ShellWord, String> {
    let mut parts = Vec::new();
    let mut unquoted = String::new();
    while let Some((_, ch)) = chars.peek().copied() {
        if ch.is_whitespace() || matches!(ch, ';' | '|' | '<' | '>' | '&') {
            break;
        }
        match ch {
            '\'' => {
                push_shell_text(&mut parts, &mut unquoted, true, true);
                chars.next();
                let mut literal = String::new();
                loop {
                    let Some((_, inner)) = chars.next() else {
                        return Err("unterminated single quote".to_string());
                    };
                    if inner == '\'' {
                        break;
                    }
                    literal.push(inner);
                }
                push_shell_text(&mut parts, &mut literal, false, false);
            }
            '"' => {
                push_shell_text(&mut parts, &mut unquoted, true, true);
                chars.next();
                let mut quoted = String::new();
                loop {
                    let Some((_, inner)) = chars.next() else {
                        return Err("unterminated double quote".to_string());
                    };
                    if inner == '"' {
                        break;
                    }
                    if inner == '\\' {
                        if let Some((_, escaped)) = chars.next() {
                            push_shell_text(&mut parts, &mut quoted, true, false);
                            let mut literal = escaped.to_string();
                            push_shell_text(&mut parts, &mut literal, false, false);
                        }
                    } else if inner == '$' && chars.peek().is_some_and(|(_, ch)| *ch == '(') {
                        chars.next();
                        push_shell_text(&mut parts, &mut quoted, true, false);
                        if chars.peek().is_some_and(|(_, ch)| *ch == '(') {
                            chars.next();
                            parts.push(ShellWordPart::ArithmeticExpansion {
                                source: read_arithmetic_expansion(chars)?,
                                glob: false,
                            });
                        } else {
                            parts.push(ShellWordPart::CommandSubstitution {
                                source: read_command_substitution(chars)?,
                                glob: false,
                            });
                        }
                    } else if inner == '`' {
                        push_shell_text(&mut parts, &mut quoted, true, false);
                        parts.push(ShellWordPart::CommandSubstitution {
                            source: read_backtick_command_substitution(chars)?,
                            glob: false,
                        });
                    } else {
                        quoted.push(inner);
                    }
                }
                push_shell_text(&mut parts, &mut quoted, true, false);
            }
            '\\' => {
                push_shell_text(&mut parts, &mut unquoted, true, true);
                chars.next();
                if let Some((_, escaped)) = chars.next() {
                    let mut literal = escaped.to_string();
                    push_shell_text(&mut parts, &mut literal, false, false);
                }
            }
            '`' => {
                push_shell_text(&mut parts, &mut unquoted, true, true);
                chars.next();
                parts.push(ShellWordPart::CommandSubstitution {
                    source: read_backtick_command_substitution(chars)?,
                    glob: true,
                });
            }
            '$' => {
                chars.next();
                if chars.peek().is_some_and(|(_, ch)| *ch == '(') {
                    chars.next();
                    push_shell_text(&mut parts, &mut unquoted, true, true);
                    if chars.peek().is_some_and(|(_, ch)| *ch == '(') {
                        chars.next();
                        parts.push(ShellWordPart::ArithmeticExpansion {
                            source: read_arithmetic_expansion(chars)?,
                            glob: true,
                        });
                    } else {
                        parts.push(ShellWordPart::CommandSubstitution {
                            source: read_command_substitution(chars)?,
                            glob: true,
                        });
                    }
                } else {
                    unquoted.push('$');
                }
            }
            _ => {
                chars.next();
                unquoted.push(ch);
            }
        }
    }
    push_shell_text(&mut parts, &mut unquoted, true, true);
    Ok(ShellWord { parts })
}

fn push_shell_text(parts: &mut Vec<ShellWordPart>, text: &mut String, expand: bool, glob: bool) {
    if text.is_empty() {
        return;
    }
    parts.push(ShellWordPart::Text {
        text: std::mem::take(text),
        expand,
        glob,
    });
}

fn read_command_substitution(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Result<String, String> {
    let mut output = String::new();
    let mut depth = 1usize;
    let mut quote = None;
    while let Some((_, ch)) = chars.next() {
        match quote {
            Some('\'') => {
                output.push(ch);
                if ch == '\'' {
                    quote = None;
                }
            }
            Some('"') => {
                output.push(ch);
                if ch == '"' {
                    quote = None;
                } else if ch == '\\' {
                    if let Some((_, escaped)) = chars.next() {
                        output.push(escaped);
                    }
                } else if ch == '$' && chars.peek().is_some_and(|(_, next)| *next == '(') {
                    chars.next();
                    depth += 1;
                    output.push('(');
                }
            }
            _ => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    output.push(ch);
                }
                '\\' => {
                    output.push(ch);
                    if let Some((_, escaped)) = chars.next() {
                        output.push(escaped);
                    }
                }
                '$' if chars.peek().is_some_and(|(_, next)| *next == '(') => {
                    chars.next();
                    depth += 1;
                    output.push_str("$(");
                }
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(output);
                    }
                    output.push(ch);
                }
                _ => output.push(ch),
            },
        }
    }
    Err("unterminated command substitution".to_string())
}

fn read_backtick_command_substitution(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Result<String, String> {
    let mut output = String::new();
    while let Some((_, ch)) = chars.next() {
        if ch == '`' {
            return Ok(output);
        }
        if ch == '\\' {
            if let Some((_, escaped)) = chars.next() {
                output.push(escaped);
            }
        } else {
            output.push(ch);
        }
    }
    Err("unterminated command substitution".to_string())
}

fn read_arithmetic_expansion(
    chars: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
) -> Result<String, String> {
    let mut output = String::new();
    let mut depth = 0usize;
    while let Some((_, ch)) = chars.next() {
        match ch {
            '(' => {
                depth += 1;
                output.push(ch);
            }
            ')' => {
                if depth == 0 && chars.peek().is_some_and(|(_, next)| *next == ')') {
                    chars.next();
                    return Ok(output);
                }
                depth = depth.saturating_sub(1);
                output.push(ch);
            }
            '\\' => {
                output.push(ch);
                if let Some((_, escaped)) = chars.next() {
                    output.push(escaped);
                }
            }
            _ => output.push(ch),
        }
    }
    Err("unterminated arithmetic expansion".to_string())
}
