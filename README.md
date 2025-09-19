# Exchange Simulator (Rust) — README

Un “exchange simulado” para **backtesting** y **replay** de datos de mercado estilo Binance.
Sirve datos históricos desde **DuckDB**, simula **órdenes** (market/limit), mantiene **cuentas** por sesión, expone **HTTP + WebSocket**, y documenta la API con **Swagger (utoipa)**.

> **Objetivo:** crear un entorno determinista para someter bots a años de datos a distintas velocidades, reproducir escenarios y validar estrategias sin tocar Binance real.

---

## TL;DR (para una IA)

* **Arquitectura hexagonal**: `domain` (modelos/traits), `services` (reglas), `infra` (DuckDB, clock, WS), `api` (axum), `dto` (wire models), `app` (bootstrap/router), `oas` (OpenAPI).
* **Datos**: DuckDB almacena `datasets`, `klines` y `symbols`. Las órdenes, cuentas y sesiones están (hoy) **en memoria**; los trades se guardan en un buffer en memoria.
* **Flujo típico**:

  1. Registrar un dataset (ruta + formato CSV/Parquet).
  2. Ingestar a DuckDB (carga `klines` y deduce `symbols`).
  3. Crear una sesión (símbolos, intervalo, rango temporal, velocidad).
  4. Iniciar/pausar/reanudar/seek de la sesión (replay).
  5. Consumir `/api/v3/klines` y colocar órdenes `/api/v3/order`.
* **Swagger**: `http://localhost:<PORT>/swagger-ui`.
* **Limitaciones actuales**: matching simplificado (market llena al último close; limit llena si el precio cruza el OHLC del kline actual), sin persistence de órdenes/trades/cuentas, sin fees reales.

---

## Requisitos

* Rust 1.87+
* DuckDB (se usa crate con **bundled**)
* Linux/Mac/WSL ok

---

## Configuración (.env ejemplo)

Crea un archivo `.env` en la raíz:

```env
# HTTP
PORT=3001

# Sesiones / reloj
DEFAULT_SPEED=1.0          # multiplicador del tiempo simulado
MAX_SESSION_CLIENTS=100

# Cuentas
DEFAULT_QUOTE=USDT
INITIAL_QUOTE_BALANCE=10000

# Datos
DATA_DIR=./data
DUCKDB_PATH=./data/market.duckdb   # si tu pool lo soporta; sino se usará el default del código
```

> Los nombres exactos dependen de `infra/config.rs`. Si alguno difiere, ajusta este ejemplo a lo que parsea ese archivo.

---

## Comandos útiles

### Desarrollo

```bash
# Compilar y correr
cargo run

# Solo compilar
cargo build

# Tests
cargo test

# Lints/format
cargo fmt
cargo clippy

# Árbol filtrado (útil para IA)
tree -a --prune \
  -I 'target|.git|.idea|.vscode|node_modules|__pycache__|.DS_Store|*.log|*.tmp' \
  -P '*.rs|Cargo.toml|Cargo.lock|README*|*.yml|*.yaml|.env*'
```

### “Llenar” la base con datos (vía API)

1. **Registrar dataset** (indica nombre, ruta y formato):

```bash
curl -sS -X POST "http://localhost:3001/api/v1/datasets" \
  -H "content-type: application/json" \
  -d '{
    "name": "binance_btcusdt_1m_2019_2021",
    "path": "./data/btcusdt_1m.parquet",
    "format": "parquet"
  }'
# → devuelve { id, name, basePath, format, createdAt }
```

2. **Ingestar** (carga a DuckDB `klines` y puebla `symbols`):

```bash
DATASET_ID="<uuid devuelto arriba>"
curl -sS -X POST "http://localhost:3001/api/v1/datasets/${DATASET_ID}/ingest"
# → 204 No Content
```

3. **Listar datasets**:

```bash
curl -sS "http://localhost:3001/api/v1/datasets"
```

---

## Endpoints

> **Swagger UI**: `http://localhost:3001/swagger-ui`
> **OpenAPI JSON**: `/api-docs/openapi.json`

### Market

* `GET /api/v1/exchangeInfo` → **Funcional**. Retorna símbolos activos (de DuckDB).
* `GET /api/v3/klines?symbol=BTCUSDT&interval=1m&startTime=...&endTime=...&limit=...` → **Funcional**. Devuelve klines históricos desde DuckDB.

### Datasets

* `POST /api/v1/datasets` → **Funcional**. Registrar dataset `{ name, path, format: csv|parquet }`.
* `GET /api/v1/datasets` → **Funcional**. Lista datasets registrados.
* `POST /api/v1/datasets/{id}/ingest` → **Funcional**. Ingesta a tablas (`klines`, `symbols`).

### Sessions (replay)

* `POST /api/v1/sessions` → **Funcional**. Crea sesión `{ symbols[], interval, startTime, endTime, speed, seed? }`.
* `GET /api/v1/sessions` → **Funcional**. Lista sesiones.
* `GET /api/v1/sessions/{id}` → **Funcional**. Estado de sesión.
* `POST /api/v1/sessions/{id}/start` → **Funcional**. Setea velocidad y resume.
* `POST /api/v1/sessions/{id}/pause` → **Funcional**.
* `POST /api/v1/sessions/{id}/resume` → **Funcional**.
* `POST /api/v1/sessions/{id}/seek?to=TIMESTAMP_MS` → **Funcional**. Mueve el reloj y el replay al timestamp dado.

### Orders / Account (simuladas)

* `POST /api/v3/order` → **Simulada**. Crea orden **market** o **limit**.

  * *Market*: llena al último `close` del kline actual.
  * *Limit*: llena si `buy: limit >= low`, `sell: limit <= high` del kline actual.
* `GET /api/v3/order?sessionId=...&orderId=...|origClientOrderId=...` → **Funcional** (memoria).
* `DELETE /api/v3/order?sessionId=...&orderId=...|origClientOrderId=...` → **Funcional** (memoria).
* `GET /api/v3/openOrders?sessionId=...&symbol?=...` → **Funcional** (memoria).
* `GET /api/v3/myTrades?sessionId=...&symbol=...` → **Funcional** (memoria).
* `GET /api/v3/account?sessionId=...` → **Funcional** (memoria).

  * Al primer uso, crea la cuenta con `DEFAULT_QUOTE` e `INITIAL_QUOTE_BALANCE`.

> **Notas de simulación**
>
> * Fees = 0 por ahora.
> * No hay *partial fills* todavía (aunque el modelo lo permite).
> * Órdenes/Trades/Cuentas/Sesiones: **in-memory**; reiniciar el proceso las borra.
> * Matching muy simplificado para testing rápido.

---

## Cómo probar rápido (cURL)

```bash
# 1) Registrar + Ingestar dataset
curl -sS -X POST http://localhost:3001/api/v1/datasets \
  -H 'content-type: application/json' \
  -d '{"name":"btc_1m","path":"./data/btc_1m.parquet","format":"parquet"}' | tee ds.json
DS_ID=$(jq -r .id ds.json)
curl -sS -X POST "http://localhost:3001/api/v1/datasets/$DS_ID/ingest"

# 2) Crear sesión
curl -sS -X POST http://localhost:3001/api/v1/sessions \
  -H 'content-type: application/json' \
  -d '{
        "symbols":["BTCUSDT"],
        "interval":"1m",
        "startTime":1577836800000,
        "endTime":1577923200000,
        "speed":1.0
      }' | tee sess.json
SESS_ID=$(jq -r .sessionId sess.json)

# 3) Start
curl -sS -X POST "http://localhost:3001/api/v1/sessions/$SESS_ID/start"

# 4) Klines
curl -sS "http://localhost:3001/api/v3/klines?symbol=BTCUSDT&interval=1m&limit=5"

# 5) Orden MARKET BUY 0.01 BTC
curl -sS -X POST http://localhost:3001/api/v3/order \
  -H 'content-type: application/json' \
  -d "{\"session_id\":\"$SESS_ID\",\"symbol\":\"BTCUSDT\",\"side\":\"BUY\",\"type\":\"MARKET\",\"quantity\":0.01}"

# 6) Cuenta
curl -sS "http://localhost:3001/api/v3/account?sessionId=$SESS_ID"
```

---

## Estructura del proyecto (qué hace cada cosa)

```
src/
├─ api/                 # Adaptadores HTTP (Axum)
│  ├─ errors.rs         # Manejador de errores/health
│  ├─ mod.rs            # Router raíz de API
│  └─ v1/               # Endpoints versión 1
│     ├─ account.rs     # GET /api/v3/account
│     ├─ datasets.rs    # POST/GET /api/v1/datasets, POST ingest
│     ├─ market.rs      # exchangeInfo, klines
│     ├─ orders.rs      # new/cancel/get/openOrders/myTrades
│     ├─ sessions.rs    # create/list/get/start/pause/resume/seek
│     └─ ws.rs          # (WIP) WS por sesión
├─ app/
│  ├─ bootstrap.rs      # Inyección de dependencias (repos, servicios)
│  ├─ router.rs         # Construcción del Router + Swagger
│  └─ mod.rs
├─ domain/              # Núcleo: modelos y puertos (traits)
│  ├─ models.rs         # Kline, Order, Fill, Session, DatasetFormat, etc.
│  ├─ traits.rs         # MarketStore, MarketIngestor, SessionsRepo, Clock, etc.
│  └─ value_objects.rs  # Tipos fuertes: Price, Quantity, Interval, Speed, TimestampMs
├─ dto/                 # Esquemas de entrada/salida API (serde + utoipa)
│  ├─ account.rs
│  ├─ datasets.rs
│  ├─ market.rs
│  ├─ orders.rs
│  ├─ sessions.rs
│  └─ ws.rs
├─ infra/               # Implementaciones técnicas (adaptadores)
│  ├─ clock.rs          # Reloj simulado por sesión (in-memory)
│  ├─ config.rs         # Carga de .env → AppConfig
│  ├─ duckdb/           # Acceso a datos históricos
│  │  ├─ db.rs          # Pool/conexión DuckDB
│  │  ├─ ingest_repo.rs # INSERT datasets, import CSV/Parquet → klines/symbols
│  │  ├─ market_repo.rs # SELECT symbols y klines
│  │  └─ mod.rs
│  ├─ repos/            # Repos en memoria (órdenes, cuentas, sesiones)
│  │  ├─ memory.rs
│  │  └─ mod.rs
│  └─ ws/               # Infra de websockets (broadcast por sesión)
│     ├─ broadcaster.rs
│     └─ mod.rs
├─ services/            # Casos de uso / lógica de dominio
│  ├─ account_service.rs# Saldos y fills
│  ├─ ingest_service.rs # Orquesta register/list/ingest datasets
│  ├─ market_service.rs # Exchange info y klines
│  ├─ orders_service.rs # Matching simplificado y fills
│  ├─ replay_service.rs # Emite klines por sesión (WS), coordina Clock
│  └─ sessions_service.rs # Crear/gestionar sesión (start/pause/resume/seek)
├─ oas.rs               # Definición OpenAPI (utoipa)
├─ error.rs             # AppError → HTTP/status
├─ main.rs              # Entrypoint binario
└─ lib.rs               # Crate lib (para tests/integración)
```

---

## Versiones clave (Cargo)

* `axum = 0.7.x`
* `tower-http = 0.6.x` (CORS, Trace)
* `utoipa = 4.x`, `utoipa-swagger-ui = 7.x`
* `duckdb = 0.9.2` (**bundled**)
* `uuid = 1.x`, `serde = 1.x`, `tokio = 1.x`
* `chrono = 0.4.31` (pin para evitar conflicto con Arrow)

---

## Esquema de datos (DuckDB)

* `datasets(id, name, path, format, created_at)`
* `klines(symbol, interval, open_time, open, high, low, close, volume, close_time)`
  (las columnas deben alinear con tu CSV/Parquet; `ingest_repo` hace `read_csv_auto`/`read_parquet`)
* `symbols(symbol, base, quote, active)` (poblado al ingerir, deduciendo `base/quote`)

> Si los nombres difieren, revisa `infra/duckdb/ingest_repo.rs`.

---

## Roadmap / Ideas

* Persistir órdenes/cuentas/sesiones en DuckDB (migrar `infra::repos::memory` a repos DuckDB).
* *Partial fills*, fees, tamaños mínimos, *lot sizes* por símbolo.
* WS públicos: streams por sesión (`kline`, `bookTicker` simulado).
* Herramientas CLI para ingest y sesiones (además de la API).
* Métricas Prometheus y trazas OpenTelemetry.

---

## Troubleshooting

* **Swagger no carga** → verifica `oas.rs` y que el server arranca con `PORT`. Abre `/swagger-ui`.
* **Conflictos chrono/arrow** → ya fijado `chrono = "=0.4.31"`.
* **No llena órdenes limit** → recuerda la regla: `BUY: limit >= low`, `SELL: limit <= high` del *kline actual*.
* **Sin datos** → asegúrate de que `path` existe y `format` correcto en `POST /datasets`.

---

¡Listo! Con este README cualquier IA (y humana) entiende el contexto, puede levantar el proyecto, ingerir datos, crear sesiones y empezar a probar bots.













