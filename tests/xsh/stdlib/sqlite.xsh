type SqlTestCell = Union[Int, Float, Str, Bytes]?

pure sqlite_rows(actual: List[List[SqlTestCell]], expected: List[List[SqlTestCell]]) -> Bool {
  actual == expected
}

test test_sqlite_memory_is_private_and_preserves_table_shape {
  with first = sqlite.memory()? {
    with second = sqlite.memory()? {
      let _ = first.execute("CREATE TABLE item (value INTEGER)")?
      assert second.query("SELECT value FROM item") is Err(_)
      let empty = first.query("SELECT value AS repeated, value AS repeated FROM item")?
      assert empty.columns == ["repeated", "repeated"]
      assert empty.rows == []
      let _ = first.execute("INSERT INTO item VALUES (2), (1)")?
      let result = first.query("SELECT value FROM item ORDER BY value")?
      assert sqlite_rows(result.rows, [[1], [2]])
    }
  }
}

test test_sqlite_binds_values_without_interpreting_text {
  with db = sqlite.memory()? {
    let _ = db.execute("CREATE TABLE item (value)")?
    let text = "'); DROP TABLE item; --"
    let _ = db.execute("INSERT INTO item VALUES (?)", [text])?
    let _ = db.execute("INSERT INTO item VALUES (?)", [null])?
    let _ = db.execute("INSERT INTO item VALUES (?)", [b"\x00\xff"])?
    let _ = db.execute("INSERT INTO item VALUES (?)", [b""])?
    let _ = db.execute("INSERT INTO item VALUES (?)", [""])?
    let _ = db.execute("INSERT INTO item VALUES (?)", [true])?
    let result = db.query("SELECT value FROM item ORDER BY rowid")?
    assert sqlite_rows(result.rows, [[text], [null], [b"\x00\xff"], [b""], [""], [1]])
    assert sqlite_rows(db.query("SELECT ?2, ?2, ?1", [11, 22])?.rows, [[22, 22, 11]])
    assert db.query("SELECT ?", []) is Err(SqliteError.InvalidArgument {..})
    assert db.query("SELECT 1", [1]) is Err(SqliteError.InvalidArgument {..})
    assert db.query("SELECT :named", [1]) is Err(SqliteError.InvalidArgument {..})
  }
}

test test_sqlite_statement_checks_happen_before_execution {
  with db = sqlite.memory()? {
    let _ = db.execute("CREATE TABLE item (value INTEGER)")?
    assert db.execute("INSERT INTO item VALUES (1) RETURNING value") is Err(SqliteError.InvalidArgument {..})
    assert db.query("SELECT value FROM item")?.rows == []
    assert sqlite_rows(db.query("INSERT INTO item VALUES (2), (3) RETURNING value")?.rows, [[2], [3]])
    assert db.execute("DELETE FROM item; DELETE FROM item") is Err(_)
    assert sqlite_rows(db.query("SELECT value FROM item ORDER BY value; -- trailing comment\n")?.rows, [[2], [3]])
  }
}

test test_sqlite_transactions_and_aliases {
  with db = sqlite.memory()? {
    let _ = db.execute("CREATE TABLE item (value INTEGER)")?
    assert ! db.in_transaction()?
    db.begin(mode: .Immediate)?
    assert db.in_transaction()?
    assert db.begin() is Err(_)
    let _ = db.execute("INSERT INTO item VALUES (1)")?
    db.rollback()?
    assert ! db.in_transaction()?
    assert db.query("SELECT value FROM item")?.rows == []
    db.begin()?
    let _ = db.execute("INSERT INTO item VALUES (2)")?
    db.commit()?
    assert sqlite_rows(db.query("SELECT value FROM item")?.rows, [[2]])
    let alias = db
    db.close()?
    assert alias.query("SELECT 1") is Err(SqliteError.Closed {..})
  }
}

test test_sqlite_persists_and_rolls_back_scope { |ctx|
  let dir = test.temp_dir(ctx)?
  let file = fp"{dir}/state.sqlite"
  with db = sqlite.open(file)? {
    let _ = db.execute("CREATE TABLE item (value INTEGER)")?
    let _ = db.execute("INSERT INTO item VALUES (1)")?
    db.begin()?
    let _ = db.execute("INSERT INTO item VALUES (2)")?
  }
  with db = sqlite.open(file, mode: .ReadWrite)? {
    assert sqlite_rows(db.query("SELECT value FROM item")?.rows, [[1]])
  }
  with db = sqlite.open(file, mode: .ReadOnly)? {
    assert db.execute("INSERT INTO item VALUES (3)") is Err(SqliteError.Open {..})
  }
  assert sqlite.open(fp"{dir}/missing", mode: .ReadOnly) is Err(SqliteError.Open {..})
}

test test_sqlite_failed_commit_keeps_transaction_open {
  with db = sqlite.memory()? {
    let _ = db.execute("CREATE TABLE parent (id INTEGER PRIMARY KEY)")?
    let _ = db.execute("CREATE TABLE child (id INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED)")?
    db.begin()?
    let _ = db.execute("INSERT INTO child VALUES (1)")?
    assert db.commit() is Err(SqliteError.Constraint {..})
    assert db.in_transaction()?
    db.rollback()?
    assert ! db.in_transaction()?
  }
}

test test_sqlite_closed_scope_and_external_process { |ctx|
  let dir = test.temp_dir(ctx)?
  let file = fp"{dir}/state.sqlite"
  let stale = with db = sqlite.open(file)? {
    let _ = db.execute("CREATE TABLE item (value INTEGER)")?
    run true
    let _ = db.execute("INSERT INTO item VALUES (7)")?
    db
  }?
  assert stale.query("SELECT 1") is Err(SqliteError.Closed {..})
  with db = sqlite.open(file)? {
    assert sqlite_rows(db.query("SELECT value FROM item")?.rows, [[7]])
    assert stale.query("SELECT 1") is Err(SqliteError.Closed {..})
  }
}

test test_sqlite_write_lock_timeout_and_recovery { |ctx|
  let dir = test.temp_dir(ctx)?
  let file = fp"{dir}/state.sqlite"
  with first = sqlite.open(file)? {
    let _ = first.execute("CREATE TABLE item (value INTEGER)")?
    with second = sqlite.open(file, busy_timeout: 0ms)? {
      first.begin(mode: .Immediate)?
      assert second.begin(mode: .Immediate) is Err(SqliteError.Busy {..})
      first.rollback()?
      second.begin(mode: .Immediate)?
      let _ = second.execute("INSERT INTO item VALUES (9)")?
      second.commit()?
      assert sqlite_rows(first.query("SELECT value FROM item")?.rows, [[9]])
    }
  }
}

test test_sqlite_caught_error_does_not_abort_transaction {
  with db = sqlite.memory()? {
    let _ = db.execute("CREATE TABLE item (value INTEGER UNIQUE)")?
    db.begin()?
    let _ = db.execute("INSERT INTO item VALUES (1)")?
    assert db.execute("INSERT INTO item VALUES (1)") is Err(SqliteError.Constraint {..})
    assert db.in_transaction()?
    let _ = db.execute("INSERT INTO item VALUES (2)")?
    db.commit()?
    assert sqlite_rows(db.query("SELECT value FROM item ORDER BY value")?.rows, [[1], [2]])
  }
}

proc sqlite_early_return(file: Path) [fs, error] -> Result[Int] {
  with db = sqlite.open(file)? {
    db.begin()?
    let _ = db.execute("INSERT INTO item VALUES (8)")?
    return 8
  }
}

test test_sqlite_early_return_rolls_back { |ctx|
  let dir = test.temp_dir(ctx)?
  let file = fp"{dir}/state.sqlite"
  with db = sqlite.open(file)? {
    let _ = db.execute("CREATE TABLE item (value INTEGER)")?
  }
  assert sqlite_early_return(file)? == 8
  with db = sqlite.open(file)? {
    assert db.query("SELECT value FROM item")?.rows == []
  }
}

test test_sqlite_result_limit_and_deadline {
  with db = sqlite.memory()? {
    assert db.query("SELECT zeroblob(16777217)") is Err(SqliteError.Limit {..})
    let interrupted = within 50ms {
      db.query("WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x + 1 FROM n WHERE x < 1000000000) SELECT sum(x) FROM n")?
    }
    assert interrupted is Err(_)
    assert sqlite_rows(db.query("SELECT 1")?.rows, [[1]])
  }
}

test test_sqlite_effects_and_argument_checking { |ctx|
  let memory = test.run_script(ctx, "pure probe() { let _ = sqlite.memory() }\n")?
  assert ! memory.success
  let method = test.run_script(ctx, "proc probe(db: SqliteDb) [error] { let _ = db.query(\"SELECT 1\") }\n")?
  assert ! method.success
  assert "requires the `fs` effect" in method.stderr
  let invalid = test.run_script(ctx, "proc probe(db: SqliteDb) [fs, error] { let _ = db.execute(\"SELECT ?\", [{value: 1}]) }\n")?
  assert ! invalid.success
}

test test_sqlite_scalar_boundaries_and_embedded_nul {
  with db = sqlite.memory()? {
    let cells = db.query("SELECT ?1, ?2, ?3, ?4, ?5", [-9223372036854775807 - 1, 9223372036854775807, 1.25, "雪\0text", b"\0\xff"])?
    assert cells.rows.len() == 1
    assert cells.rows[0][0] == -9223372036854775807 - 1
    assert cells.rows[0][1] == 9223372036854775807
    assert cells.rows[0][2] == 1.25
    assert cells.rows[0][3] == "雪\0text"
    assert cells.rows[0][4] == b"\0\xff"
    assert db.query("SELECT ?3", [1, 2, 3])?.rows[0][0] == 3
  }
}

proc sqlite_foreign() [fs, error] -> Result[SqliteDb] {
  let databases = [1, 2] |> par-map(jobs: 2) { |_| sqlite.memory()? }
  databases[0]
}

test test_sqlite_rejects_foreign_evaluator_handle {
  let foreign = sqlite_foreign()?
  with local = sqlite.memory()? {
    assert foreign.query("SELECT 1") is Err(SqliteError.Closed {..})
    assert local.query("SELECT 1")?.rows[0][0] == 1
  }
}

test test_sqlite_file_paths_follow_scoped_working_directory { |ctx|
  let dir = test.temp_dir(ctx)?
  cd (dir) {
    with db = sqlite.open(p"relative.sqlite")? {
      let _ = db.execute("CREATE TABLE item (value INTEGER)")?
    }
    if system.uname()?.sysname == "Linux" {
      let raw = Path.parse_bytes(b"db-\xff.sqlite")?
      with db = sqlite.open(raw)? {
        let _ = db.execute("CREATE TABLE marker (value INTEGER)")?
      }
      with db = sqlite.open(raw, mode: .ReadWrite)? {
        assert db.query("SELECT value FROM marker")?.rows == []
      }
    }
  }
  assert fp"{dir}/relative.sqlite".exists()?
  with db = sqlite.open(fp"{dir}/relative.sqlite", mode: .ReadWrite)? {
    assert db.query("SELECT value FROM item")?.rows == []
  }
}

test test_sqlite_trace_names_operation_without_bound_values { |ctx|
  let trace = test.run_xsht_trace(ctx, """
let db = sqlite.memory()?
let _ = db.query("SELECT ?", ["sqlite-secret-972"])?
db.close()?
""", ["--raw", "--trace-format", "jsonl"])?
  assert trace.success, trace.stderr
  assert "SqliteDb.query" in trace.stderr
  assert "sqlite-secret-972" not in trace.stderr
}
