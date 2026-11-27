#!/usr/bin/env -S xsh --

cli main(database = p"settings.sqlite") {
  with db = sqlite.open(database)? {
    let _ = db.execute("CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL)")?
    db.begin(mode: .Immediate)?
    let changed = db.execute(
      "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
      ["feature", "enabled"],
    )?
    db.commit()?
    print f"{changed} rows updated"

    db.begin()?
    let _ = db.execute("UPDATE settings SET value = ? WHERE key = ?", ["temporary", "feature"])?
    db.rollback()?

    let settings = db.query("SELECT key, value FROM settings ORDER BY key")?
    for row in settings.rows {
      let key = row[0]
      let value = row[1]
      if key != null and key is Str and value != null and value is Str {
        print f"{key} = {value}"
      } else {
        fail "settings contains a non-text value"
      }
    }
  }
}
