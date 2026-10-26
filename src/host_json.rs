pub use deser_value::Value;

pub fn parse(text: &str) -> Result<Value, String> {
    deser_json::from_str(text).map_err(|error| error.to_string())
}

pub fn encode(value: &Value) -> Result<String, String> {
    deser_json::to_string(value).map_err(|error| error.to_string())
}

pub fn encode_pretty(value: &Value) -> Result<String, String> {
    deser_json::SerializerConfig::builder()
        .pretty(deser_json::Indent::Spaces(2))
        .build()
        .to_string(value)
        .map_err(|error| error.to_string())
}
