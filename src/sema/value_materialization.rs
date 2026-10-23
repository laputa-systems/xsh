use super::check::CheckedFunctionTarget;
use super::types::SchemeCallableId;
use crate::symbol::Name;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ValueLeafSource {
    DirectTarget(CheckedFunctionTarget),
    Relationship(SchemeCallableId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValueMaterialization<T, I> {
    pub(crate) source: T,
    pub(crate) destination: T,
    pub(crate) shape: Materialization<T, I>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Materialization<T, I> {
    Keep,
    Callable { source: ValueLeafSource, instance: I },
    Record(BTreeMap<Name, ValueMaterialization<T, I>>),
    List(Box<ValueMaterialization<T, I>>),
    MapValues(Box<ValueMaterialization<T, I>>),
    Optional(Box<ValueMaterialization<T, I>>),
    Result { ok: Box<ValueMaterialization<T, I>>, error: Box<ValueMaterialization<T, I>> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InstantiationEvidence<T, I> {
    Invocation(I),
    Value(Box<ValueMaterialization<T, I>>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum MaterializationMapError<E> {
    TypeDepth,
    Mapping(E),
}

impl<T, I> ValueMaterialization<T, I> {
    pub(crate) fn try_map<U, J, E>(
        &self,
        map_type: &mut impl FnMut(&T) -> Result<U, E>,
        map_instance: &mut impl FnMut(ValueLeafSource, &I) -> Result<(ValueLeafSource, J), E>,
    ) -> Result<ValueMaterialization<U, J>, MaterializationMapError<E>> {
        self.try_map_at_depth(map_type, map_instance, 0)
    }

    fn try_map_at_depth<U, J, E>(
        &self,
        map_type: &mut impl FnMut(&T) -> Result<U, E>,
        map_instance: &mut impl FnMut(ValueLeafSource, &I) -> Result<(ValueLeafSource, J), E>,
        depth: usize,
    ) -> Result<ValueMaterialization<U, J>, MaterializationMapError<E>> {
        if depth > super::constraints::MAX_TYPE_DEPTH { return Err(MaterializationMapError::TypeDepth); }
        let source = map_type(&self.source).map_err(MaterializationMapError::Mapping)?;
        let destination = map_type(&self.destination).map_err(MaterializationMapError::Mapping)?;
        let shape = match &self.shape {
            Materialization::Keep => Materialization::Keep,
            Materialization::Callable { source, instance } => {
                let (source, instance) = map_instance(*source, instance).map_err(MaterializationMapError::Mapping)?;
                Materialization::Callable { source, instance }
            }
            Materialization::Record(fields) => Materialization::Record(fields.iter().map(|(&name, field)| {
                Ok((name, field.try_map_at_depth(map_type, map_instance, depth + 1)?))
            }).collect::<Result<_, MaterializationMapError<E>>>()?),
            Materialization::List(item) => Materialization::List(Box::new(item.try_map_at_depth(map_type, map_instance, depth + 1)?)),
            Materialization::MapValues(value) => Materialization::MapValues(Box::new(value.try_map_at_depth(map_type, map_instance, depth + 1)?)),
            Materialization::Optional(value) => Materialization::Optional(Box::new(value.try_map_at_depth(map_type, map_instance, depth + 1)?)),
            Materialization::Result { ok, error } => Materialization::Result {
                ok: Box::new(ok.try_map_at_depth(map_type, map_instance, depth + 1)?),
                error: Box::new(error.try_map_at_depth(map_type, map_instance, depth + 1)?),
            },
        };
        Ok(ValueMaterialization { source, destination, shape })
    }
}
