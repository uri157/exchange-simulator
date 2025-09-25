# Exchange Simulator (Rust)

A fast, deterministic **Binance-compatible simulator** to backtest and replay strategies over historical data—**so your bot can talk to it as if it were Binance**.
It serves a **Binance-style HTTP API** and a **WebSocket** for klines, reads market data from **DuckDB**, and simulates **MARKET/LIMIT** orders with **per-session accounts**.

> **Goal:** run bots against years of data at different speeds, reproduce scenarios exactly, and validate strategies **without touching real Binance**.

---

## What you can do

* **Replay** historical markets per session (start / pause / resume / seek) at configurable speed.
* **Stream klines** over WebSocket and **query klines** over HTTP from a local DuckDB.
* **Create sessions** for isolated tests (each with its own clock and account).
* **Simulate orders** and observe balances & fills—no exchange required.
* Optionally use a **Binance proxy** to list live symbols, then **register & ingest** datasets into DuckDB.

---

## Architecture at a glance

* **API & WS:** Axum (Rust) with Binance-like routes + `kline@<interval>:<symbol>` stream.
* **Services:** replay engine, market queries, orders, sessions.
* **Domain (hexagonal):** exchange logic isolated from adapters.
* **Infra:** DuckDB (datasets), in-memory repos (sessions/orders/accounts), WS broadcaster.
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

> Defaults are sensible. If you need to change ports/paths later, you can add a `.env` with `PORT`, `DATA_DIR`, `DUCKDB_PATH`, etc.

---

## Speak “Binance” (API compatibility)

Your bot should feel at home. Typical endpoints:

* **Symbols (local):**
  `GET /api/v1/exchangeInfo`
* **Klines (from DuckDB):**
  `GET /api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=...&endTime=...&limit=...`
  *(an alias exists at `/api/v3/klines`)*
* **Sessions (replay control):**
  `POST /api/v1/sessions` → create • `GET /api/v1/sessions` → list •
  `POST /api/v1/sessions/{id}/start|pause|resume|seek`
* **Orders & account (simulated):**
  `POST /api/v3/order` • `GET /api/v3/account?sessionId=...` • `GET /api/v3/openOrders` • `GET /api/v3/order` • `DELETE /api/v3/order`
* **WebSocket (klines):**
  `ws://localhost:3001/ws?sessionId=<uuid>&streams=kline@1m:ETHBTC`
  Messages look like Binance’s `kline` payloads (symbol, interval, OHLCV, open/close time).

> **OpenAPI JSON** at `/api-docs/openapi.json` to explore all routes.

---

## Datasets & (optional) Binance proxy

* **Binance proxy (optional):** list pairs (e.g., all `USDT` quotes) to help you decide what to ingest.
* **Dataset registration:** add a dataset (CSV/Parquet) with name/symbol/interval/path.
* **Ingestion:** load it into DuckDB. From there, **all kline queries and replays are served locally**.

A typical flow is:

1. Discover a symbol/interval (via proxy or your files).
2. **Register dataset** → **Ingest** → **Create session** → **Start replay**.
3. Point your bot to the simulator’s HTTP/WS and run your strategy.

---

## Order simulation rules (simple & deterministic)

* **MARKET** fills at the **last close** of the current kline.
* **LIMIT**

  * **BUY** fills if `limit ≥ low` of the current kline.
  * **SELL** fills if `limit ≤ high` of the current kline.
* No fees or partial fills yet (kept simple on purpose, easy to extend).

Each session maintains its **own account** (initialized on demand with a default quote & balance) and **its own clock** (monotonic; never goes backward), so runs are repeatable.

---

## Typical workflow

1. **Start the server** (`cargo run`).
2. **(Optional) Ingest data** into DuckDB (symbols, intervals, date ranges).
3. **Create a session** with symbol, interval, and time range.
4. **Connect WS** to receive klines while the session runs, and **send orders** over REST.
5. **Adjust speed / seek / pause / resume** to test edge cases and latency sensitivity.
6. Inspect results via **account & orders** endpoints or your bot’s logs.

---

## Troubleshooting

* **No kline stream:** ensure the session’s time range actually has data; check WS query (`sessionId` + correct `streams=` format).
* **404 on klines:** both `/api/v1/market/klines` and `/api/v3/klines` exist—verify parameters.
* **WS 1011 / keep-alive:** usually comes from the **client** (its ping/timeout). The server does not drop idle connections by itself.

---

## Roadmap

* Persist sessions/orders/accounts in DuckDB (replace in-memory repos).
* Symbol filters (tick size / lot size) for order validation.
* Metrics (Prometheus) and traces (OpenTelemetry).
* WS stats (active clients per session).

---

## License

Educational/demo project. **Not** financial advice. Use at your own risk.
