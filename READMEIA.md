# Exchange Simulator (Rust) — README for AI

Exchange simulator for **replay/backtesting** with historical data in **DuckDB**, **simulated orders** (market/limit), **per-session accounts**, **Binance-style HTTP API**, and **kline WebSocket**.

> **Goal:** run bots against years of data at different speeds, reproduce deterministic scenarios, and validate strategies without touching real Binance.

---

## TL;DR (to start right now)

* **Exposed WS:** `GET /ws?sessionId=<uuid>&streams=kline@1m:ETHBTC`
* **Key REST:**

  * `GET /api/v1/exchangeInfo` (local symbols)
  * `GET /api/v1/market/klines` (**or** `GET /api/v3/klines`) — klines from DuckDB
  * Sessions: `POST /api/v1/sessions`, `GET /api/v1/sessions`, `POST /{id}/start|pause|resume|seek`
  * Orders (simulated): `POST /api/v3/order`, `GET /api/v3/account`, etc.
* **OpenAPI JSON:** `/api-docs/openapi.json` (the Swagger UI lives in the **front-end**)
* **Current persistence:** DuckDB for market data; sessions/orders/accounts in **memory**
* **Simple matching:** MARKET fills at last close; LIMIT fills if it crosses the current kline OHLC
* **CORS:** permissive in dev

---

## Architecture (mental map)

* **domain/**: models and ports (traits)
* **services/**: use cases (market, sessions, replay, orders)
* **infra/**: adapters (DuckDB, clock, in-memory repos, WS broadcaster)
* **api/**: Axum controllers (HTTP/WS)
* **dto/**: request/response contracts (serde + utoipa)
* **app/**: wiring/injection and router (CORS/Trace/OpenAPI)
* **oas.rs**: OpenAPI definition

```
src/
├─ api/v1/ (market.rs, sessions.rs, orders.rs, datasets.rs, ws.rs)
├─ app/ (bootstrap.rs, router.rs)
├─ domain/ (models.rs, traits.rs, value_objects.rs)
├─ dto/ (market.rs, sessions.rs, orders.rs, datasets.rs, ws.rs)
├─ infra/ (duckdb/, ws/, clock.rs, repos/)
├─ services/ (market_service.rs, replay_service.rs, ...)
└─ oas.rs
```

---

## Configuration

Create `.env` at the repo root (names expected by `infra/config.rs`):

```env
# HTTP
PORT=3001

# Data
DATA_DIR=./data
DUCKDB_PATH=./data/market.duckdb

# WebSocket
WS_BUFFER=1024

# Clock / sessions
DEFAULT_SPEED=1.0
MAX_SESSION_CLIENTS=100

# Simulated accounts
DEFAULT_QUOTE=USDT
INITIAL_QUOTE_BALANCE=10000
```

---

## Run

```bash
# Build + run
cargo run

# With useful logs
RUST_LOG=info,tower_http=info,exchange_simulator=debug cargo run
```

Expected output:

```
INFO opening DuckDB duckdb_path=.../data/market.duckdb
INFO duckdb warmup datasets=... klines=... symbols=...
INFO starting exchange simulator server addr=0.0.0.0:3001
```

---

## Market API (local/DuckDB)

### Exchange info (symbols)

```
GET /api/v1/exchangeInfo
```

### Klines from DuckDB

Two equivalent routes:

```
GET /api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=1757833200000&endTime=1757839200000&limit=1000
GET /api/v3/klines?symbol=ETHBTC&interval=1m&startTime=...&endTime=...&limit=...
```

* `startTime/endTime` in **ms**
* `interval` uses domain values (e.g., `1m`, `1h`, `1d`)

---

## Dataset management

```
POST /api/v1/datasets                 # register {name, path, format: csv|parquet}
POST /api/v1/datasets/{id}/ingest     # ingestion → klines + symbols
GET  /api/v1/datasets                 # list
GET  /api/v1/datasets/symbols         # available symbols
GET  /api/v1/datasets/{symbol}/intervals
GET  /api/v1/datasets/{symbol}/{interval}/range  # { firstOpenTime, lastCloseTime }
```

---

## Sessions (replay)

```
POST /api/v1/sessions                 # create {symbols[], interval, startTime, endTime, speed, seed?}
GET  /api/v1/sessions                 # list
GET  /api/v1/sessions/{id}            # status
POST /api/v1/sessions/{id}/start
POST /api/v1/sessions/{id}/pause
POST /api/v1/sessions/{id}/resume
POST /api/v1/sessions/{id}/seek?to=<ms>
```

**Notes:**

* Replay emits candles in order and respects `[startTime, endTime]`.
* You can **connect WS** before `start`; the server **does not** close for idleness.

---

## WebSocket

**Route:**

```
GET /ws?sessionId=<uuid>&streams=<streams>
```

* `streams`: currently `kline@<interval>:<symbol>` (e.g., `kline@1m:ETHBTC`)
* Multiple connections per session: OK (buffer configurable via `WS_BUFFER`).

**Message (kline)**

```json
{
  "event": "kline",
  "data": {
    "symbol": "ETHBTC",
    "interval": "1m",
    "openTime": 1758150240000,
    "closeTime": 1758150299999,
    "open": 0.03942,
    "high": 0.03946,
    "low": 0.03942,
    "close": 0.03946,
    "volume": 66.5555
  },
  "stream": "kline@1m:ETHBTC"
}
```

> **Keepalive:** the server **does not** enforce a timeout; if you see `1011 keepalive ping timeout` it usually comes from the **client** (e.g., a Python script). Modern browsers/WS clients shouldn’t disconnect.

---

## Orders & accounts (simulated)

* **Orders**

  ```
  POST /api/v3/order
  ```

  * MARKET: fills at the last `close`
  * LIMIT:

    * BUY: fills if `limit >= low` of the current kline
    * SELL: fills if `limit <= high` of the current kline

* **Queries & account**

  ```
  GET    /api/v3/order
  DELETE /api/v3/order
  GET    /api/v3/openOrders
  GET    /api/v3/myTrades
  GET    /api/v3/account?sessionId=...
  ```

  * The account is initialized on-demand with `DEFAULT_QUOTE` and `INITIAL_QUOTE_BALANCE`.

> **Persistence:** currently **in-memory** (restart wipes). DuckDB stores datasets/klines/symbols only.

---

## OpenAPI

* **JSON**: `GET /api-docs/openapi.json`
  The **front-end** renders the UI (Swagger) consuming this JSON.

---

## cURL recipes (quick sanity)

```bash
# What symbols/intervals/range do I have?
curl -s http://localhost:3001/api/v1/datasets/symbols | jq
curl -s http://localhost:3001/api/v1/datasets/ETHBTC/intervals | jq
curl -s http://localhost:3001/api/v1/datasets/ETHBTC/1m/range | jq

# 10 candles of 1m
START=1757833200000
END=$(( START + 60000*10 ))
curl -s "http://localhost:3001/api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=$START&endTime=$END&limit=1000" | jq length
curl -s "http://localhost:3001/api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=$START&endTime=$END&limit=1" | jq '.[0]'

# Create session + start
curl -sS -X POST http://localhost:3001/api/v1/sessions \
  -H 'content-type: application/json' \
  -d '{"symbols":["ETHBTC"],"interval":"1m","startTime":'$START',"endTime":'$END',"speed":1.0}' | tee sess.json
SESS_ID=$(jq -r .sessionId sess.json)
curl -sS -X POST "http://localhost:3001/api/v1/sessions/$SESS_ID/start"

# Connect WS (manual inspection with websocat)
# websocat "ws://localhost:3001/ws?sessionId=$SESS_ID&streams=kline@1m:ETHBTC"
```

---

## Design choices & limits (for AI)

* **Design:** hexagonal (independent domain), Axum as controller, services orchestrate repos/infra.
* **Simulated clock** per session (pause/resume/seek) used by replay/orders.
* **Monotonicity** of `closeTime` in replay; the clock never goes backward.
* **No fees or partial fills** today; the model is ready to extend.
* **WS closing:** the broadcaster only shuts down when the session is deleted or explicitly closed.

---

## Typical extensions

* Persist **sessions/orders/accounts** in DuckDB (migrate in-memory repos).
* Add **WS stats** (active connections per session) every N seconds.
* Validate orders against **lot sizes/tick sizes** per symbol.
* Prometheus metrics and OpenTelemetry traces.

---

## Troubleshooting

* **No candles on WS:**

  * Check that the session’s **range** has data (`/datasets/{symbol}/{interval}/range`).
  * Confirm the client uses `GET /ws` with `sessionId` and `streams`.
  * Enable logs: `RUST_LOG=info,tower_http=info,exchange_simulator=debug`
* **`/api/v1/market/klines` 404:**

  * Ensure you’re hitting that route (there’s also `/api/v3/klines`).
* **1011 close:**

  * Usually from the client (keepalive). The server doesn’t drop for idle.

---

## Versions (typical Cargo)

* `axum = 0.7.x`, `tower-http = 0.6.x`
* `utoipa = 4.x`
* `duckdb = 0.9.x` (**bundled**)
* `tokio = 1.x`, `serde = 1.x`, `uuid = 1.x`
* `chrono = "=0.4.31"` (to avoid Arrow conflicts)
