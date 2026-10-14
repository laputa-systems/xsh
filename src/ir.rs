/// Return the compiled full-instruction tag names in schema order.
pub fn full_instruction_tag_names() -> &'static [&'static str] {
    crate::runtime::eval::full_instruction_tag_names()
}
