use super::lex::lex_shell;
use super::syntax::{
    Chain, ChainOp, PipeOp, Pipeline, Redirection, ShellLine, ShellToken, SimpleCommand,
};

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
