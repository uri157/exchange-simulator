# scripts/make_features.py
from __future__ import annotations

import argparse
import json
import time
from typing import Dict, List, Optional

import duckdb
import pandas as pd
import numpy as np


def _parse_int_list(s: Optional[str]) -> List[int]:
    if not s:
        return []
    return [int(x.strip()) for x in s.split(",") if x.strip()]


def _table_for_interval(interval: str) -> str:
    i = interval.lower()
    if i in ("1m", "1min", "1"):
        return "ohlc_1m"
    return "ohlc"  # para 1h, 4h, 1d cargado por scripts/load_to_duckdb


def _ema(s: pd.Series, n: int) -> pd.Series:
    return s.ewm(span=n, adjust=False, min_periods=n).mean()


def _rsi(s: pd.Series, n: int) -> pd.Series:
    delta = s.diff()
    gain = delta.clip(lower=0)
    loss = -delta.clip(upper=0)
    avg_gain = gain.ewm(alpha=1.0/n, adjust=False, min_periods=n).mean()
    avg_loss = loss.ewm(alpha=1.0/n, adjust=False, min_periods=n).mean()
    rs = avg_gain / avg_loss
    rsi = 100.0 - (100.0 / (1.0 + rs))
    return rsi


def main():
    p = argparse.ArgumentParser(description="Generar y guardar features en DuckDB")
    p.add_argument("--duckdb-path", default="data/duckdb/exsim.duckdb")
    p.add_argument("--symbols", required=True, help="Lista separada por coma, ej: BTCUSDT,ETHUSDT")
    p.add_argument("--interval", required=True, help="TF base de los OHLC (ej: 1h, 1m)")
    p.add_argument("--start", default=None, help="YYYY-MM-DD o ISO; opcional")
    p.add_argument("--end", default=None, help="YYYY-MM-DD o ISO; opcional")
    p.add_argument("--ema", default="5,20,50", help="EMAs (coma). Ej: 5,20,50")
    p.add_argument("--rsi", default="14", help="RSI periods (coma). Ej: 14,28")
    p.add_argument("--align", choices=["close", "open"], default="close",
                   help="Timestamp a usar para features (close|open)")
    p.add_argument("--set-id", default=None, help="ID del feature set. Si no, se genera auto_*")
    p.add_argument("--replace", action="store_true", help="Borra filas existentes del set_id antes de insertar")
    args = p.parse_args()

    symbols = [s.strip() for s in args.symbols.split(",") if s.strip()]
    ema_n = _parse_int_list(args.ema)
    rsi_n = _parse_int_list(args.rsi)

    if args.set_id:
        set_id = args.set_id
    else:
        set_id = f"auto_{args.interval}_{int(time.time())}"

    params = {
        "interval": args.interval,
        "ema": ema_n,
        "rsi": rsi_n,
        "align": args.align,
        "price_source": "close",
    }

    con = duckdb.connect(args.duckdb_path)  # RW
    tf_table = _table_for_interval(args.interval)
    features_table = f"features_{args.interval.lower()}"

    # Asegurar tablas
    con.sql(f"""
      CREATE TABLE IF NOT EXISTS feature_sets(
        set_id      VARCHAR PRIMARY KEY,
        created_at  TIMESTAMP DEFAULT now(),
        base_tf     VARCHAR NOT NULL,
        params_json JSON
      );
    """)
    con.sql(f"""
      CREATE TABLE IF NOT EXISTS {features_table}(
        set_id VARCHAR NOT NULL,
        symbol VARCHAR NOT NULL,
        ts     BIGINT  NOT NULL,
        data   JSON,
        PRIMARY KEY (set_id, symbol, ts)
      );
    """)

    # Upsert del feature set
    con.execute(
        """
        INSERT INTO feature_sets(set_id, base_tf, params_json)
        VALUES (?, ?, ?)
        ON CONFLICT (set_id) DO UPDATE SET
          base_tf=excluded.base_tf,
          params_json=excluded.params_json;
        """,
        [set_id, args.interval, json.dumps(params)],
    )

    if args.replace:
        con.execute(f"DELETE FROM {features_table} WHERE set_id = ?", [set_id])

    # Rango temporal (opcional)
    start_ts = None
    end_ts = None
    if args.start:
        start_ts = int(pd.Timestamp(args.start, tz="UTC").timestamp() * 1000)
    if args.end:
        end_ts = int(pd.Timestamp(args.end, tz="UTC").timestamp() * 1000)

    # Cargar y procesar por símbolo
    total_rows = 0
    for sym in symbols:
        sql = f"""
          SELECT symbol, ts, close_ts, open, high, low, close, volume
          FROM {tf_table}
          WHERE symbol = ?
        """
        params_q = [sym]
        if start_ts is not None:
            sql += " AND ts >= ?"
            params_q.append(start_ts)
        if end_ts is not None:
            sql += " AND ts <= ?"
            params_q.append(end_ts)
        sql += " ORDER BY ts ASC"

        df = con.execute(sql, params_q).df()
        if df.empty:
            print(f"[WARN] Sin OHLC para {sym} en {args.interval}")
            continue

        df = df.sort_values("ts").reset_index(drop=True)
        price = df["close"].astype(float)

        # EMAs
        for n in ema_n:
            df[f"ema_{n}"] = _ema(price, n)

        # RSI
        for n in rsi_n:
            df[f"rsi_{n}"] = _rsi(price, n)

        # Construir JSON por fila (omite NaN)
        feat_cols = [c for c in df.columns if c.startswith("ema_") or c.startswith("rsi_")]
        def _row_json(row) -> Dict[str, float]:
            out = {}
            for c in feat_cols:
                v = row[c]
                if pd.notna(v) and np.isfinite(v):
                    out[c] = float(v)
            return out

        if args.align == "close":
            ts_col = "close_ts"
        else:
            ts_col = "ts"

        out_rows = []
        for _, r in df.iterrows():
            j = _row_json(r)
            if not j:
                # si querés guardar con nulos, podrías hacerlo; aquí omitimos filas sin features válidos
                continue
            out_rows.append([set_id, sym, int(r[ts_col]), json.dumps(j)])

        if not out_rows:
            print(f"[WARN] Sin filas con features válidos para {sym}")
            continue

        out_df = pd.DataFrame(out_rows, columns=["set_id", "symbol", "ts", "data"])
        con.register("df_features", out_df)
        con.sql(f"INSERT INTO {features_table} SELECT set_id, symbol, ts, data::JSON FROM df_features")
        con.unregister("df_features")
        total_rows += len(out_rows)
        print(f"[OK] {sym}: insertadas {len(out_rows)} filas en {features_table} (set_id={set_id})")

    print(f"[DONE] set_id={set_id}, filas totales insertadas: {total_rows}")


if __name__ == "__main__":
    main()
