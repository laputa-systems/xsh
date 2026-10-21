const valid_root_bundle = b"-----BEGIN CERTIFICATE-----\nMIIBiTCCAS+gAwIBAgIUYzdXOaaSy4+qMqBhCb1RlSx+uskwCgYIKoZIzj0EAwIw\nETEPMA0GA1UEAwwGcm9vdC1hMCAXDTI2MTAyMTE2NDQyMloYDzIxMjYwOTI3MTY0\nNDIyWjARMQ8wDQYDVQQDDAZyb290LWEwWTATBgcqhkjOPQIBBggqhkjOPQMBBwNC\nAARcP2UyLdgkd5fYxeNJTsg4NzPis/YuI9aOpSQdh39Xg0ABt9fuakMI/yaRMzjH\n9IOJ48a47y/CETqeHPm1jqaFo2MwYTAdBgNVHQ4EFgQUwZF2LXHG9HCa0fHnqlZG\nZQvt77AwHwYDVR0jBBgwFoAUwZF2LXHG9HCa0fHnqlZGZQvt77AwDwYDVR0TAQH/\nBAUwAwEB/zAOBgNVHQ8BAf8EBAMCAQYwCgYIKoZIzj0EAwIDSAAwRQIgdKlanzXx\nwTyOI1EX6qrW7L/SbQFbLo/bjxfrkMjLUS8CIQD4LQ8LDeEzq1LSK49qc8Uy/n0a\nxAPXzKgnnPJo3S06Qg==\n-----END CERTIFICATE-----\n"

proc check_root_bundle(pem: Bytes) [] -> Result[Unit] {
  net.validate_root_bundle(pem)
}

test test_net_validate_root_bundle_accepts_each_valid_pem_certificate {
  check_root_bundle(valid_root_bundle)?
  net.validate_root_bundle(bytes.concat([valid_root_bundle, b"\n# extra root\n", valid_root_bundle]))?
}

test test_net_validate_root_bundle_rejects_empty_raw_der_and_mixed_invalid_pem {
  let invalid = [
    b"",
    b"\x30\x03\x02\x01\x00",
    b"-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n",
    b"-----BEGIN CERTIFICATE-----\ninvalid base64!\n-----END CERTIFICATE-----\n",
    b"-----BEGIN CERTIFICATE-----\nAAAA\n",
    b"-----BEGIN PRIVATE KEY-----\nAAAA\n-----END PRIVATE KEY-----\n",
    b"unrelated text",
  ]
  for pem in invalid {
    match net.validate_root_bundle(pem) {
      Err(NetError.TrustStore {..}) => {}
      _ => assert false, "invalid root bundle was accepted"
    }
    if !pem.is_empty() {
      match net.validate_root_bundle(bytes.concat([valid_root_bundle, pem])) {
        Err(NetError.TrustStore {..}) => {}
        _ => assert false, "invalid certificate in mixed root bundle was ignored"
      }
    }
  }
}
