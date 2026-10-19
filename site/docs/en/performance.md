## Facile against Vitesse, in turn

The fairest comparison: both servers run together and are measured **in turn** (`bench/ab.sh`), seven times each, so that the machine's variations touch both alike. The median is kept.

| Scenario | Facile | Vitesse | Difference |
|---|--:|--:|--:|
| `GET /` (text) | 267,163 | 256,532 | **+4.1%** |
| `GET /json` | 274,179 | 259,325 | **+5.7%** |
| `GET /json`, 12 browser headers | 267,658 | 249,391 | **+7.3%** |
| `GET /users/:id` (parameter + JSON) | 276,757 | 265,733 | **+4.1%** |
| `POST /echo` (JSON read and sent back) | 250,942 | 262,727 | −4.5% |
| `GET /` pipelined ×16 | 2,702,136 | 2,607,071 | **+3.6%** |

Requests per second. Facile leads on five scenarios out of six. On the JSON echo, each request really goes through the Rythme interpreter (reading the JSON into values, calling the handler, writing the JSON): Vitesse, compiled Rust, keeps the lead. With 32 connections, the lead on `GET`s grows to +12% and +21%: Facile's engine answers sooner when the machine is not saturated.

## Every server

The same scenarios, each server measured alone, one after the other (`bench/run.sh`), median of three runs. Minutes pass from one server to the next: differences below 8% between Facile and Vitesse are within the machine's noise, hence the table above. The differences with actix-web and axum are clear.

| Scenario | Facile | Facile without cache | Vitesse | actix-web | axum |
|---|--:|--:|--:|--:|--:|
| `GET /` | 264,688 | 276,904 | 262,704 | 248,508 | 191,135 |
| `GET /json` | 251,993 | 274,725 | 252,254 | 249,498 | 175,985 |
| `GET /json`, browser | 237,712 | 250,867 | 252,333 | 191,140 | 142,658 |
| `GET /users/:id` | 267,607 | 271,150 | 272,276 | 224,220 | 166,449 |
| `POST /echo` | 233,792 | 236,361 | 256,236 | 175,884 | 129,084 |
| `GET /` pipelined ×16 | 2,638,009 | 2,128,851 | 2,682,204 | 1,274,073 | 254,865 |

## What the cache does

Without pipelining, all these servers hit the same floor: about 5 µs of kernel work per request (reading, writing, the loopback). The cache of pure responses hardly changes the throughput of lone requests — it shows as soon as that floor is shared: pipelined, **2.7 million requests per second with the cache, 2.1 million without**. In the process, a kept response costs about **73 ns** of CPU, against about 1 µs when the Rythme handler runs.

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
