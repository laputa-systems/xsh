use crate::diagnostic::Diagnostic;

#[derive(Clone, Debug, Default)]
pub struct ResolveOutput {
    pub diagnostics: Vec<Diagnostic>,
}
