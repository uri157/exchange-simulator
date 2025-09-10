# scripts/analyze_trades.py
from __future__ import annotations

import argparse
import duckdb
import pandas as pd


def main():
    p = argparse.ArgumentParser(description="Enriquecer fills de un run con features y guardarlos en DuckDB")
    p.add_argument("--duckdb-path", default="data/duckdb/exsim.duckdb")
    p.add_argument("--run-id", required=True, help="UUID del run")
    p.add_argument("--set-id", required=True, help="Feature set id (de feature_sets)")
    p.add_argument("--interval", required=True, help="TF base del feature set (ej: 1h, 1m)")
    p.add_argument("--replace", action="store_true", help="Borrar filas existentes para (run_id,set_id) en trade_features")
    args = p.parse_args()

    con = duckdb.connect(args.duckdb_path)

    features_table = f"features_{args.interval.lower()}"

    # asegurar tabla destino
    con.sql("""
      CREATE TABLE IF NOT EXISTS trade_features(
        run_id   UUID    NOT NULL,
        seq      BIGINT  NOT NULL,
        ts       BIGINT  NOT NULL,
        symbol   VARCHAR NOT NULL,
        side     VARCHAR NOT NULL,
        price    DOUBLE  NOT NULL,
        qty      DOUBLE  NOT NULL,
        realized_pnl DOUBLE DEFAULT 0.0,
        fee      DOUBLE DEFAULT 0.0,
        is_maker BOOLEAN DEFAULT FALSE,
        set_id   VARCHAR NOT NULL,
        features JSON,
        PRIMARY KEY (run_id, seq, set_id)
      );
    """)

    # chequeos
    has_features = con.sql(f"SELECT COUNT(*) AS n FROM {features_table} WHERE set_id = ?", [args.set_id]).fetchone()[0]
    if has_features == 0:
        print(f"[WARN] No hay features para set_id={args.set_id} en {features_table}")
    has_fills = con.sql("SELECT COUNT(*) AS n FROM trades_fills WHERE run_id = ?", [args.run_id]).fetchone()[0]
    if has_fills == 0:
        print(f"[WARN] No hay fills para run_id={args.run_id}")

    if args.replace:
        con.execute("DELETE FROM trade_features WHERE run_id = ? AND set_id = ?", [args.run_id, args.set_id])

    # join por (symbol, ts). OJO: asumimos que los features se indexan por close_ts (make_features align=close)
    con.execute(f"""
      INSERT INTO trade_features
      SELECT
        f.run_id, f.seq, f.ts, f.symbol, f.side, f.price, f.qty, f.realized_pnl, f.fee, f.is_maker,
        ? AS set_id,
        ft.data AS features
      FROM trades_fills f
      JOIN {features_table} ft
        ON ft.set_id = ?
       AND ft.symbol = f.symbol
       AND ft.ts = f.ts
      WHERE f.run_id = ?
      ON CONFLICT (run_id, seq, set_id) DO UPDATE SET
        price=excluded.price,
        qty=excluded.qty,
        realized_pnl=excluded.realized_pnl,
        fee=excluded.fee,
        is_maker=excluded.is_maker,
        features=excluded.features
    """, [args.set_id, args.set_id, args.run_id])

    # Resumen
    df = con.sql("""
      SELECT COUNT(*) AS rows_inserted
      FROM trade_features
      WHERE run_id = ? AND set_id = ?
    """, [args.run_id, args.set_id]).df()
    print("\n[OK] Filas en trade_features para este run/set:", df.iloc[0]["rows_inserted"])

    sample = con.sql("""
      SELECT run_id, seq, ts, symbol, side, price, qty,
             realized_pnl, fee, is_maker, set_id, features
      FROM trade_features
      WHERE run_id = ? AND set_id = ?
      ORDER BY seq
      LIMIT 10
    """, [args.run_id, args.set_id]).df()
    if not sample.empty:
        print("\nPrimeras filas:")
        print(sample)
    else:
        print("\n[INFO] No hay filas para mostrar (¿no coincidieron ts?)")


if __name__ == "__main__":
    main()
