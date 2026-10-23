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
    Callable { source: ValueLeafSource, instance: Box<I> },
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
    pub(crate) fn try_map<C, U, J, E>(
        &self,
        context: &mut C,
        map_type: &mut impl FnMut(&mut C, &T) -> Result<U, E>,
        map_instance: &mut impl FnMut(&mut C, ValueLeafSource, &I) -> Result<(ValueLeafSource, J), E>,
    ) -> Result<ValueMaterialization<U, J>, MaterializationMapError<E>> {
        self.try_map_at_depth(context, map_type, map_instance, 0)
    }

    fn try_map_at_depth<C, U, J, E>(
        &self,
        context: &mut C,
        map_type: &mut impl FnMut(&mut C, &T) -> Result<U, E>,
        map_instance: &mut impl FnMut(&mut C, ValueLeafSource, &I) -> Result<(ValueLeafSource, J), E>,
        depth: usize,
    ) -> Result<ValueMaterialization<U, J>, MaterializationMapError<E>> {
        if depth > super::constraints::MAX_TYPE_DEPTH { return Err(MaterializationMapError::TypeDepth); }
        let source = map_type(context, &self.source).map_err(MaterializationMapError::Mapping)?;
        let destination = map_type(context, &self.destination).map_err(MaterializationMapError::Mapping)?;
        let shape = match &self.shape {
            Materialization::Keep => Materialization::Keep,
            Materialization::Callable { source, instance } => {
                let (source, instance) = map_instance(context, *source, instance).map_err(MaterializationMapError::Mapping)?;
                Materialization::Callable { source, instance: Box::new(instance) }
            }
            Materialization::Record(fields) => Materialization::Record(fields.iter().map(|(&name, field)| {
                Ok((name, field.try_map_at_depth(context, map_type, map_instance, depth + 1)?))
            }).collect::<Result<_, MaterializationMapError<E>>>()?),
            Materialization::List(item) => Materialization::List(Box::new(item.try_map_at_depth(context, map_type, map_instance, depth + 1)?)),
            Materialization::MapValues(value) => Materialization::MapValues(Box::new(value.try_map_at_depth(context, map_type, map_instance, depth + 1)?)),
            Materialization::Optional(value) => Materialization::Optional(Box::new(value.try_map_at_depth(context, map_type, map_instance, depth + 1)?)),
            Materialization::Result { ok, error } => Materialization::Result {
                ok: Box::new(ok.try_map_at_depth(context, map_type, map_instance, depth + 1)?),
                error: Box::new(error.try_map_at_depth(context, map_type, map_instance, depth + 1)?),
            },
        };
        Ok(ValueMaterialization { source, destination, shape })
    }
}

impl<T, I> InstantiationEvidence<T, I> {
    pub(crate) fn invocation(&self) -> Option<&I> {
        match self { Self::Invocation(instance) => Some(instance), Self::Value(_) => None }
    }

    pub(crate) fn try_map<C, U, J, E>(
        &self,
        context: &mut C,
        map_type: &mut impl FnMut(&mut C, &T) -> Result<U, E>,
        map_instance: &mut impl FnMut(&mut C, ValueLeafSource, &I) -> Result<(ValueLeafSource, J), E>,
        map_invocation: &mut impl FnMut(&mut C, &I) -> Result<J, E>,
    ) -> Result<InstantiationEvidence<U, J>, MaterializationMapError<E>> {
        Ok(match self {
            Self::Invocation(instance) => InstantiationEvidence::Invocation(map_invocation(context, instance).map_err(MaterializationMapError::Mapping)?),
            Self::Value(plan) => InstantiationEvidence::Value(Box::new(plan.try_map(context, map_type, map_instance)?)),
        })
    }
}

pub(crate) type CheckedInstantiation = InstantiationEvidence<super::schemes::SchemeType, super::schemes::CallInstantiation>;

pub(crate) struct MaterializationNodes<'a, T, I> {
    pending: Vec<&'a ValueMaterialization<T, I>>,
}

impl<'a, T, I> Iterator for MaterializationNodes<'a, T, I> {
    type Item = &'a ValueMaterialization<T, I>;

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.pending.pop()?;
        match &node.shape {
            Materialization::Keep | Materialization::Callable { .. } => {}
            Materialization::Record(fields) => self.pending.extend(fields.values().rev()),
            Materialization::List(item) | Materialization::MapValues(item) | Materialization::Optional(item) => self.pending.push(item),
            Materialization::Result { ok, error } => { self.pending.push(error); self.pending.push(ok); }
        }
        Some(node)
    }
}

impl<T, I> InstantiationEvidence<T, I> {
    pub(crate) fn nodes(&self) -> MaterializationNodes<'_, T, I> {
        MaterializationNodes { pending: match self { Self::Invocation(_) => Vec::new(), Self::Value(plan) => vec![plan] } }
    }

    pub(crate) fn instances(&self) -> impl Iterator<Item = &I> {
        let invocation = match self { Self::Invocation(instance) => Some(instance), Self::Value(_) => None };
        invocation.into_iter().chain(self.nodes().filter_map(|node| match &node.shape {
            Materialization::Callable { instance, .. } => Some(instance.as_ref()),
            _ => None,
        }))
    }
}

impl ValueMaterialization<super::schemes::SchemeType, super::schemes::CallInstantiation> {
    fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        let mut bytes = size_of::<Self>();
        let mut pending = vec![self];
        while let Some(node) = pending.pop() {
            bytes += node.source.retained_bytes().saturating_sub(size_of::<super::schemes::SchemeType>())
                + node.destination.retained_bytes().saturating_sub(size_of::<super::schemes::SchemeType>());
            match &node.shape {
                Materialization::Keep => {}
                Materialization::Callable { instance, .. } => bytes += instance.retained_bytes(),
                Materialization::Record(fields) => {
                    bytes += fields.len() * size_of::<(Name, Self)>();
                    pending.extend(fields.values());
                }
                Materialization::List(item) | Materialization::MapValues(item) | Materialization::Optional(item) => {
                    bytes += size_of::<Self>();
                    pending.push(item);
                }
                Materialization::Result { ok, error } => {
                    bytes += 2 * size_of::<Self>();
                    pending.push(ok);
                    pending.push(error);
                }
            }
        }
        bytes
    }
}

impl InstantiationEvidence<super::schemes::SchemeType, super::schemes::CallInstantiation> {
    pub(crate) fn retained_bytes(&self) -> usize {
        use std::mem::size_of;
        size_of::<Self>() + match self {
            Self::Invocation(instance) => instance.retained_bytes().saturating_sub(size_of::<super::schemes::CallInstantiation>()),
            Self::Value(plan) => plan.retained_bytes(),
        }
    }
}
