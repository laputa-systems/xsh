let entries = [{level: "INFO", timestamp: "2026-01-01"}, {level: "ERROR", timestamp: "2026-01-02"}]
let has_errors = entries |> any .level == "ERROR"
let all_timestamped = entries |> all .timestamp != ""
let has_debug = entries |> any .level == "DEBUG"
print f"${has_errors} ${all_timestamped} ${has_debug}"
