# ExSim — Binance Futures Subset Gateway

Un gateway REST/WS compatible (subset) con **Binance USDⓈ-M Futures** para re‑producir datos históricos desde **DuckDB** y simular ejecución de órdenes (MARKET/LIMIT) con fees/slippage. Pensado para conectar tus **bots** sin cambiar su lógica.

---

## TL;DR

```bash
# 1) Ejecutar el gateway

source .venv/bin/activate     # <- esto “entra” al venv

python -m gateway.main \
  --duckdb-path data/duckdb/exsim.duckdb \
  --symbol BTCUSDT --interval 1m \
  --start 2024-07-01 --end 2024-07-05 \
  --speed 25 --maker-bps 1 --taker-bps 2 --port 9001 --reload

# 2) Probar docs
open http://localhost:9001/docs

# 3) Conectar tus bots
export BINANCE_BASE_URL=http://localhost:9001
export BINANCE_WS_BASE=ws://localhost:9001
```

> **Swagger**: `/docs` · **ReDoc**: `/redoc` · **Rutas**: `/__routes`

---

## Estructura

```
gateway/
├─ main.py            # CLI → uvicorn + create_app(cfg)
├─ app.py             # FastAPI factory + wiring de routers/WS
├─ api/
│  ├─ deps.py         # get_sim (inyecta estado)
│  ├─ routes_market.py   # /fapi/v1/time, /klines, /fundingRate, /premiumIndex
│  ├─ routes_meta.py     # /fapi/v1/exchangeInfo
│  ├─ routes_orders.py   # /fapi/v1/order, /openOrders, /allOpenOrders, /ticker/bookTicker
│  ├─ routes_account.py  # /fapi/v2/balance, /v2/positionRisk (+v1), leverage/margin/dual, listenKey
│  └─ routes_admin.py    # /admin/status, /admin/replay (sin auth)
├─ core/
│  ├─ models.py       # ReplayConfig, Order, Position, Account + helpers
│  ├─ state.py        # SimState (DuckDB, executor, WS, lifecycle)
│  ├─ executor.py     # Motor de órdenes, fees/slippage, fills
│  ├─ replayer.py     # Lectura de OHLC desde DuckDB, stream a N velas/seg
│  └─ store.py        # RunStore (runs, trades_fills, equity_curve)
├─ ws/
│  └─ stream.py       # WebSocket multiplexor (/stream y /ws/stream)
└─ utils/
   ├─ time.py         # now_ms, to_ms
   └─ binance.py      # helpers para payloads/URLs estilo Binance
```

---

## Configuración (CLI + ENV)

Parámetros principales (CLI):

* `--duckdb-path` (ruta a DB)
* `--symbol` (`BTCUSDT`, etc.)
* `--interval` (`1m`, `1h`, `4h`, `1d`)
* `--start`, `--end` (epoch ms/s, ISO 8601, o `YYYY-MM-DD`)
* `--speed` (velas por segundo)
* `--maker-bps`, `--taker-bps`, `--slippage-bps`
* `--starting-balance`
* `--host`, `--port`, `--reload`

Variables de entorno equivalentes (opcionales): `HOST`, `PORT`, `DUCKDB_PATH`, `SPEED`, `FEE_MAKER_BPS`, `FEE_TAKER_BPS`, `SLIPPAGE_BPS`, `STARTING_BALANCE`.

Para conectar **bots**:

```
BINANCE_BASE_URL=http://localhost:9001
BINANCE_WS_BASE=ws://localhost:9001
```

> Si tu cliente/SDK arma la URL a `/stream`, este gateway ofrece **/stream** y **/ws/stream** (alias) para compatibilidad.

---

## Endpoints (subset)

### Mercado

```
GET /fapi/v1/time                 → { serverTime }
GET /fapi/v1/exchangeInfo         → symbols, PRICE_FILTER.tickSize, LOT_SIZE.stepSize
GET /fapi/v1/klines               → [[openTime, open, high, low, close, volume, closeTime], ...]
GET /fapi/v1/fundingRate          → [{ fundingTime, fundingRate }]
GET /fapi/v1/premiumIndex         → { markPrice, lastFundingRate }
```

### Órdenes / libro (sim)

```
POST   /fapi/v1/order             → MARKET/LIMIT (acepta quantity u origQty; reduceOnly/stopPrice opcionales)
DELETE /fapi/v1/order             → cancela por orderId
DELETE /fapi/v1/allOpenOrders     → cancela todas por símbolo
GET    /fapi/v1/openOrders        → órdenes vivas
GET    /fapi/v1/ticker/bookTicker → bid/ask sintético
```

### Cuenta / riesgo

```
GET  /fapi/v2/balance
GET  /fapi/v2/positionRisk  (alias /fapi/v1/positionRisk)
POST /fapi/v1/leverage
POST /fapi/v1/marginType
POST /fapi/v1/positionSide/dual
POST /fapi/v1/listenKey
```

### WebSocket

```
/ws/stream?streams=btcusdt@kline_1m/btcusdt@markPrice@1s
/stream?streams=...              # alias (recomendado; igual que Binance)

Eventos emitidos:
- { stream: "btcusdt@kline_1m", data: { e:"kline", k:{ i:"1m", x:true, ... } } }
- { stream: "btcusdt@markPrice@1s", data: { e:"markPriceUpdate", p:"..." } }
```

---

## Persistencia en DuckDB

Cada ejecución crea un **run** y persiste:

* `runs(run_id, strategy, params_json, ...)`
* `trades_fills(run_id, seq, ts, symbol, side, price, qty, realized_pnl, fee, is_maker)`
* `equity_curve(run_id, ts, equity)`

> Se asume que el esquema existe (creado por `scripts.init_duckdb`). `store.ensure_min_schema()` puede crearlo mínimamente para pruebas.

---

## Notas de simulación

* **Fees**: `maker_bps`/`taker_bps` (por defecto 2/4 bps).
* **Slippage**: aplicado a órdenes **MARKET** (en contra).
* **LIMIT**: ejecutadas en cierre de barra si `low <= price` (BUY) o `high >= price` (SELL).
* **Hedge/Dual**: expone endpoints para modo dual/leverage/margin pero el modelo interno es single‑position por símbolo (simplificado). Para backtests de señales funciona bien.
* **Compatibilidad**: el body acepta `quantity` (como Binance) o `origQty` (compat extra), y parámetros `reduceOnly`/`stopPrice`.

---

## Smoke tests

```bash
# server time
curl -s http://localhost:9001/fapi/v1/time

# exchangeInfo (tick/step)
curl -s http://localhost:9001/fapi/v1/exchangeInfo | jq '.symbols[] | select(.symbol=="BTCUSDT")'

# crear LIMIT
curl -s -X POST http://localhost:9001/fapi/v1/order \
  -H 'Content-Type: application/json' \
  -d '{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"GTC","quantity":"0.001","price":"50000","newClientOrderId":"readme-limit"}'

# open orders
curl -s 'http://localhost:9001/fapi/v1/openOrders?symbol=BTCUSDT'
```

---

## Postman

Colección preparada (importar en Postman):

* **exsim\_postman\_collection.json** → [Descargar](sandbox:/mnt/data/exsim_postman_collection.json)

---

## Seguridad

Este gateway es **para desarrollo/backtesting**. No implementa autenticación ni firma; CORS abierto.

---

## Roadmap breve

* Soportar más tipos de orden (STOP/STOP\_LIMIT, reduceOnly completo)
* WS de user‑data (`executionReport`) mockeado
* Mejorar modelo de posición a dual‑side real por símbolo
* Validaciones por tickSize/stepSize por símbolo
