use std::collections::BTreeMap;
use deser::ErrorKind;
use deser_ini::{SyntaxDetail, from_str};

#[test]
fn section_key_collisions_retain_their_typed_detail_and_location() {
    for input in ["server = root\n[server]\nhost = worker\n"] {
        let error = from_str::<BTreeMap<String, String>>(input).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Syntax);
        assert_eq!(error.attachment::<SyntaxDetail>(), Some(&SyntaxDetail::DuplicateKey));
        assert!(error.offset().is_some());
    }
}
