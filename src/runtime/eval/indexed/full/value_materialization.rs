use super::schemes::{FullInstantiation, SchemeTypeId};
use super::IrRange;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullValueMaterialization {
    pub(super) instruction: u32,
    pub(super) source: SchemeTypeId,
    pub(super) destination: SchemeTypeId,
    pub(super) root: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(super) enum FullMaterializationKind {
    Keep,
    Callable,
    Record,
    List,
    MapValues,
    Optional,
    Result,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullMaterializationNode {
    pub(super) kind: FullMaterializationKind,
    pub(super) reserved: [u8; 3],
    pub(super) children: IrRange,
    pub(super) leaf: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullMaterializationEdge {
    pub(super) name: u32,
    pub(super) node: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub(super) struct FullMaterializationLeaf {
    pub(super) relationship: u32,
    pub(super) instance: FullInstantiation,
}

impl super::FullBuilder {
    pub(super) fn encode_value_materialization(
        &mut self,
        plan: &crate::sema::value_materialization::ValueMaterialization<crate::sema::schemes::SchemeType, crate::sema::schemes::CallInstantiation>,
    ) -> Result<u32, super::IrBuildError> {
        let types = self.active_scheme.map(|scheme| self.scheme_type_parameter_map(scheme)).transpose()?.unwrap_or_default();
        let rows = self.active_scheme.map(|scheme| self.scheme_row_parameter_map(scheme)).transpose()?.unwrap_or_default();
        let effects = self.active_scheme.map(|scheme| self.scheme_effect_parameter_map(scheme)).transpose()?.unwrap_or_default();
        let source = self.intern_scheme_type(&plan.source, &types, &rows, &effects)?;
        let destination = self.intern_scheme_type(&plan.destination, &types, &rows, &effects)?;
        let root = self.encode_materialization_node(plan, 0)?;
        let id = materialization_index(self.store.value_materializations.len())?;
        self.store.value_materializations.push(FullValueMaterialization { instruction: super::IR_NONE, source, destination, root });
        Ok(id)
    }

    fn encode_materialization_node(
        &mut self,
        plan: &crate::sema::value_materialization::ValueMaterialization<crate::sema::schemes::SchemeType, crate::sema::schemes::CallInstantiation>,
        depth: usize,
    ) -> Result<u32, super::IrBuildError> {
        use crate::sema::value_materialization::Materialization as M;
        if depth > crate::sema::constraints::MAX_TYPE_DEPTH { return Err(materialization_build_error()); }
        let mut children = Vec::new();
        let mut leaf = super::IR_NONE;
        let kind = match &plan.shape {
            M::Keep => FullMaterializationKind::Keep,
            M::Callable { relationship, instance } => {
                let relationship = self.intern_scheme_callable_value(*relationship)?.0;
                let instance = self.encode_instantiation(instance)?;
                leaf = materialization_index(self.store.materialization_leaves.len())?;
                self.store.materialization_leaves.push(FullMaterializationLeaf { relationship, instance });
                FullMaterializationKind::Callable
            }
            M::Record(fields) => {
                for (name, child) in fields {
                    let name = self.intern_string(name.as_str().as_str())?.raw();
                    let node = self.encode_materialization_node(child, depth + 1)?;
                    children.push(FullMaterializationEdge { name, node });
                }
                FullMaterializationKind::Record
            }
            M::List(child) | M::MapValues(child) | M::Optional(child) => {
                children.push(FullMaterializationEdge { name: super::IR_NONE, node: self.encode_materialization_node(child, depth + 1)? });
                match &plan.shape { M::List(_) => FullMaterializationKind::List, M::MapValues(_) => FullMaterializationKind::MapValues, _ => FullMaterializationKind::Optional }
            }
            M::Result { ok, error } => {
                children.push(FullMaterializationEdge { name: super::IR_NONE, node: self.encode_materialization_node(ok, depth + 1)? });
                children.push(FullMaterializationEdge { name: super::IR_NONE, node: self.encode_materialization_node(error, depth + 1)? });
                FullMaterializationKind::Result
            }
        };
        let start = self.store.materialization_edges.len();
        self.store.materialization_edges.extend(children);
        let children = super::table_range(start, self.store.materialization_edges.len())?;
        let id = materialization_index(self.store.materialization_nodes.len())?;
        self.store.materialization_nodes.push(FullMaterializationNode { kind, reserved: [0; 3], children, leaf });
        Ok(id)
    }

    pub(super) fn attach_value_materialization(&mut self, instruction: usize, scope: Option<crate::sema::guarded_requirements::GuardScopeId>) -> Result<(), super::IrBuildError> {
        if self.store.tags.get(instruction) != Some(&super::FullTag::ExprCheckedValue) { return Ok(()); }
        let payload = self.store.payload(self.store.data[instruction].range()).map_err(|_| materialization_build_error())?;
        if payload.get(1) != Some(&(super::decode::ValueCheckTag::Materialization as u32)) { return Ok(()); }
        let id = *payload.get(2).ok_or_else(materialization_build_error)?;
        let plan = self.store.value_materializations.get_mut(id as usize).ok_or_else(materialization_build_error)?;
        if plan.instruction != super::IR_NONE { return Err(materialization_build_error()); }
        plan.instruction = materialization_index(instruction)?;
        let mut pending = vec![plan.root];
        while let Some(node) = pending.pop() {
            let node = *self.store.materialization_nodes.get(node as usize).ok_or_else(materialization_build_error)?;
            let children = node.children.bounds(self.store.materialization_edges.len()).ok_or_else(materialization_build_error)?;
            pending.extend(self.store.materialization_edges[children].iter().map(|child| child.node));
            if node.kind == FullMaterializationKind::Callable {
                let leaf = self.store.materialization_leaves.get(node.leaf as usize).copied().ok_or_else(materialization_build_error)?;
                self.record_guard_call_origins(scope, instruction, node.leaf, crate::sema::schemes::InstantiationRole::Value, leaf.instance)?;
            }
        }
        Ok(())
    }
}

fn materialization_build_error() -> super::IrBuildError {
    super::IrBuildError::format("value_materialization", None, 0, 0)
}

fn materialization_index(index: usize) -> Result<u32, super::IrBuildError> {
    u32::try_from(index).ok().filter(|index| *index != super::IR_NONE).ok_or_else(materialization_build_error)
}

impl super::FullProgram {
    pub(in crate::runtime::eval) fn materialize_checked_value(
        &self,
        id: u32,
        instruction: u32,
        caller: &super::PreparedInvocationBindings,
        mut value: super::LoweredValue,
    ) -> Result<(super::LoweredValue, super::Type), super::IrVerifyError> {
        let caller = self.invocation_at_source(instruction, caller)?;
        let plan = self.store.value_materializations.get(id as usize).ok_or_else(materialization_verify_error)?;
        if plan.instruction != instruction { return Err(materialization_verify_error()); }
        let mut budget = super::verify_schemes::SchemeInstantiationBudget::new(&self.store);
        let destination = super::verify_schemes::instantiate_bound_type(&self.store, plan.destination, &caller.substitution, &mut budget, 0)?;
        self.materialize_value_node(plan.root, &mut value, caller, 0, instruction)?;
        Ok((value, destination))
    }

    fn materialize_value_node(
        &self,
        id: u32,
        value: &mut super::LoweredValue,
        caller: &super::PreparedInvocationBindings,
        depth: usize,
        source_instruction: u32,
    ) -> Result<(), super::IrVerifyError> {
        use super::LoweredValue as V;
        if depth > 128 { return Err(materialization_verify_error()); }
        let node = *self.store.materialization_nodes.get(id as usize).ok_or_else(materialization_verify_error)?;
        if node.kind == FullMaterializationKind::Keep { return Ok(()); }
        let range = node.children.bounds(self.store.materialization_edges.len()).ok_or_else(materialization_verify_error)?;
        let children = &self.store.materialization_edges[range];
        match (node.kind, value) {
            (FullMaterializationKind::Callable, value) => {
                let leaf = self.store.materialization_leaves.get(node.leaf as usize).ok_or_else(materialization_verify_error)?;
                self.materialize_callable_leaf(leaf, value, caller, super::PreparedCallSource::Materialization { instruction: source_instruction, leaf: node.leaf })?;
            }
            (FullMaterializationKind::Record, V::Record(fields) | V::Module(fields)) => {
                for child in children {
                    if self.store.materialization_nodes.get(child.node as usize).is_some_and(|node| node.kind == FullMaterializationKind::Keep) { continue; }
                    let name = self.store.string(child.name)?;
                    let field = std::sync::Arc::make_mut(fields).get_mut(name).ok_or_else(materialization_verify_error)?;
                    self.materialize_value_node(child.node, field, caller, depth + 1, source_instruction)?;
                }
            }
            (FullMaterializationKind::Record, V::RecordVec(fields)) => {
                let mut selected = bbtree::unsync::BBTreeMap::new();
                for child in children {
                    if self.store.materialization_nodes.get(child.node as usize).is_some_and(|node| node.kind == FullMaterializationKind::Keep) { continue; }
                    selected.insert(self.store.string(child.name)?, child.node);
                }
                let mut converted = 0;
                for (name, field) in std::sync::Arc::make_mut(fields) {
                    if let Some(&node) = selected.get(name.as_str().as_str()) {
                        self.materialize_value_node(node, field, caller, depth + 1, source_instruction)?;
                        converted += 1;
                    }
                }
                if converted != selected.len() { return Err(materialization_verify_error()); }
            }
            (FullMaterializationKind::List, V::List(items)) => {
                let child = children.first().ok_or_else(materialization_verify_error)?;
                for item in items { self.materialize_value_node(child.node, item, caller, depth + 1, source_instruction)?; }
            }
            (FullMaterializationKind::List, V::SharedList(items)) => {
                let child = children.first().ok_or_else(materialization_verify_error)?;
                for item in std::sync::Arc::make_mut(items) { self.materialize_value_node(child.node, item, caller, depth + 1, source_instruction)?; }
            }
            (FullMaterializationKind::MapValues, V::Map(items)) => {
                let child = children.first().ok_or_else(materialization_verify_error)?;
                for item in std::sync::Arc::make_mut(items).values_mut() { self.materialize_value_node(child.node, item, caller, depth + 1, source_instruction)?; }
            }
            (FullMaterializationKind::Optional, V::Null) => {}
            (FullMaterializationKind::Optional, value) => {
                self.materialize_value_node(children.first().ok_or_else(materialization_verify_error)?.node, value, caller, depth + 1, source_instruction)?;
            }
            (FullMaterializationKind::Result, V::ResultOk(value)) => {
                self.materialize_value_node(children.first().ok_or_else(materialization_verify_error)?.node, value, caller, depth + 1, source_instruction)?;
            }
            (FullMaterializationKind::Result, V::ResultErr(value)) => {
                let child = children.get(1).ok_or_else(materialization_verify_error)?;
                if self.store.materialization_nodes.get(child.node as usize).is_none_or(|node| node.kind != FullMaterializationKind::Keep) {
                    let mut payload = crate::runtime::eval::lowered_ops::lowered_value_from_runtime_any(value).ok_or_else(materialization_verify_error)?;
                    self.materialize_value_node(child.node, &mut payload, caller, depth + 1, source_instruction)?;
                    **value = payload.into_value();
                }
            }
            _ => return Err(materialization_verify_error()),
        }
        Ok(())
    }

    fn materialize_callable_leaf(
        &self,
        leaf: &FullMaterializationLeaf,
        value: &mut super::LoweredValue,
        caller: &super::PreparedInvocationBindings,
        call_source: super::PreparedCallSource,
    ) -> Result<(), super::IrVerifyError> {
        use crate::runtime::eval::{PreparedCallable, PreparedCallableState};
        let super::LoweredValue::Callable(callable) = value else { return Err(materialization_verify_error()); };
        let prepared = callable.instance().ok_or_else(materialization_verify_error)?;
        if !std::ptr::eq(prepared.program.as_ref(), self) { return Err(materialization_verify_error()); }
        let PreparedCallableState::SchemeValue(source) = &prepared.state else { return Err(materialization_verify_error()); };
        let target = self.function_view(prepared.function, prepared.kind)?.ok_or_else(materialization_verify_error)?.index();
        self.validate_scheme_callable_target(*source, target, prepared.kind)?;
        let relationship = leaf.relationship;
        self.validate_scheme_callable_target(relationship, target, prepared.kind)?;
        let bindings = self.invocation_payload(&leaf.instance, Some(relationship), target, caller, call_source)?;
        *value = super::LoweredValue::Callable(crate::runtime::value::CallableValue::prepared(callable.identity(), std::sync::Arc::new(PreparedCallable {
            program: std::sync::Arc::clone(&prepared.program), function: prepared.function, kind: prepared.kind,
            state: PreparedCallableState::Instance(bindings),
        })));
        Ok(())
    }
}

fn materialization_verify_error() -> super::IrVerifyError {
    super::IrVerifyError::new("checked value materialization disagrees with its owned plan")
}
