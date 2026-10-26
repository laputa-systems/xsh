use std::collections::{BTreeMap, BTreeSet};

use crate::source::SourceId;
use crate::symbol::Name;
use crate::syntax::arena::{ArenaAssignTargetKind, ArenaProgram, ArenaRange, AssignTargetId};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct AssignmentFactOwner {
    source_id: SourceId,
    statement_start: u32,
    statement_len: u32,
}

impl AssignmentFactOwner {
    fn new(source_id: SourceId, statements: ArenaRange) -> Self {
        Self {
            source_id,
            statement_start: statements.start,
            statement_len: statements.len,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct AssignmentFacts {
    owners: BTreeMap<AssignmentFactOwner, BTreeSet<Name>>,
}

impl AssignmentFacts {
    pub(super) fn initialize_owner(&mut self, source_id: SourceId, statements: ArenaRange) {
        self.owners
            .entry(AssignmentFactOwner::new(source_id, statements))
            .or_default();
    }

    pub(super) fn insert(
        &mut self,
        source_id: SourceId,
        statements: ArenaRange,
        name: Name,
    ) {
        self.owners
            .get_mut(&AssignmentFactOwner::new(source_id, statements))
            .expect("assignment owner is initialized before capture")
            .insert(name);
    }

    pub(super) fn names(
        &self,
        source_id: SourceId,
        statements: ArenaRange,
    ) -> Option<&BTreeSet<Name>> {
        self.owners
            .get(&AssignmentFactOwner::new(source_id, statements))
    }

    pub(super) fn merge(&mut self, other: &Self) {
        for (&owner, names) in &other.owners {
            if let Some(existing) = self.owners.get(&owner) {
                assert_eq!(existing, names, "assignment facts differ for {owner:?}");
            } else {
                self.owners.insert(owner, names.clone());
            }
        }
    }
}

#[doc = "Returns the name rooted by a name, field, or index target, or `None` for an environment target."]
pub fn assignment_target_root_name(
    arena: &ArenaProgram,
    target: AssignTargetId,
) -> Option<Name> {
    let mut target = target;
    loop {
        match arena.arena.assign_target(target).kind {
            ArenaAssignTargetKind::Name(name) => return Some(name),
            ArenaAssignTargetKind::Field { base, .. }
            | ArenaAssignTargetKind::Index { base, .. } => target = base,
            ArenaAssignTargetKind::Env(_) => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AssignmentFacts;
    use crate::source::SourceId;
    use crate::symbol::Name;
    use crate::syntax::arena::ArenaRange;

    #[test]
    fn assignment_fact_merge_requires_equal_complete_owner_summaries() {
        let owner = (SourceId::new(1), ArenaRange { start: 3, len: 2 });
        let mut left = AssignmentFacts::default();
        left.initialize_owner(owner.0, owner.1);
        left.insert(owner.0, owner.1, Name::intern("left"));

        let mut same = AssignmentFacts::default();
        same.initialize_owner(owner.0, owner.1);
        same.insert(owner.0, owner.1, Name::intern("left"));
        left.merge(&same);
        assert_eq!(left.names(owner.0, owner.1).unwrap().len(), 1);

        let mut incomplete = AssignmentFacts::default();
        incomplete.initialize_owner(owner.0, owner.1);
        incomplete.insert(owner.0, owner.1, Name::intern("other"));
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            left.merge(&incomplete)
        }))
        .is_err());
    }
}
