const host = "db-01"
const port = 5432
const load = 0.75
const grace = 90s
const conf = /etc/postgresql/postgresql.conf
const replicas = ["db-02", "db-03"]
const limits = {cpu: 2, memory_mb: 4096}
const owner: Str? = null
let data = fp"/srv/{host}/data"

print f"{host}:{port} load={load} grace={grace} doubled={grace * 2}"
print f"{conf.name()} in {conf.parent()} (.{conf.ext()})"
print f"data={data} replicas={replicas.join(",")}"
print f"cpu={limits.cpu} owner={owner ?? "nobody"}"
