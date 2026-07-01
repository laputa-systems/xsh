use crate::modules::signature::convert_type;
use crate::sema::types::Type;
use std::collections::BTreeMap;
use std::sync::LazyLock;

pub fn record_schemas() -> BTreeMap<&'static str, Type> {
    xsh_registry::records::record_schemas()
        .into_iter()
        .map(|(name, ty)| (name, convert_type(&ty)))
        .collect()
}

static RECORD_SCHEMAS: LazyLock<BTreeMap<&'static str, Type>> = LazyLock::new(record_schemas);

pub(crate) fn standard_record_type(name: &str) -> Option<Type> {
    RECORD_SCHEMAS.get(name).cloned()
}

#[cfg(test)]
mod tests {
    use super::record_schemas;
    use crate::modules::signature::convert_type;

    #[test]
    fn record_schema_adapter_exactly_mirrors_registry() {
        let main = record_schemas();
        let registry = xsh_registry::records::record_schemas();

        assert_eq!(main.len(), registry.len());
        for (name, registry_ty) in registry {
            assert_eq!(main.get(name), Some(&convert_type(&registry_ty)));
        }
    }
}
