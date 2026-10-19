# ORM and databases

Facile's ORM describes data with **models**, looks for it with **queries** that are values, and reads or writes it with `db`. The same code runs on three databases, chosen by a URL.

| URL | Database |
|---|---|
| `memory://` (the default) | in memory, shared by the workers: development and tests |
| `sqlite://app.db` | SQLite, compiled into `facile`: nothing to install |
| `postgres://user:pass@host/db` | PostgreSQL, over TLS (Heroku Postgres, Neon, Supabase...) |

The URL comes from the `DATABASE_URL` variable, or from `database(app, "sqlite://app.db")`.

## Models

```rythme
projects = model("projects", {
  name: field(:text, {required: true, unique: true, max: 80}),
  tasks: has_many("tasks", :project),
})

tasks = model("tasks", {
  title: field(:text, {required: true, min: 2, max: 120}),
  done: field(:bool, {default: false}),
  priority: field(:int, {default: 1, min: 1, max: 5}),
  due: :date,
  tags: :json,
  email: field(:text, {email: true}),
  status: field(:text, {one_of: ["open", "closed"], default: "open"}),
  project: belongs_to("projects"),
})

app("Projects") |> models([projects, tasks])
```

Each table has an `id` (an integer made by the database) and, unless `model(..., {timestamps: false})`, `created_at` and `updated_at`.

| Kind | Rythme value |
|---|---|
| `:text`, `:int`, `:float`, `:bool` | Text, Int, Float, Bool |
| `:date` | a text `2026-10-19` |
| `:datetime` | a text `2026-10-19T08:30:00Z` |
| `:json` | any value (list, record...) |

The rules of `field`: `required`, `unique`, `default`, `min` and `max` (the value of a number, the length of a text), `email`, `one_of`, `index`. An empty field reads as `None`.

**Tables make themselves**: at startup, `facile` creates the tables of the models given to `models(...)` and adds the columns they lack. Table and field names are checked (lowercase, digits, `_`): they cannot carry SQL injection, and every value is sent as a parameter.

## Queries

A model is also the query of all its rows. Each function returns a new one:

```rythme
urgent = tasks
  |> where(:done, false)                         # equal
  |> where(:priority, ">=", 4)                   # =, !=, <, <=, >, >=
  |> where(:title, :contains, "milk")            # :contains, :starts_with, :ends_with (case ignored)
  |> where(:status, :in, ["open"])               # :in, :not_in
  |> where(:due, None)                           # IS NULL
  |> order_by(:due)                              # :asc by default, or :desc
  |> order_by(:priority, :desc)
  |> limit(20)
  |> offset(40)

either = tasks |> where_any([[:priority, 5], [:done, true]])           # one or the other
found = tasks |> search([:title, :email], "ada")                       # a text in one of the fields
light = tasks |> pick([:id, :title])                                   # only these fields
```

## Reading

```rythme
db.all(q)                   # the list of records
db.first(q)                 # Some(record) or None
db.find(tasks, 42)          # Some(record) or None
db.count(q)                 # an Int
db.exists(q)                # a Bool
db.sum(q, :priority)        # Some(n), or None without rows; also db.avg, db.min, db.max
db.count_by(q, :status)     # a Dict: value -> number of rows
db.paginate(q, 2, 20)       # {items, page, per_page, total, pages}
```

## Relations

```rythme
with_project = tasks |> include(:project) |> db.all      # each task has its `project` field: Some(...) or None
with_tasks = projects |> include(:tasks) |> db.all       # each project has the list of its tasks
```

`belongs_to("projects")` stores `project_id`; `has_many("tasks", :project)` reads the tasks whose `project_id` is the project's id. `include` makes **one more query**, not one per row.

## Writing

```rythme
match db.insert(tasks, {title: "Milk", project: home})
  Ok(task) -> json(task, 201)                            # with its id and its dates
  Err(e) -> json(e, e.status)                            # {status: 422, error, fields: {title: "..."}}
end

db.insert_all(tasks, [{title: "A"}, {title: "B"}])       # all or none
db.update(tasks, 42, {done: true})                       # Ok(record), Err(:not_found), Err({status: 422...})
db.save(tasks, task)                                     # inserts without an id, updates with one
db.update_where(tasks |> where(:done, true), {status: "closed"})   # Ok(number)
db.delete(tasks, 42)                                     # true if it existed
db.delete_where(tasks |> where(:status, "closed"))       # the number of rows
db.validate(tasks, {title: "x"})                         # checks without writing
```

Every write is checked by the rules of the fields; an unknown field is refused; a `unique` value already taken gives `422`.

## Transactions

```rythme
fn transfer(req) = db.transaction do
  db.update(accounts, 1, {balance: 50})?
  db.update(accounts, 2, {balance: 150})?
  Ok("done")
end
```

Everything the function writes is kept, unless it returns `Err(...)` or stops on an error: then **nothing** is kept. A transaction inside another becomes a savepoint.

## SQL written by hand

```rythme
db.sql("SELECT status, COUNT(*) AS n FROM tasks WHERE priority >= ? GROUP BY status", [3])
# Ok([{status: "open", n: 4}, ...]) or Err(message)
```

The `?` are parameters (Facile writes them `$1`, `$2`... for PostgreSQL).

## A REST API in one line

```rythme
app("API") |> resource("/api/tasks", tasks)
```

| Request | Effect |
|---|---|
| `GET /api/tasks` | the list; `?done=false` filters, `?q=milk` searches, `?order=-id` sorts, `?limit=10&offset=20`, or `?page=2&per_page=10` |
| `POST /api/tasks` | creates (JSON or form): `201`, or `422` with the errors |
| `GET /api/tasks/:id` | one row, or `404` |
| `PUT /api/tasks/:id` | replaces every field |
| `PATCH /api/tasks/:id` | changes the fields given |
| `DELETE /api/tasks/:id` | `204`, or `404` |

`resource(app, path, model, [:list, :show])` keeps only some actions. The middlewares of the application apply to these routes too.
