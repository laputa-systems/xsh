use super::lex::lex_shell;
use super::syntax::{
    Chain, ChainOp, PipeOp, Pipeline, Redirection, RedirectionKind, ShellLine, ShellToken,
    ShellWord, ShellWordPart, SimpleCommand,
};

fn literal_descriptor(descriptor: &str) -> ShellWord {
    ShellWord {
        source_end: 0,
        parts: vec![ShellWordPart::Text {
            text: descriptor.to_string(),
            expand: false,
            glob: false,
        }],
    }
}

pub(crate) struct ShellParser<'a> {
    source: &'a str,
    tokens: Vec<ShellToken>,
    index: usize,
}

impl<'a> ShellParser<'a> {
    pub(crate) fn new(source: &'a str) -> Self {
        Self {
            source,
            tokens: Vec::new(),
            index: 0,
        }
    }

    pub(crate) fn parse_line(mut self) -> Result<ShellLine, String> {
        self.tokens = lex_shell(self.source)?;
        let mut chains = Vec::new();
        let mut op = ChainOp::Start;
        let mut background = false;
        while self.index < self.tokens.len() {
            let pipeline = self.parse_pipeline()?;
            chains.push(Chain { op, pipeline });
            op = match self.tokens.get(self.index) {
                Some(ShellToken::Background) => {
                    self.index += 1;
                    if self.index != self.tokens.len() {
                        return Err("background marker must end the command".to_string());
                    }
                    background = true;
                    break;
                }
                Some(ShellToken::Semi) => {
                    self.index += 1;
                    ChainOp::Sequence
                }
                Some(ShellToken::And) => {
                    self.index += 1;
                    ChainOp::And
                }
                Some(ShellToken::Or) => {
                    self.index += 1;
                    ChainOp::Or
                }
                Some(_) => return Err("expected command separator".to_string()),
                None => break,
            };
        }
        Ok(ShellLine { chains, background })
    }

    fn parse_pipeline(&mut self) -> Result<Pipeline, String> {
        let mut commands = vec![self.parse_simple_command()?];
        let mut pipes = Vec::new();
        loop {
            let pipe = match self.tokens.get(self.index) {
                Some(ShellToken::Pipe) => PipeOp::Stdout,
                Some(ShellToken::PipeErr) => PipeOp::StdoutStderr,
                _ => break,
            };
            self.index += 1;
            pipes.push(pipe);
            commands.push(self.parse_simple_command()?);
        }
        Ok(Pipeline { commands, pipes })
    }

    fn parse_simple_command(&mut self) -> Result<SimpleCommand, String> {
        let mut words = Vec::new();
        let mut redirections = Vec::new();
        while let Some(token) = self.tokens.get(self.index).cloned() {
            match token {
                ShellToken::Word(word) => {
                    self.index += 1;
                    words.push(word);
                }
                ShellToken::Redir(RedirectionKind::StdinClose) => {
                    self.index += 1;
                    redirections.push(Redirection {
                        kind: RedirectionKind::StdinClose,
                        target: literal_descriptor("-"),
                    });
                }
                ShellToken::Redir(kind) if kind.is_dup() => {
                    self.index += 1;
                    let descriptor = if kind == RedirectionKind::StdoutToStderr {
                        "2"
                    } else {
                        "1"
                    };
                    redirections.push(Redirection {
                        kind,
                        target: literal_descriptor(descriptor),
                    });
                }
                ShellToken::Redir(kind) => {
                    self.index += 1;
                    let Some(ShellToken::Word(target)) = self.tokens.get(self.index).cloned()
                    else {
                        return Err("redirection requires a target".to_string());
                    };
                    self.index += 1;
                    redirections.push(Redirection { kind, target });
                }
                ShellToken::Pipe
                | ShellToken::PipeErr
                | ShellToken::And
                | ShellToken::Or
                | ShellToken::Background
                | ShellToken::Semi => break,
            }
        }
        if words.is_empty() {
            Err("expected command".to_string())
        } else {
            Ok(SimpleCommand {
                words,
                redirections,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_stdin_is_a_foreground_self_contained_redirection() {
        for source in ["cat 0<&-", "cat <&-", "cat 0<&- <input", "cat <input 0<&-"] {
            let parsed = ShellParser::new(source).parse_line().expect("parse stdin close");
            assert!(!parsed.background);
            assert_eq!(parsed.chains.len(), 1);
            let command = &parsed.chains[0].pipeline.commands[0];
            assert_eq!(command.words.len(), 1);
            assert_eq!(command.words[0].text(), "cat");
            assert_eq!(command.redirections.iter().filter(|item| item.kind == RedirectionKind::StdinClose).count(), 1);
        }
        let parsed = ShellParser::new("cat 0<input").parse_line().expect("parse explicit stdin");
        assert_eq!(parsed.chains[0].pipeline.commands[0].redirections[0].kind, RedirectionKind::Stdin);
        for source in ["cat 0<&", "cat 0<&-oops", "cat <&1"] {
            assert!(ShellParser::new(source).parse_line().is_err(), "{source}");
        }
    }
}
