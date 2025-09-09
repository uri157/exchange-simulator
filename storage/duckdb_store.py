# storage/duckdb_store.py
from __future__ import annotations

import duckdb
from pathlib import Path

# Tabla genérica multi-timeframe (recomendada)
OHLC_GENERIC_DDL = """
CREATE TABLE IF NOT EXISTS ohlc (
  symbol   TEXT    NOT NULL,
  tf       TEXT    NOT NULL,   -- '1m', '1h', etc.
  ts       BIGINT  NOT NULL,   -- open time (ms)
  open     DOUBLE  NOT NULL,
  high     DOUBLE  NOT NULL,
  low      DOUBLE  NOT NULL,
  close    DOUBLE  NOT NULL,
  volume   DOUBLE  NOT NULL,
  close_ts BIGINT  NOT NULL,   -- close time (ms)
  PRIMARY KEY(symbol, tf, ts)
);
"""

# Mantengo la de 1m por conveniencia (opcionalmente la espejamos)
OHLC_1M_DDL = """
CREATE TABLE IF NOT EXISTS ohlc_1m (
  symbol   TEXT    NOT NULL,
  ts       BIGINT  NOT NULL,  -- open time (ms)
  open     DOUBLE  NOT NULL,
  high     DOUBLE  NOT NULL,
  low      DOUBLE  NOT NULL,
  close    DOUBLE  NOT NULL,
  volume   DOUBLE  NOT NULL,
  close_ts BIGINT  NOT NULL,  -- close time (ms)
  PRIMARY KEY(symbol, ts)
);
"""

FUNDING_DDL = """
CREATE TABLE IF NOT EXISTS funding (
  symbol        TEXT   NOT NULL,
  funding_time  BIGINT NOT NULL, -- ms
  funding_rate  DOUBLE NOT NULL,
  PRIMARY KEY(symbol, funding_time)
);
"""

FEATURE_SETS_DDL = """
CREATE TABLE IF NOT EXISTS feature_sets (
  set_id      TEXT      PRIMARY KEY,
  created_at  TIMESTAMP DEFAULT now(),
  base_tf     TEXT      NOT NULL,      -- ej. '1m'
  params_json JSON
);
"""

FEATURES_1M_DDL = """
CREATE TABLE IF NOT EXISTS features_1m (
  set_id   TEXT    NOT NULL,
  symbol   TEXT    NOT NULL,
  ts       BIGINT  NOT NULL,  -- ms (alineado con ohlc_1m.ts)
  data     JSON,
  PRIMARY KEY (set_id, symbol, ts)
);
"""

RUNS_DDL = """
CREATE TABLE IF NOT EXISTS runs (
  run_id         UUID      PRIMARY KEY,
  created_at     TIMESTAMP DEFAULT now(),
  strategy       TEXT,
  params_json    JSON,
  feature_set_id TEXT,
  code_hash      TEXT
);
"""

TRADES_FILLS_DDL = """
CREATE TABLE IF NOT EXISTS trades_fills (
  run_id      UUID    NOT NULL,
  seq         BIGINT  NOT NULL,
  ts          BIGINT  NOT NULL,    -- ms
  symbol      TEXT    NOT NULL,
  side        TEXT    NOT NULL,    -- 'BUY'/'SELL'
  price       DOUBLE  NOT NULL,
  qty         DOUBLE  NOT NULL,
  realized_pnl DOUBLE DEFAULT 0.0,
  fee         DOUBLE  DEFAULT 0.0,
  is_maker    BOOLEAN DEFAULT FALSE,
  PRIMARY KEY (run_id, seq)
);
"""

EQUITY_CURVE_DDL = """
CREATE TABLE IF NOT EXISTS equity_curve (
  run_id   UUID    NOT NULL,
  ts       BIGINT  NOT NULL, -- ms
  equity   DOUBLE  NOT NULL,
  PRIMARY KEY (run_id, ts)
);
"""

def ensure_schema(db_path: str) -> duckdb.DuckDBPyConnection:
    Path(db_path).parent.mkdir(parents=True, exist_ok=True)
    con = duckdb.connect(db_path)
    con.execute("PRAGMA threads = " + str(max(1, __import__('os').cpu_count() or 1)))

    # Esquema de mercado
    con.execute(OHLC_GENERIC_DDL)
    con.execute(OHLC_1M_DDL)
    con.execute(FUNDING_DDL)

    # Esquema de features y runs
    con.execute(FEATURE_SETS_DDL)
    con.execute(FEATURES_1M_DDL)
    con.execute(RUNS_DDL)
    con.execute(TRADES_FILLS_DDL)
    con.execute(EQUITY_CURVE_DDL)
    return con
