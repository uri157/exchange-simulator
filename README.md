Perfect—based on your audit, here’s a **corrected, human-oriented README** that fixes the two issues:

* Sessions are **persisted in DuckDB** (only orders & accounts are in-memory).
* The WS kline payload is **custom (event/data/stream)**, i.e., **inspired by** Binance, not byte-compatible.

You can replace your current README with this.

---

# Exchange Simulator (Rust)

A fast, deterministic **Binance-like simulator** to backtest and replay strategies over historical data—so your bot can talk to it almost like it were Binance.
It serves a **Binance-style HTTP API** and a **WebSocket** for klines, reads market data (and **sessions**) from **DuckDB**, and simulates **MARKET/LIMIT** orders with **per-session accounts**.

> Goal: run bots against years of data at different speeds, reproduce scenarios exactly, and validate strategies **without touching real Binance**.

---

## What you can do

* **Replay** historical markets per session (start / pause / resume / seek) at configurable speed.
* **Stream klines** over WebSocket and **query klines** over HTTP from a local DuckDB.
* **Create sessions** for isolated tests (each with its own clock and account).
* **Simulate orders** and observe balances & fills—no exchange required.
* Optionally use a **Binance proxy** to list symbols, then **register & ingest** datasets into DuckDB.

---

## Architecture at a glance

* **API & WS:** Axum (Rust) with Binance-inspired routes and a kline stream.
* **Services:** replay engine, market queries, orders, sessions.
* **Domain (hexagonal):** exchange logic isolated from adapters.
* **Infra:** **DuckDB for datasets and sessions**; in-memory repos for **orders and accounts**; WS broadcaster.
* **OpenAPI JSON:** available at `/api-docs/openapi.json` (Swagger UI can be served by a separate front).

---

## Quick start

1. **Requirements:** Rust (stable), `cargo`. DuckDB is embedded by the crate (no external service).
2. **Run:**

   ```bash
   cargo run
   ```

   You should see logs like:

   ```
   INFO opening DuckDB .../data/market.duckdb
   INFO starting exchange simulator server addr=0.0.0.0:3001
   ```

> Defaults are sensible. If you need to change ports/paths later, add a `.env` (e.g., `PORT`, `DATA_DIR`, `DUCKDB_PATH`).

---

## HTTP & WebSocket (Binance-inspired)

Typical endpoints:

* **Symbols (local):**
  `GET /api/v1/exchangeInfo`
* **Klines (served from DuckDB):**
  `GET /api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=...&endTime=...&limit=...`
  *(alias: `/api/v3/klines`)*
* **Sessions (replay control):**
  `POST /api/v1/sessions`, `GET /api/v1/sessions`, `GET /api/v1/sessions/{id}`,
  `POST /api/v1/sessions/{id}/start|pause|resume|seek`
* **Orders & account (simulated):**
  `POST /api/v3/order`, `GET /api/v3/account?sessionId=...`,
  `GET /api/v3/openOrders`, `GET /api/v3/order`, `DELETE /api/v3/order`
* **WebSocket (klines):**
  `ws://localhost:3001/ws?sessionId=<uuid>&streams=kline@1m:ETHBTC`

**WS payload format (custom, Binance-inspired):**

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

> Note: this is **not byte-compatible** with Binance’s official payload; structure is simplified (`event`/`data`/`stream`).

**OpenAPI JSON:** `GET /api-docs/openapi.json`

---

## Datasets & Binance proxy (optional)

* Use a **Binance proxy** (if enabled) to list symbols/pairs (e.g., all `USDT` quotes).
* **Register** a dataset (CSV/Parquet) and **ingest** it into DuckDB.
* From there, **all kline queries and replays are served locally**.

---

## Order simulation rules (deterministic)

* **MARKET** fills at the **last close** of the current kline.
* **LIMIT**

  * **BUY** fills if `limit ≥ low` of the current kline.
  * **SELL** fills if `limit ≤ high` of the current kline.
* No fees or partial fills yet (kept simple and extensible).

Each session has its **own clock** (monotonic: never goes backward) and **own account** (initialized on demand with default quote & balance).

---

## Troubleshooting

* **No kline stream:** verify the session’s time range has data and the WS query uses `sessionId` + `streams=kline@<interval>:<symbol>`.
* **404 on klines:** both `/api/v1/market/klines` and `/api/v3/klines` exist—check params.
* **WS 1011 / keep-alive:** commonly from the **client**; the server doesn’t drop idle connections by itself.

---

## Roadmap

* Persist **orders and accounts** in DuckDB (today only sessions are persisted).
* Symbol filters (tick size / lot size) for order validation.
* Metrics (Prometheus) and traces (OpenTelemetry).
* WS stats (active clients per session).

---

## License

Educational/demo project. **Not** financial advice. Use at your own risk.
