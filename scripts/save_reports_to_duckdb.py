# scripts/save_reports_to_duckdb.py
from __future__ import annotations

import argparse, csv, json, uuid
from datetime import datetime
from pathlib import Path

import duckdb

def parse_bool(s: str) -> bool:
    return str(s).strip().lower() in ("1", "true", "t", "yes", "y")

def main():
    p = argparse.ArgumentParser(description="Guarda ./reports en DuckDB (runs, trades_fills, equity_curve).")
    p.add_argument("--duckdb-path", required=True, help="Ruta a la DB DuckDB (p.ej. data/duckdb/exsim.duckdb)")
    p.add_argument("--run-id", default=None, help="UUID opcional; si no se pasa se genera uno")
    args = p.parse_args()

    db = args.duckdb_path
    run_id = args.run_id or str(uuid.uuid4())

    reports = Path("reports")
    summary_fp = reports / "summary.json"
    trades_fp  = reports / "trades.csv"
    equity_fp  = reports / "equity.csv"

    if not summary_fp.exists() or not trades_fp.exists() or not equity_fp.exists():
        raise SystemExit("Faltan archivos en ./reports (summary.json, trades.csv, equity.csv). Ejecutá un backtest primero.")

    with summary_fp.open() as f:
        summary = json.load(f)

    strategy = summary.get("strategy")
    params_json = json.dumps(summary.get("strategy_params", {}), ensure_ascii=False)

    con = duckdb.connect(db)
    con.execute("BEGIN")

    # Inserta run
    con.execute(
        """
        INSERT INTO runs (run_id, created_at, strategy, params_json, feature_set_id, code_hash)
        VALUES (?, now(), ?, ?, NULL, NULL)
        """,
        [run_id, strategy, params_json],
    )

    # Inserta trades_fills
    with trades_fp.open(newline="") as f:
        reader = csv.DictReader(f)
        seq = 0
        rows = []
        for r in reader:
            seq += 1
            rows.append((
                run_id,
                seq,
                int(float(r["timestamp"])) if r["timestamp"] else 0,
                r["symbol"],
                r["side"],
                float(r["price"]),
                float(r["quantity"]),
                float(r.get("realized_pnl", 0.0) or 0.0),
                float(r.get("fee", 0.0) or 0.0),
                parse_bool(r.get("is_maker", "false")),
            ))
        if rows:
            con.executemany(
                """INSERT INTO trades_fills
                   (run_id, seq, ts, symbol, side, price, qty, realized_pnl, fee, is_maker)
                   VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)""",
                rows,
            )

    # Inserta equity_curve
    with equity_fp.open(newline="") as f:
        reader = csv.DictReader(f)
        rows = [(run_id, int(float(r["timestamp"])), float(r["equity"])) for r in reader]
        if rows:
            con.executemany(
                "INSERT INTO equity_curve (run_id, ts, equity) VALUES (?, ?, ?)",
                rows,
            )

    con.execute("COMMIT")
    con.close()

    print(f"[OK] Guardado run_id={run_id} en {db}")

if __name__ == "__main__":
    main()
