# Exchange Simulator (Rust)

Simulador de exchange estilo Binance que permite **backtesting** y **replay** controlado utilizando **Axum** y **DuckDB**. El servicio carga datasets históricos, reproduce velas a distintas velocidades y expone endpoints REST/WS compatibles con un subset de Binance para que los bots se conecten sin modificar su lógica.

## Características principales

- **API REST y WS** con Axum 0.7 y streams per-sesión mediante `tokio::sync::broadcast`.
- **DuckDB** como base de datos para símbolos y velas OHLC (ingesta de CSV/Parquet por endpoint).
- **Sesiones de replay** con reloj simulado por sesión (pausa, resume, seek, cambio de velocidad).
- **Motor de órdenes** simplificado (MARKET/LIMIT) que ejecuta contra el último kline publicado y actualiza balances.
- **Swagger UI** (`/swagger-ui`) generado con `utoipa`.

## Arquitectura

```
src/
  app/            # wiring y router principal
  api/            # controladores REST/WS
  dto/            # DTOs expuestos en la API
  domain/         # modelos y traits (ports)
  services/       # reglas de negocio (market, ingest, sessions, orders, replay, account)
  infra/          # adapters: DuckDB, repos en memoria, clock simulado, broadcaster WS
  oas.rs          # documento OpenAPI
  main.rs         # binario principal (Axum server)
  lib.rs          # exporta módulos para tests/integración
```

Los servicios dependen de `traits` definidos en `domain::traits`. Las implementaciones viven en `infra`. `app::bootstrap` crea las dependencias y arma el `Router` con middlewares de CORS/Trace.

## Requisitos

- Rust 1.75+ (edición 2024).
- DuckDB embebido (`duckdb-rs` con `bundled`).
- Datasets OHLC en CSV/Parquet con columnas: `symbol, interval, open_time, open, high, low, close, volume, close_time`.

## Configuración

Variables `.env` leídas por `infra::config::AppConfig`:

```
PORT=3001
DUCKDB_PATH=./data/market.duckdb
DATA_DIR=./data
DEFAULT_SPEED=1.0
WS_BUFFER=1024
MAX_SESSION_CLIENTS=100
```

## Cómo ejecutar

```bash
# 1. Formatear y compilar
cargo fmt
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse cargo build

# 2. Levantar el servidor
env PORT=3001 cargo run
```

> Nota: en entornos sin acceso completo a crates.io se puede forzar el modo `sparse` con `CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse`.

## Ingesta y uso básico

1. Registrar dataset `POST /api/v1/datasets` con `{ "name": "binance", "path": "/ruta/klines.csv", "format": "csv" }`.
2. Ingerir dataset `POST /api/v1/datasets/{id}/ingest` (usa `read_csv_auto` o `read_parquet`).
3. Crear sesión `POST /api/v1/sessions` indicando símbolos, intervalo y rango temporal.
4. `POST /api/v1/sessions/{id}/start` para iniciar el replay. Suscribirse a `/ws?session_id=...` para recibir eventos `kline`.
5. Enviar órdenes `POST /api/v3/order` (MARKET/LIMIT) y consultar balances con `GET /api/v3/account`.

## Endpoints principales

| Método | Ruta | Descripción |
| ------ | ---- | ----------- |
| GET | `/ping` | Health check |
| GET | `/api/v1/exchangeInfo` | Símbolos activos |
| GET | `/api/v3/klines` | Consulta puntual de velas |
| POST | `/api/v1/datasets` | Registrar dataset |
| POST | `/api/v1/datasets/{id}/ingest` | Cargar CSV/Parquet a DuckDB |
| GET | `/api/v1/sessions` | Listar sesiones |
| POST | `/api/v1/sessions/{id}/start` | Iniciar replay |
| POST | `/api/v1/sessions/{id}/pause` | Pausar sesión |
| POST | `/api/v1/sessions/{id}/resume` | Reanudar sesión |
| POST | `/api/v1/sessions/{id}/seek?to=ms` | Saltar en el timeline |
| POST | `/api/v3/order` | Crear orden MARKET/LIMIT |
| GET | `/api/v3/order` | Consultar orden |
| DELETE | `/api/v3/order` | Cancelar orden |
| GET | `/api/v3/openOrders` | Órdenes abiertas |
| GET | `/api/v3/account` | Balances por sesión |
| GET | `/ws?session_id=...` | Stream WS con eventos `kline` |

La documentación completa de la API se encuentra en `/swagger-ui`.

## Tests

El proyecto incluye:

- Tests unitarios para reglas puntuales (`orders_service` y reloj simulado).
- Test de integración (`tests/integration.rs`) que ingiere un dataset artificial, ejecuta un replay y verifica una orden MARKET.

Para ejecutar la suite:

```bash
CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse cargo test
```

## Limitaciones actuales

- Motor de matching simplificado (sin order book ni fills parciales).
- Solo se simulan órdenes MARKET/LIMIT sin fees ni slippage configurables (placeholders para ampliar).
- Requiere datasets con columnas normalizadas (no detecta metadatos automáticamente).

## Licencia

MIT.


