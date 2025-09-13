# ExSim — Binance Futures‑Like Gateway & Simulator (Guía completa)

**ExSim** es un gateway **REST/WS** compatible (subset) con **Binance USDⓈ‑M Futures** que *reproduce* datos históricos almacenados en **DuckDB** y simula la **ejecución de órdenes** (MARKET/LIMIT/STOP\_MARKET) con **fees** y **slippage**. Su objetivo es que puedas conectar tus **bots reales** (o librerías/SDKs tipo Binance) **sin cambiar su lógica**, pero contra un entorno determinista y reproducible.

---

## Índice

* [Qué resuelve](#qué-resuelve)
* [Arquitectura y carpetas](#arquitectura-y-carpetas)
* [Modelo de simulación](#modelo-de-simulación)
* [Esquema DuckDB y persistencia](#esquema-duckdb-y-persistencia)
* [Instalación y datos](#instalación-y-datos)
* [Ejecutar el gateway (CLI + ENV)](#ejecutar-el-gateway-cli--env)
* [HTTP API (subset Binance)](#http-api-subset-binance)
* [WebSocket API](#websocket-api)
* [Admin API (replay y estado)](#admin-api-replay-y-estado)
* [Ejemplos rápidos (curl y Python)](#ejemplos-rápidos-curl-y-python)
* [Integración con bots (variables de entorno)](#integración-con-bots-variables-de-entorno)
* [Consultas útiles en SQL](#consultas-útiles-en-sql)
* [Troubleshooting](#troubleshooting)
* [Limitaciones y roadmap](#limitaciones-y-roadmap)

---

## Qué resuelve

* **Iteración rápida** de estrategias sin tocar un exchange real.
* **Reproducibilidad**: mismo histórico + mismas reglas ⇒ mismos resultados.
* **Observabilidad**: persiste **runs**, **fills** y **equity** en DuckDB.
* **Compatibilidad**: expone un subconjunto de endpoints y streams de Binance para re‑usar tus clientes/SDKs.

---

## Arquitectura y carpetas

```
gateway/
├─ main.py            # CLI → uvicorn + create_app(cfg)
├─ app.py             # FastAPI factory + wiring de routers/WS
├─ api/
│  ├─ deps.py         # get_sim (inyecta el estado SimState)
│  ├─ routes_market.py   # /fapi/v1/time, /klines, /fundingRate, /premiumIndex
│  ├─ routes_meta.py     # /fapi/v1/exchangeInfo (tickSize/stepSize)
│  ├─ routes_orders.py   # /fapi/v1/order, /openOrders, /allOpenOrders, bookTicker
│  ├─ routes_account.py  # balance/positionRisk (+ leverage/margin/dual)
│  └─ routes_admin.py    # /admin/status, /admin/replay (sin auth)
├─ core/
│  ├─ models.py       # ReplayConfig, Order, Position, Account
│  ├─ state.py        # SimState (conexión DuckDB, executor, WS, lifecycle)
│  ├─ executor.py     # Motor de órdenes (fills, fees, slippage)
│  ├─ replayer.py     # Lee OHLC de DuckDB y emite N velas/seg
│  └─ store.py        # RunStore (runs, trades_fills, equity_curve)
├─ ws/
│  └─ stream.py       # Multiplexor de streams tipo Binance (/stream, /ws/stream)
└─ utils/
   ├─ time.py         # now_ms, to_ms
   └─ binance.py      # helpers para payloads/URLs estilo Binance
```

> **Nota**: El gateway crea un **run** por ejecución y lo persiste (metadatos + fills + equity) en DuckDB. Así podés auditar/analizar cada sesión.

---

## Modelo de simulación

* **Fuente de datos**: barras OHLC (y funding opcional) leídas de **DuckDB**.
* **Reproducción**: el *replayer* avanza barras a **`--speed` velas por segundo** y emite eventos por HTTP/WS.
* **Órdenes**:

  * **MARKET**: se llenan al **precio de cierre** de la barra actual ± *slippage\_bps* (en contra del lado) + **fee taker**.
  * **LIMIT**: se llenan en el **cierre** si el precio objetivo **hubo** podido ejecutarse dentro del rango de la barra: `BUY si low <= price` · `SELL si high >= price`. Si no, quedan **OPEN**.
  * **STOP\_MARKET** (reduceOnly opcional): se arma una condición de disparo y, al cumplirse, envía una **MARKET**.
  * **Cancelación**: por `orderId` o `allOpenOrders`.
  * **Rounding**: se respetan `tickSize` (precio) y `stepSize` (cantidad) de `exchangeInfo`.
* **Cuenta / Riesgo**:

  * **positionRisk** devuelve una **posición única por símbolo** (modelo single‑position; las rutas de *dual/leverage/margin* existen para compatibilidad pero su efecto es limitado en la simulación).
  * **Fees** configurables: `maker_bps` y `taker_bps`.
  * **Slippage** configurable para MARKET: `slippage_bps`.
* **Persistencia**: Todo fill actualiza `trades_fills` y el equity en `equity_curve`.

---

## Esquema DuckDB y persistencia

Se asume una DB inicializada (ver [Instalación y datos](#instalación-y-datos)). Tablas usadas por el gateway:

* **runs**

  * `run_id UUID, created_at TIMESTAMP, strategy VARCHAR, params_json JSON, code_hash VARCHAR`
  * `strategy` suele ser `"gateway/binance-sim"`.
* **trades\_fills**

  * `run_id UUID, seq BIGINT, ts BIGINT, symbol VARCHAR, side VARCHAR, price DOUBLE, qty DOUBLE, realized_pnl DOUBLE, fee DOUBLE, is_maker BOOLEAN`
* **equity\_curve**

  * `run_id UUID, ts BIGINT, equity DOUBLE`
* **ohlc** / **ohlc\_1m** / etc. (tu histórico)

  * `symbol, ts(open), open, high, low, close, volume, close_ts`
* **funding** (opcional)

  * `symbol, funding_time, funding_rate`

> Si ves `Catalog Error: Table with name runs does not exist!`, inicializá el esquema con el script correspondiente (ver abajo) o usa `store.ensure_min_schema()` si está expuesto por CLI.

---

## Instalación y datos

```bash
# 1) Crear venv e instalar dependencias
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -U pip setuptools wheel
python -m pip install -r requirements.txt

# 2) Inicializar la DB (crea el esquema mínimo)
python -m scripts.init_duckdb

# 3) Cargar histórico en DuckDB (ejemplo 1h)
python -m scripts.load_to_duckdb \
  --db data/duckdb/exsim.duckdb \
  --source api|files \
  --symbol BTCUSDT --interval 1h \
  --start 2024-01-01 --end 2024-12-31
```

> Si ya tenés `data/duckdb/exsim.duckdb` con datos, podés saltar el paso 3.

---

## Ejecutar el gateway (CLI + ENV)

### CLI básico

```bash
python -m gateway.main \
  --duckdb-path data/duckdb/exsim.duckdb \
  --symbol BTCUSDT --interval 1h \
  --start 2024-01-01 --end 2024-12-31 \
  --speed 25 --maker-bps 1 --taker-bps 2 \
  --port 9010 --host 0.0.0.0
```

Abre **[http://localhost:9010/docs](http://localhost:9010/docs)** (Swagger) y **/redoc**.

### Variables de entorno equivalentes

`HOST, PORT, DUCKDB_PATH, SYMBOL, INTERVAL, START, END, SPEED, FEE_MAKER_BPS, FEE_TAKER_BPS, SLIPPAGE_BPS, STARTING_BALANCE`.

### Conexión desde otra máquina

Expone HTTP/WS sin auth (para desarrollo). Si tu bot corre en Docker y el gateway fuera (o viceversa), ajustá la base URL/WS o usa `host.docker.internal` según tu SO.

---

## HTTP API (subset Binance)

### Mercado / Meta

* `GET /fapi/v1/time` → `{ serverTime }`
* `GET /fapi/v1/exchangeInfo` → `symbols[], PRICE_FILTER.tickSize, LOT_SIZE.stepSize`
* `GET /fapi/v1/klines?symbol=BTCUSDT&interval=1h&startTime=...&endTime=...&limit=N`
  → `[[openTime, open, high, low, close, volume, closeTime], ...]`
* `GET /fapi/v1/fundingRate?symbol=BTCUSDT&limit=N` → `[{ fundingTime, fundingRate }, ...]`
* `GET /fapi/v1/premiumIndex?symbol=BTCUSDT` → `{ markPrice, lastFundingRate }`

### Órdenes

* `POST /fapi/v1/order`

  * Body (JSON): `symbol, side(BUY|SELL), type(MARKET|LIMIT|STOP_MARKET), quantity|origQty, price(opc), timeInForce(opc), stopPrice(opc), reduceOnly(opc), newClientOrderId(opc)`
  * Respuesta: estilo Binance (`orderId, status, executedQty, fills[]`, etc.)
* `DELETE /fapi/v1/order?symbol=...&orderId=...` → Cancela una orden
* `DELETE /fapi/v1/allOpenOrders?symbol=...` → Cancela todas
* `GET /fapi/v1/openOrders?symbol=...` → Lista de órdenes vivas
* `GET /fapi/v1/ticker/bookTicker?symbol=...` → `bidPrice/askPrice` sintéticos

### Cuenta / Riesgo

* `GET /fapi/v2/balance`
* `GET /fapi/v2/positionRisk` (alias `/fapi/v1/positionRisk`)
* `POST /fapi/v1/leverage`, `POST /fapi/v1/marginType`, `POST /fapi/v1/positionSide/dual`, `POST /fapi/v1/listenKey`

  * Endpoints presentes por compat; el modelo interno es single‑position.

> **Auth/Firmas**: **no** se validan (desarrollo). Los parámetros `timestamp/signature` se ignoran.

---

## WebSocket API

* **Rutas**: `/stream` y `/ws/stream` (alias, como Binance)
* **Query**: `streams=btcusdt@kline_1h/btcusdt@markPrice@1s`
* **Eventos**:

  * Kline: `{ stream:"btcusdt@kline_1h", data:{ e:"kline", E:..., s:"BTCUSDT", k:{ t, T, s, i:"1h", o, c, h, l, v, x:true } } }`
  * Mark price: `{ stream:"btcusdt@markPrice@1s", data:{ e:"markPriceUpdate", E:..., s:"BTCUSDT", p:"..." } }`
* **Cierre de vela**: `k.x === true`.

---

## Admin API (replay y estado)

* `POST /admin/replay`

  * Body: `{ "speed_bars_per_sec": <int> }`
  * Respuesta: `{ ok:true, run_id, bars:<total_barras_cargadas> }`
* `GET  /admin/status`

  * Respuesta típica:

    ```json
    {
      "symbol": "BTCUSDT",
      "interval": "1h",
      "run_id": "<uuid>",
      "ws_clients": 1,
      "bars_loaded": 509869,
      "equity_now": 100000.0,
      "position": { "qty": 0.0, "avg_price": 0.0 },
      "leverage": 1, "margin_type": "cross", "dual_side": true
    }
    ```

---

## Ejemplos rápidos (curl y Python)

### curl

```bash
# server time
curl -s http://localhost:9010/fapi/v1/time

# exchange info (tick/step del símbolo)
curl -s http://localhost:9010/fapi/v1/exchangeInfo | jq '.symbols[] | select(.symbol=="BTCUSDT")'

# crear una LIMIT
curl -s -X POST http://localhost:9010/fapi/v1/order \
  -H 'Content-Type: application/json' \
  -d '{"symbol":"BTCUSDT","side":"BUY","type":"LIMIT","timeInForce":"GTC","quantity":"0.001","price":"50000","newClientOrderId":"readme-limit"}'

# órdenes abiertas
curl -s 'http://localhost:9010/fapi/v1/openOrders?symbol=BTCUSDT' | jq

# posición
curl -s 'http://localhost:9010/fapi/v2/positionRisk?symbol=BTCUSDT' | jq

# controlar el replay (velocidad)
curl -s -X POST http://localhost:9010/admin/replay -H 'Content-Type: application/json' -d '{"speed_bars_per_sec":25}'
```

### Python (requests + websockets)

```python
import asyncio, json, websockets, requests
BASE = "http://localhost:9010"

print(requests.get(f"{BASE}/fapi/v1/time").json())

async def run_ws():
    url = "ws://localhost:9010/stream?streams=btcusdt@kline_1h"
    async with websockets.connect(url) as ws:
        while True:
            msg = json.loads(await ws.recv())
            if msg.get("data",{}).get("k",{}).get("x"):
                k = msg["data"]["k"]
                print("cierre:", k["T"], k["c"])  # closeTime, close

asyncio.run(run_ws())
```

---

## Integración con bots (variables de entorno)

Apuntá tu bot a este gateway con las mismas ENV que usarías para Binance:

```
BINANCE_BASE_URL=http://localhost:9010
BINANCE_WS_BASE=ws://localhost:9010
```

Ejemplo (bot EMA):

```
URL=http://localhost:9010 SYM=BTCUSDT TF=1h FAST=3 SLOW=9 ALLOC=1000 STOP=0.01 ALLOW_SHORTS=false
```

En Docker, si el bot corre en contenedor y el gateway en el host, usa `http://host.docker.internal:9010` (según SO).

---

## Consultas útiles en SQL

```sql
-- últimos runs
SELECT run_id, strategy, created_at
FROM runs
ORDER BY created_at DESC
LIMIT 5;

-- conteo y PnL de un run
WITH f AS (
  SELECT ts, seq, side, qty, price, fee, realized_pnl,
         CASE WHEN side='BUY' THEN qty ELSE -qty END AS signed_qty
  FROM trades_fills WHERE run_id = '<RUN_ID>'
)
SELECT COUNT(*)                                   AS fills,
       SUM(CASE WHEN side='BUY'  THEN 1 ELSE 0 END) AS buys,
       SUM(CASE WHEN side='SELL' THEN 1 ELSE 0 END) AS sells,
       SUM(realized_pnl)                           AS realized_pnl,
       SUM(fee)                                    AS fees,
       SUM(realized_pnl) - SUM(fee)                AS net_pnl
FROM f;

-- equity final del run
SELECT equity
FROM equity_curve
WHERE run_id = '<RUN_ID>'
ORDER BY ts DESC
LIMIT 1;
```

---

## Troubleshooting

* **`IO Error: Cannot open file ... exsim.duckdb`**: verificá la ruta `--duckdb-path` (creá carpetas). Si no existe, corré `scripts.init_duckdb` y luego cargá datos.
* **`Table with name runs does not exist`**: no se inicializó el esquema. Corré `python -m scripts.init_duckdb`.
* **Sin órdenes/fills**: confirmá que hay datos en el rango `--start/--end`, que el bot realmente envía órdenes y que `exchangeInfo` devuelve `tickSize/stepSize` coherentes.
* **WS sin mensajes**: verificá `/admin/status` (`ws_clients`) y la query `streams`.
* **Puertos en uso**: cambiá `--port` o cerrá procesos previos.

---

## Limitaciones y roadmap

**Limitaciones actuales**

* Un solo **símbolo** por instancia (pensado para pruebas simples).
* Modelo **single‑position** (las rutas *dual/margin/leverage* son de compatibilidad).
* No se simula *match‑engine* de profundidad; `bookTicker` es sintético.
* Auth/Firmas **no** implementadas (uso de desarrollo).

**Roadmap breve**

* Tipos de orden adicionales (STOP\_LIMIT, TRAILING\_STOP, OCO).
* WS de user‑data (`executionReport`) mockeado.
* Soporte nativo multi‑símbolo / multi‑instancia.
* Mejoras en el modelo de fills (volumen, parcialidad, impacto).

---

## FAQ (pensado para IA)

* **¿Qué hace este proyecto en una frase?**
  Emula un Binance Futures reducido usando datos de DuckDB para probar bots reales sin tocar el exchange real.
* **¿Qué necesito para correrlo?**
  Python + dependencias, una DB DuckDB con OHLC, y ejecutar `gateway.main`.
* **¿Cómo veo cuántas órdenes se ejecutaron?**
  Consultá `trades_fills` filtrando por `run_id`.
* **¿Cómo controlo la velocidad del replay?**
  `POST /admin/replay` con `{ "speed_bars_per_sec": N }`.
* **¿Puedo conectarme por WebSocket como en Binance?**
  Sí, usando `/stream` y streams del tipo `btcusdt@kline_1h`.
