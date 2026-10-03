use super::*;
use super::super::indexed::full::{BuildRunEnvironmentSource, BuildRunPacketSource, BuildRunStdinSource};

impl CompactLowerConstructProbe<'_, '_> {
    /// Command directives keep their authored values and source ports before
    /// encoding turns them into pooled words and executable expression rows.
    pub(super) fn original_run_packet_source(&self, id: crate::syntax::arena::RunFormId) -> Option<BuildRunPacketSource> {
        use crate::syntax::arena::{ArenaEnvAssignmentValue, ArenaRedirectionTarget};
        let run = self.program.arena.run_form(id);
        let [segment] = self.program.arena.run_segments(run.segments) else { return None; };
        let mut environment = Vec::new();
        for assignment in self.program.arena.env_assignments(segment.env) {
            let ArenaEnvAssignmentValue::CommandArg(argument) = &assignment.value else { return None; };
            let ArenaCommandArgKind::Word(parts) = argument.kind else { return None; };
            let argument_span = self.program.arena.span(argument.span);
            let mut text = String::new();
            for part in self.program.arena.word_parts(parts) {
                match part {
                    ArenaWordPart::Bare(value) => text.push_str(self.bare_text_value_in_span(&value, argument_span)?),
                    ArenaWordPart::Quoted(value) => text.push_str(&self.text_value_in_span(&value, argument_span)?),
                    _ => return None,
                }
            }
            environment.push(BuildRunEnvironmentSource { name: assignment.name, span: self.program.arena.span(assignment.span), argument_span, text: Arc::from(text) });
        }
        let mut stdin = Vec::new();
        for redirection in self.program.arena.redirections(segment.redirections) {
            if redirection.kind != crate::syntax::node::RedirectionKind::StdinRead { return None; }
            let ArenaRedirectionTarget::Path(argument) = &redirection.target else { return None; };
            let (expression, mode) = match argument.kind {
                ArenaCommandArgKind::Typed(expression) => (expression, 0),
                ArenaCommandArgKind::Word(parts) => {
                    let parts = self.program.arena.word_parts(parts).collect::<Vec<_>>();
                    let [ArenaWordPart::Shorthand(expression) | ArenaWordPart::Interpolation(expression)] = parts.as_slice() else { return None; };
                    (*expression, 1)
                }
                _ => return None,
            };
            let origin = self.expression_identity(expression);
            let ty = *self.solved().expressions.get(&origin)?;
            let caller = self.solved().expression_owners.get(&origin).copied();
            let source_type = crate::sema::inference::ScopedRoot { ty, scope: self.solved().expression_scope(origin, caller).ok()? };
            stdin.push(BuildRunStdinSource { kind: redirection.kind, span: self.program.arena.span(redirection.span), argument_span: self.program.arena.span(argument.span), mode, origin, source_type });
        }
        Some(BuildRunPacketSource { environment, stdin })
    }
}
