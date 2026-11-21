##! Remote limit test.
use system.remote as remote

test test_remote_test_limit_is_positive {
  assert remote.REMOTE_TEST_ONLY_LIMIT > 0
}
