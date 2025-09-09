# scripts/init_duckdb.py
from __future__ import annotations

import argparse
import os
from pathlib import Path

import duckdb
try:
    import yaml
except Exception:
    yaml = None

from storage.duckdb_store import ensure_schema

DEFAULT_DB_PATH = "data/duckdb/exsim.duckdb"

def _config_path_from_yaml() -> str:
    cfg = Path("default.yaml")
    if not cfg.exists() or yaml is None:
        return DEFAULT_DB_PATH
    with cfg.open("r") as f:
        data = yaml.safe_load(f) or {}
    return (data.get("duckdb") or {}).get("path", DEFAULT_DB_PATH)

def main():
    parser = argparse.ArgumentParser(description="Init DuckDB schema")
    parser.add_argument("--db", type=str, default=None, help="Ruta del .duckdb (override)")
    args = parser.parse_args()

    db_path = args.db or _config_path_from_yaml()
    con = ensure_schema(db_path)

    # Mostrar tablas creadas
    print(f"[OK] Schema creado en: {db_path}")
    print("[INFO] Tablas:")
    print(con.execute("SHOW TABLES").fetchdf())

    # Sanidad mínima: describir columnas clave
    for t in ("ohlc_1m", "funding", "feature_sets", "features_1m", "runs", "trades_fills", "equity_curve"):
        print(f"\n[DESCRIBE] {t}")
        print(con.execute(f"DESCRIBE {t}").fetchdf())

if __name__ == "__main__":
    main()
