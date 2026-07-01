#[derive(Clone, Debug)]
pub(crate) struct ShellLine {
    pub(crate) chains: Vec<Chain>,
    pub(crate) background: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct Chain {
    pub(crate) op: ChainOp,
    pub(crate) pipeline: Pipeline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ChainOp {
    Start,
    Sequence,
    And,
    Or,
}

#[derive(Clone, Debug)]
pub(crate) struct Pipeline {
    pub(crate) commands: Vec<SimpleCommand>,
    pub(crate) pipes: Vec<PipeOp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PipeOp {
    Stdout,
    StdoutStderr,
}

#[derive(Clone, Debug)]
pub(crate) struct SimpleCommand {
    pub(crate) words: Vec<ShellWord>,
    pub(crate) redirections: Vec<Redirection>,
}

#[derive(Clone, Debug)]
pub(crate) struct Redirection {
    pub(crate) kind: RedirectionKind,
    pub(crate) target: ShellWord,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RedirectionKind {
    Stdin,
    StdoutWrite,
    StdoutAppend,
    StderrWrite,
    StderrAppend,
    StdoutToStderr,
    StderrToStdout,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellToken {
    Word(ShellWord),
    Pipe,
    PipeErr,
    And,
    Or,
    Semi,
    Background,
    Redir(RedirectionKind),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShellWord {
    pub(crate) parts: Vec<ShellWordPart>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ShellWordPart {
    Text {
        text: String,
        expand: bool,
        glob: bool,
    },
    CommandSubstitution {
        source: String,
        glob: bool,
    },
    ArithmeticExpansion {
        source: String,
        glob: bool,
    },
}

impl ShellWord {
    #[allow(clippy::single_call_fn)]
    pub(crate) fn text(&self) -> String {
        let mut output = String::new();
        for part in &self.parts {
            match part {
                ShellWordPart::Text { text, .. } => output.push_str(text),
                ShellWordPart::CommandSubstitution { source, .. } => {
                    output.push_str("$(");
                    output.push_str(source);
                    output.push(')');
                }
                ShellWordPart::ArithmeticExpansion { source, .. } => {
                    output.push_str("$((");
                    output.push_str(source);
                    output.push_str("))");
                }
            }
        }
        output
    }
}
