## Facile against Vitesse, in turn

The fairest comparison: both servers run together and are measured **in turn** (`bench/ab.sh`), seven times each, so that the machine's variations touch both alike. The median is kept.

| Scenario | Facile | Vitesse | Difference |
|---|--:|--:|--:|
| `GET /` (text) | 182,211 | 176,846 | +3.0% |
| `GET /json` | 185,985 | 184,331 | +0.9% |
| `GET /json`, 12 browser headers | 165,742 | 172,452 | −3.9% |
| `GET /users/:id` (parameter + JSON) | 193,260 | 184,753 | **+4.6%** |
| `POST /echo` (JSON read and sent back) | 151,567 | 179,761 | −15.7% |
| `GET /` pipelined ×16 | 1,834,231 | 1,817,494 | +0.9% |

Requests per second. Facile and Vitesse are neck and neck: since November 2026 Vitesse has an engine per thread like Facile's (one epoll per core, no task per connection, a short spin before sleeping), and the differences on four scenarios are within the noise of the machine (5%). Facile leads on `/users/:id` thanks to its cache of pure responses (the response is kept, the handler does not run again); Vitesse leads with a browser's request, and clearly on the JSON echo, where each request really goes through the Rythme interpreter (reading the JSON into values, calling the handler, writing the JSON).

## Every server

The same scenarios, each server measured alone, one after the other (`bench/run.sh`), median of three runs. Minutes pass from one server to the next: differences below 8% between Facile and Vitesse are within the machine's noise, hence the table above. The differences with actix-web and axum are clear.

| Scenario | Facile | Facile without cache | Vitesse | actix-web | axum |
|---|--:|--:|--:|--:|--:|
| `GET /` | 179,063 | 180,142 | 175,956 | 174,475 | 104,439 |
| `GET /json` | 180,760 | 174,481 | 173,191 | 152,498 | 101,206 |
| `GET /json`, browser | 174,346 | 153,930 | 165,337 | 138,917 | 86,681 |
| `GET /users/:id` | 192,112 | 166,986 | 178,731 | 152,877 | 96,321 |
| `POST /echo` | 142,811 | 155,466 | 171,438 | 115,069 | 81,171 |
| `GET /` pipelined ×16 | 1,977,558 | 1,300,910 | 1,770,173 | 983,771 | 171,985 |

Measured on November 3, 2026, in the same session as the table above, on a slower virtual machine than the previous measures (compare the columns, not with older numbers). Without its cache, Facile runs every handler in the Rythme interpreter: Vitesse, compiled Rust, is then ahead on four scenarios out of six.

## What the cache does

Without pipelining, all these servers hit the same floor: about 5 µs of kernel work per request (reading, writing, the loopback). The cache of pure responses hardly changes the throughput of lone requests — it shows as soon as that floor is shared: pipelined, **2.0 million requests per second with the cache, 1.3 million without**. In the process, a kept response costs about **73 ns** of CPU, against about 1 µs when the Rythme handler runs.

## The method

- A 4 vCPU virtual machine; the server pinned on 2 cores (`taskset -c 0,1`), wrk on the other 2; 128 keep-alive connections; 4 to 5 seconds per run.
- The servers of Vitesse, actix-web and axum are those of Vitesse's `bench/` folder; Facile's are the same routes written in Rythme (`bench/app.ry`).
- The server's CPU time is read in `/proc` during the run.
- To reproduce: `bench/run.sh` (all) and `bench/ab.sh` (Facile against Vitesse, in turn).

## Why it is fast

- **One thread per core**, each with its event loop (epoll), its socket (`SO_REUSEPORT`) and its Rythme interpreter: no lock, no request changes threads.
- **No allocation** to read a request: httparse (SIMD) splits the head, headers stay positions in the buffer.
- **One write per batch**: the responses to pipelined requests leave together.
- **A short wait** before sleeping: after a busy turn, the loop looks a few microseconds more for work, which costs less than waking a sleeping thread.
- **The clock read once per turn**, the `Date` header written once a second.
- **mimalloc**, and Rythme values built without detours (JSON read straight into records, shared shapes).
