# Exchange Simulator (Rust) — README para IA

Simulador de exchange para **replay/backtesting** con datos históricos en **DuckDB**, **órdenes simuladas** (market/limit), **cuentas por sesión**, **API HTTP estilo Binance** y **WebSocket** de velas.

> **Objetivo:** correr bots contra años de datos a distintas velocidades, reproducir escenarios deterministas y validar estrategias sin tocar Binance real.

---

## TL;DR (para empezar ya)

* **WS expuesto:** `GET /ws?sessionId=<uuid>&streams=kline@1m:ETHBTC`
* **REST clave:**

  * `GET /api/v1/exchangeInfo` (símbolos locales)
  * `GET /api/v1/market/klines` (**o** `GET /api/v3/klines`) — klines desde DuckDB
  * Sesiones: `POST /api/v1/sessions`, `GET /api/v1/sessions`, `POST /{id}/start|pause|resume|seek`
  * Órdenes (simuladas): `POST /api/v3/order`, `GET /api/v3/account`, etc.
* **OpenAPI JSON:** `/api-docs/openapi.json` (la UI de Swagger vive en el **front**)
* **Persistencia actual:** DuckDB para datos de mercado; sesiones/órdenes/cuentas en **memoria**
* **Matching simple:** MARKET llena al último close; LIMIT llena si cruza OHLC del kline actual
* **CORS:** permisivo en dev

---

## Arquitectura (mapa mental)

* **domain/**: modelos y puertos (traits)
* **services/**: casos de uso (market, sessions, replay, orders)
* **infra/**: adaptadores (DuckDB, reloj, repos en memoria, broadcaster WS)
* **api/**: controladores Axum (HTTP/WS)
* **dto/**: contratos de request/response (serde + utoipa)
* **app/**: wiring/inyección y router (CORS/Trace/OpenAPI)
* **oas.rs**: definición OpenAPI

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

## Configuración

Crear `.env` en la raíz (nombres esperados por `infra/config.rs`):

```env
# HTTP
PORT=3001

# Datos
DATA_DIR=./data
DUCKDB_PATH=./data/market.duckdb

# WebSocket
WS_BUFFER=1024

# Reloj / sesiones
DEFAULT_SPEED=1.0
MAX_SESSION_CLIENTS=100

# Cuentas simuladas
DEFAULT_QUOTE=USDT
INITIAL_QUOTE_BALANCE=10000
```

---

## Ejecutar

```bash
# Compilar + correr
cargo run

# Con logs útiles
RUST_LOG=info,tower_http=info,exchange_simulator=debug cargo run
```

Salida esperada:

```
INFO opening DuckDB duckdb_path=.../data/market.duckdb
INFO duckdb warmup datasets=... klines=... symbols=...
INFO starting exchange simulator server addr=0.0.0.0:3001
```

---

## API de Mercado (local/DuckDB)

### Exchange info (símbolos)

```
GET /api/v1/exchangeInfo
```

### Klines desde DuckDB

Dos rutas equivalentes:

```
GET /api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=1757833200000&endTime=1757839200000&limit=1000
GET /api/v3/klines?symbol=ETHBTC&interval=1m&startTime=...&endTime=...&limit=...
```

* `startTime/endTime` en **ms**
* `interval` usa los del dominio (ej. `1m`, `1h`, `1d`)

---

## Gestión de datasets

```
POST /api/v1/datasets                 # registrar {name, path, format: csv|parquet}
POST /api/v1/datasets/{id}/ingest     # ingesta → klines + symbols
GET  /api/v1/datasets                 # listar
GET  /api/v1/datasets/symbols         # símbolos disponibles
GET  /api/v1/datasets/{symbol}/intervals
GET  /api/v1/datasets/{symbol}/{interval}/range  # { firstOpenTime, lastCloseTime }
```

---

## Sesiones (replay)

```
POST /api/v1/sessions                 # crear {symbols[], interval, startTime, endTime, speed, seed?}
GET  /api/v1/sessions                 # listar
GET  /api/v1/sessions/{id}            # estado
POST /api/v1/sessions/{id}/start
POST /api/v1/sessions/{id}/pause
POST /api/v1/sessions/{id}/resume
POST /api/v1/sessions/{id}/seek?to=<ms>
```

**Notas:**

* El replay emite velas en orden y respeta `[startTime, endTime]`.
* Puedes **conectar WS** antes de `start`; el server **no** cierra por inactividad.

---

## WebSocket

**Ruta:**

```
GET /ws?sessionId=<uuid>&streams=<streams>
```

* `streams`: por ahora `kline@<interval>:<symbol>` (p. ej. `kline@1m:ETHBTC`)
* Conexiones múltiples por sesión: OK (buffer configurable vía `WS_BUFFER`).

**Mensaje (kline)**

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

> **Keepalive:** el servidor **no** impone timeout; si ves `1011 keepalive ping timeout` suele ser del **cliente** (p. ej. script Python). Los navegadores/WS modernos no deberían cortarse.

---

## Órdenes y cuentas (simuladas)

* **Órdenes**

  ```
  POST /api/v3/order
  ```

  * MARKET: llena al último `close`
  * LIMIT:

    * BUY: llena si `limit >= low` del kline actual
    * SELL: llena si `limit <= high` del kline actual

* **Consultas y cuenta**

  ```
  GET    /api/v3/order
  DELETE /api/v3/order
  GET    /api/v3/openOrders
  GET    /api/v3/myTrades
  GET    /api/v3/account?sessionId=...
  ```

  * La cuenta se inicializa on-demand con `DEFAULT_QUOTE` e `INITIAL_QUOTE_BALANCE`.

> **Persistencia:** por ahora **in-memory** (reiniciar borra). DuckDB solo almacena datasets/klines/symbols.

---

## OpenAPI

* **JSON**: `GET /api-docs/openapi.json`
  El **front** renderiza la UI (Swagger) consumiendo este JSON.

---

## Recetas de cURL (sanity check rápido)

```bash
# ¿Qué símbolos/intervalos/rango tengo?
curl -s http://localhost:3001/api/v1/datasets/symbols | jq
curl -s http://localhost:3001/api/v1/datasets/ETHBTC/intervals | jq
curl -s http://localhost:3001/api/v1/datasets/ETHBTC/1m/range | jq

# 10 velas de 1m
START=1757833200000
END=$(( START + 60000*10 ))
curl -s "http://localhost:3001/api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=$START&endTime=$END&limit=1000" | jq length
curl -s "http://localhost:3001/api/v1/market/klines?symbol=ETHBTC&interval=1m&startTime=$START&endTime=$END&limit=1" | jq '.[0]'

# Crear sesión + start
curl -sS -X POST http://localhost:3001/api/v1/sessions \
  -H 'content-type: application/json' \
  -d '{"symbols":["ETHBTC"],"interval":"1m","startTime":'$START',"endTime":'$END',"speed":1.0}' | tee sess.json
SESS_ID=$(jq -r .sessionId sess.json)
curl -sS -X POST "http://localhost:3001/api/v1/sessions/$SESS_ID/start"

# Conectar WS (inspección manual con websocat)
# websocat "ws://localhost:3001/ws?sessionId=$SESS_ID&streams=kline@1m:ETHBTC"
```

---

## Decisiones y límites (para IA)

* **Diseño**: hexagonal (dominio independiente), Axum como controlador, servicios orquestan repos/infra.
* **Reloj simulado** por sesión (pausa/resume/seek) usado por replay/órdenes.
* **Monotonicidad** de `closeTime` en replay; nunca retrocede el reloj.
* **Sin fees ni partial fills** hoy; modelo listo para ampliarse.
* **Cierre WS**: el broadcaster corta solo cuando se elimina la sesión o se decide cerrar explícitamente.

---

## Extensiones típicas

* Persistir **sesiones/órdenes/cuentas** en DuckDB (migrar repos en memoria).
* Agregar **stats WS** (conexiones activas por sesión) cada N segundos.
* Validar órdenes contra **lot sizes/tick sizes** por símbolo.
* Métricas Prometheus y trazas OpenTelemetry.

---

## Troubleshooting

* **No veo velas en WS**:

  * Revisa que el **rango** de la sesión tenga datos (`/datasets/{symbol}/{interval}/range`).
  * Confirma que el cliente usa `GET /ws` con `sessionId` y `streams`.
  * Activa logs: `RUST_LOG=info,tower_http=info,exchange_simulator=debug`
* **`/api/v1/market/klines` 404**:

  * Asegura que usas esa ruta (existe también `/api/v3/klines`).
* **Cierre 1011**:

  * Suele ser del cliente (keepalive). El servidor no corta por idle.

---

## Versiones (Cargo típicas)

* `axum = 0.7.x`, `tower-http = 0.6.x`
* `utoipa = 4.x`
* `duckdb = 0.9.x` (**bundled**)
* `tokio = 1.x`, `serde = 1.x`, `uuid = 1.x`
* `chrono = "=0.4.31"` (para evitar conflictos con Arrow)

---

## Preguntas que puedes hacerle a una IA (y este README responde)

* “¿Qué endpoints tengo para klines y con qué parámetros?”
* “¿Cómo me conecto al WebSocket y qué mensajes llegarán?”
* “¿Cómo creo una sesión y empiezo el replay?”
* “¿Qué persiste en DuckDB y qué queda en memoria?”
* “¿Cómo pruebo que hay datos en el rango antes de abrir WS?”

---

Listo. Con esto, cualquier IA (y humana) puede levantar, entender, testear y extender el proyecto de inmediato.
