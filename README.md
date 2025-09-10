# Exchange Simulator + Backtesting (con DuckDB)

> Un “mini-exchange” local para **backtestear** estrategias de futuros usando velas históricas (desde CSV o API) y persistiendo todo en **DuckDB**.
> Incluye: cargador de datos, runner de backtests, almacenamiento de resultados (trades + equity), builder de features (EMA/RSI, etc.) y un **gateway** que emula endpoints de exchange para conectar bots.

---

## TL;DR (rápido para correr)

```bash
# 1) Crear venv e instalar deps
python3 -m venv .venv
source .venv/bin/activate
python -m pip install -U pip setuptools wheel
python -m pip install -r requirements.txt

# 2) Inicializar DB (crea esquema)
python -m scripts.init_duckdb

# 3) Cargar datos (ej: 1h desde archivos locales o API)
#    - desde archivos:  --source files
#    - desde API:       --source api
python -m scripts.load_to_duckdb --db data/duckdb/exsim.duckdb \
  --source files --symbol BTCUSDT --interval 1h \
  --start 2024-07-01 --end 2024-07-05

# 4) Correr un backtest (lee de DuckDB)
python -m backtests.bt_runner \
  --symbol BTCUSDT --interval 1h \
  --start 2024-07-01 --end 2024-07-05 \
  --data-source duckdb --duckdb-path data/duckdb/exsim.duckdb \
  --fill-model ohlc_up \
  --strategy backtests.strategies.sma:SMA \
  --strategy-params '{"fast":5,"slow":20,"qty":0.002}'

# 5) (Opcional) Features técnicas
python -m scripts.make_features \
  --duckdb-path data/duckdb/exsim.duckdb \
  --symbols BTCUSDT --interval 1h \
  --start 2024-07-01 --end 2024-07-05 \
  --ema 5,20,50 --rsi 14 \
  --align close \
  --set-id demo_1h_ema_rsi --replace

# 6) Consultar resultados en SQL
python - <<'PY'
import duckdb
con = duckdb.connect("data/duckdb/exsim.duckdb")
print(con.sql("SELECT run_id, strategy, created_at FROM runs ORDER BY created_at DESC LIMIT 5").df())
PY
```

---

## ¿Qué problema resuelve?

* Poder **iterar rápido** estrategias sin depender de un exchange real ni demoras de mercado.
* Guardar **todo el histórico** (OHLC + funding) en un formato eficiente (DuckDB).
* Ejecutar backtests reproducibles, versionar estrategias y **persistir runs** (trades + equity).
* Construir **features** (EMAs, RSI, etc.) para análisis y filtros post-trade.
* (Gateway) Emular un exchange para que los **bots reales** se conecten “como si” fuera Binance, pero leyendo del histórico local.

---

## Arquitectura (carpetas clave)

```
.
├── backtests/
│   ├── bt_runner.py          # Runner de backtests (CLI)
│   ├── strategy_api.py       # Contrato de estrategia (hooks)
│   └── strategies/
│       └── sma.py            # Ejemplo: cruce de medias
├── data/
│   ├── duckdb/               # DB file (exsim.duckdb)
│   ├── binance_api.py        # Pull directo desde API
│   ├── binance_files.py      # Loader desde CSVs locales
│   └── duckdb_source.py      # Loader desde DuckDB (lectura)
├── gateway/
│   └── sim_gateway.py        # Gateway REST/WS que emula exchange (subset)
├── scripts/
│   ├── init_duckdb.py        # Crea esquema DuckDB
│   ├── load_to_duckdb.py     # Carga OHLC/Funding (API o files) → DB
│   ├── make_features.py      # Calcula EMAs/RSI y las guarda en DB
│   └── analyze_trades.py     # Ejemplos de análisis con SQL/joins
├── sim/
│   ├── exchange_sim.py       # Motor de fills y posiciones
│   ├── fill_models.py        # Modelos de ejecución (OHLC, random, book)
│   └── models.py             # Bar/Order/Position/Trade models
└── replay/
    └── replayer.py           # Utilidades para “reproducir” series
```

---

## Dependencias

* Python 3.12+
* `duckdb`, `pandas`, `pyarrow`, `numpy`, `requests`, `PyYAML`, `pytest`
  (Se instalan con `pip -r requirements.txt`)

---

## Datos: fuentes y formatos

### Fuentes disponibles

* `api`: descarga candles y funding desde endpoints tipo Binance.
* `files`: lee CSVs locales (por si ya descargaste datos antes).
* `duckdb`: lectura directa desde `data/duckdb/exsim.duckdb`.

### Transformación a formato “Binance-like”

Los loaders exponen:

* `get_klines(symbol, interval, startTime, endTime)` →
  `[openTime, open, high, low, close, volume, closeTime]`

* `get_funding_rates(symbol, startTime, endTime)` →
  `[{"fundingTime": ..., "fundingRate": ...}, ...]`

Así el **runner** y los **fill models** no dependen de la fuente.

---

## Esquema de DuckDB

Se crea con `scripts.init_duckdb`. Tablas principales:

* **ohlc**: barras multi-TF (usada para 1h en los ejemplos)
  `symbol VARCHAR, ts BIGINT (open), open DOUBLE, high DOUBLE, low DOUBLE, close DOUBLE, volume DOUBLE, close_ts BIGINT`

* **ohlc\_1m**: barras 1m (si usás espejo/minuto)
  mismas columnas que `ohlc`.

* **funding**:
  `symbol VARCHAR, funding_time BIGINT, funding_rate DOUBLE`

* **runs** (metadata del backtest):
  `run_id UUID, created_at TIMESTAMP, strategy VARCHAR, params_json JSON, feature_set_id VARCHAR, code_hash VARCHAR`

* **trades\_fills** (todas las ejecuciones):
  `run_id UUID, seq BIGINT, ts BIGINT, symbol VARCHAR, side VARCHAR, price DOUBLE, qty DOUBLE, realized_pnl DOUBLE, fee DOUBLE, is_maker BOOLEAN`

* **equity\_curve**:
  `run_id UUID, ts BIGINT, equity DOUBLE`

* **feature\_sets**:
  `set_id VARCHAR, created_at TIMESTAMP, base_tf VARCHAR, params_json JSON`

* **features\_<tf>** (p.ej. `features_1h`, `features_1m`):
  `set_id VARCHAR, symbol VARCHAR, ts BIGINT, data JSON`
  (en `data` guardamos EMAs/RSI, etc. Acceso vía `json_extract`).

> Nota: si tu proceso crea `features_1h` y la tabla no existe, el script la crea “on demand”.

---

## Backtests

### Runner (CLI)

```
python -m backtests.bt_runner \
  --symbol BTCUSDT --interval 1h \
  --start 2024-07-01 --end 2024-07-05 \
  --data-source duckdb --duckdb-path data/duckdb/exsim.duckdb \
  --fill-model ohlc_up \
  --strategy backtests.strategies.sma:SMA \
  --strategy-params '{"fast":5,"slow":20,"qty":0.002}'
```

**Parámetros clave**

* `--data-source`: `api | files | duckdb`
* `--duckdb-path`: path a la DB (si usás `duckdb`)
* `--fill-model`: `ohlc_up | ohlc_down | random | book`
* `--strategy`: ruta dinámica `"modulo.submodulo:Clase"`
* `--strategy-params`: JSON (cantidad, ventanas, etc.)
* Fees/Slippage: `--maker-bps`, `--taker-bps`, `--slippage-bps`
* Repro: `--seed`

**Outputs**

* `reports/trades.csv`
* `reports/equity.csv`
* `reports/summary.json`
* Además, el runner **guarda automáticamente** la run en DuckDB (`runs`, `trades_fills`, `equity_curve`) y te imprime el `Run ID`.

### Estrategias (plugin)

Contrato (ver `backtests/strategy_api.py`):

* `on_start(self)`
* `on_bar(self, bar: Bar)`  ← hook principal por vela
* `on_finish(self)`

Ejemplo incluido: `backtests.strategies.sma:SMA`.
Modelo de barra en `sim/models.py` (`Bar.open, high, low, close, volume, open_time, close_time`).

### Fill models

En `sim/fill_models.py`:

* `OHLCPathFill(up_first=True/False, slippage_bps=...)`
  Simula recorrido intra-bar determinista.
* `RandomOHLC(seed, slippage_bps)`
* `BookTickerFill()` (stub para book-based; opcional usar data\_source).

---

## Carga de datos (scripts)

* **Init DB**

  ```
  python -m scripts.init_duckdb
  ```

* **Cargar OHLC/Funding → DB**

  ```
  python -m scripts.load_to_duckdb --db data/duckdb/exsim.duckdb \
    --source api|files --symbol BTCUSDT --interval 1h \
    --start 2024-07-01 --end 2024-07-05
  ```

* **Backfill mensual 1m (bash)**

  ```bash
  DB=data/duckdb/exsim.duckdb
  SYM=BTCUSDT
  TF=1m
  mkdir -p logs

  for Y in {2018..2025}; do
    for M in $(seq -w 1 12); do
      START="${Y}-${M}-01"
      if [ "$(date -d "$START" +%s)" -gt "$(date +%s)" ]; then break 2; fi
      END=$(date -d "$START +1 month -1 day" +%F)
      echo "[$(date -u +%FT%TZ)] Cargando ${SYM} ${TF} ${START} -> ${END}"
      python -m scripts.load_to_duckdb \
        --db "$DB" --source api \
        --symbol "$SYM" --interval "$TF" \
        --start "$START" --end "$END" \
        | tee -a "logs/backfill_${SYM}_${TF}.log"
    done
  done
  ```

* **Features técnicas**

  ```
  python -m scripts.make_features \
    --duckdb-path data/duckdb/exsim.duckdb \
    --symbols BTCUSDT --interval 1h \
    --start 2024-07-01 --end 2024-07-05 \
    --ema 5,20,50 --rsi 14 --align close \
    --set-id demo_1h_ema_rsi --replace
  ```

* **Análisis de fills (ejemplos)**

  ```
  python -m scripts.analyze_trades --help
  ```

---

## Gateway (emulación de exchange)

Módulo: `gateway/sim_gateway.py`
Objetivo: exponer una **interfaz REST/WS compatible (subset)** para que bots externos se conecten como si fuera un exchange y consuman histórico/stream generado desde DuckDB.

Uso típico:

```bash
python -m gateway.sim_gateway --help
python -m gateway.sim_gateway --duckdb-path data/duckdb/exsim.duckdb \
  --symbol BTCUSDT --interval 1m --speed 2x
```

> Nota: los endpoints/streams disponibles pueden consultarse con `-h`. La idea es entregar **klines/bookTicker** en “replay” con control de velocidad y cortes por rango.

---

## Consultas útiles (SQL)

```python
import duckdb
con = duckdb.connect("data/duckdb/exsim.duckdb")

# Últimos runs
con.sql("SELECT run_id, strategy, created_at FROM runs ORDER BY created_at DESC LIMIT 5").df()

# Resumen de fills/fees de un run
RUN = "<tu-run-id>"
con.sql(f"""
  SELECT COUNT(*) AS fills,
         SUM(fee) AS total_fees,
         SUM(realized_pnl) AS realized_pnl
  FROM trades_fills
  WHERE run_id = '{RUN}'
""").df()

# Última equity del run
con.sql(f"""
  SELECT equity FROM equity_curve
  WHERE run_id = '{RUN}'
  ORDER BY ts DESC LIMIT 1
""").df()

# Join con features (ej: EMA/RSI en features_1h)
SET='demo_1h_ema_rsi'
con.sql(f"""
WITH j AS (
  SELECT f.ts, f.realized_pnl,
         json_extract(e.data, '$.ema_5')  AS ema_5,
         json_extract(e.data, '$.ema_20') AS ema_20,
         json_extract(e.data, '$.rsi_14') AS rsi_14
  FROM trades_fills f
  JOIN features_1h e
    ON e.ts = f.ts AND e.symbol = f.symbol AND e.set_id = '{SET}'
  WHERE f.run_id = '{RUN}'
)
SELECT (ema_5 > ema_20) AS ema5_gt_ema20,
       COUNT(*) AS fills,
       SUM(realized_pnl) AS sum_realized,
       AVG(realized_pnl) AS avg_realized,
       AVG(rsi_14) AS avg_rsi
FROM j
GROUP BY 1
ORDER BY 1 DESC
""").df()
```

---

## Buenas prácticas / Git

* **No commitear** el entorno `.venv/` ni la base `exsim.duckdb` (pueden ser grandes).
  Asegurate que `.gitignore` incluya:

  ```
  .venv/
  data/duckdb/*.duckdb
  logs/
  __pycache__/
  *.pyc
  ```
* Si “metiste la pata” y agregaste `.venv/` al repo:

  ```bash
  git rm -r --cached .venv
  git commit -m "remove venv from git history (cached)"
  ```

  (Si hace falta limpiar historia completa, usar `git filter-repo`/`filter-branch` fuera del alcance de este README.)

---

## Troubleshooting

* **PEP 668 / “externally-managed-environment”**
  Usá venv: `python3 -m venv .venv && source .venv/bin/activate`.

* **DuckDB “read\_only”**
  Si ves `Connection Error: ... different configuration` o no puede escribir:
  cerrá conexiones previas y reabrí sin `read_only=True`.

* **`python` vs `python3`**
  Dentro del venv, el binario se llama `python`. Fuera, puede ser `python3`.

* **Rate limits de API**
  El backfill mensual ya respeta ventanas; si Binance corta, relanzá el mes afectado: el `INSERT ... ON CONFLICT` (o equivalente) evita duplicados.

---

## Roadmap (breve)

* Gateway Binance-compatible (más endpoints, órdenes simuladas, posiciones).
* Más fill models (slippage dinámico, impacto por volumen).
* Librería de indicadores ampliada (BB, ATR, MACD, etc.).
* Explorador de runs/estrategias en UI ligera.

---

## Licencia

(Define la que prefieras para el repo.)

---

## Créditos

Hecho para iterar estrategias de forma rápida, reproducible y “offline-friendly” ✨. Si necesitás ampliar el gateway o agregar nuevos indicadores, ya está todo preparado para enchufar módulos sin romper el resto.
