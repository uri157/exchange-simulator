# scripts/load_to_duckdb.py
from __future__ import annotations

import argparse
import os
from datetime import datetime, timezone
from typing import Any, Dict, List, Optional, Tuple

from storage.duckdb_store import ensure_schema

# fuentes (ya las venís usando)
from data import binance_api, binance_files


def _parse_date_ms(d: str) -> int:
    d = d.strip().replace("Z", "")
    try:
        dt = datetime.fromisoformat(d)
    except Exception:
        dt = datetime.strptime(d, "%Y-%m-%d")
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)


def _fetch_klines_and_funding(source: str, symbol: str, interval: str, start_ms: Optional[int], end_ms: Optional[int]):
    if source == "files":
        ds = binance_files.BinanceFileData()
        kl = ds.get_klines(symbol, interval, startTime=start_ms, endTime=end_ms)
        fr = ds.get_funding_rates(symbol, startTime=start_ms, endTime=end_ms)
        return kl, fr
    elif source == "api":
        kl = binance_api.get_klines(symbol, interval, startTime=start_ms, endTime=end_ms)
        fr = binance_api.get_funding_rates(symbol, startTime=start_ms, endTime=end_ms)
        return kl, fr
    else:
        raise ValueError("source must be 'api' or 'files'")


def _upsert_ohlc(con, symbol: str, tf: str, klines: List[List[Any]], mirror_1m: bool):
    if not klines:
        return 0

    start_ms = int(klines[0][0])
    end_ms   = int(klines[-1][0])
    # limpiamos rango para idempotencia
    con.execute("DELETE FROM ohlc WHERE symbol=? AND tf=? AND ts BETWEEN ? AND ?", [symbol, tf, start_ms, end_ms])

    rows = [
        (
            symbol, tf,
            int(r[0]),
            float(r[1]), float(r[2]), float(r[3]), float(r[4]),
            float(r[5]),
            int(r[6]),
        )
        for r in klines
        if len(r) >= 7
    ]
    con.executemany(
        "INSERT INTO ohlc (symbol, tf, ts, open, high, low, close, volume, close_ts) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        rows,
    )

    # espejo opcional en ohlc_1m si tf == '1m'
    if mirror_1m and tf == "1m":
        con.execute("DELETE FROM ohlc_1m WHERE symbol=? AND ts BETWEEN ? AND ?", [symbol, start_ms, end_ms])
        rows_1m = [(symbol, r[2], r[3], r[4], r[5], r[6], r[7], r[8]) for r in rows]
        con.executemany(
            "INSERT INTO ohlc_1m (symbol, ts, open, high, low, close, volume, close_ts) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            rows_1m,
        )

    return len(rows)


def _upsert_funding(con, symbol: str, funding_rows: List[Dict[str, Any]]):
    if not funding_rows:
        return 0
    start_ms = int(funding_rows[0]["fundingTime"])
    end_ms   = int(funding_rows[-1]["fundingTime"])
    con.execute("DELETE FROM funding WHERE symbol=? AND funding_time BETWEEN ? AND ?", [symbol, start_ms, end_ms])
    rows = [(symbol, int(r["fundingTime"]), float(r["fundingRate"])) for r in funding_rows]
    con.executemany(
        "INSERT INTO funding (symbol, funding_time, funding_rate) VALUES (?, ?, ?)",
        rows,
    )
    return len(rows)


def main():
    parser = argparse.ArgumentParser(description="Carga OHLC+Funding a DuckDB")
    parser.add_argument("--db", type=str, default="data/duckdb/exsim.duckdb", help="Ruta .duckdb")
    parser.add_argument("--source", choices=["api", "files"], default="files", help="Origen de datos")
    parser.add_argument("--symbol", required=True)
    parser.add_argument("--interval", required=True, help="Ej: 1m, 1h, 4h, 1d")
    parser.add_argument("--start", type=str, default=None, help="YYYY-MM-DD o ISO (UTC)")
    parser.add_argument("--end", type=str, default=None, help="YYYY-MM-DD o ISO (UTC)")
    parser.add_argument("--mirror-1m", action="store_true", help="También escribir en ohlc_1m si tf=1m")
    args = parser.parse_args()

    start_ms = _parse_date_ms(args.start) if args.start else None
    end_ms   = _parse_date_ms(args.end) if args.end else None

    con = ensure_schema(args.db)

    print(f"[INFO] Leyendo {args.symbol} {args.interval} {args.start} -> {args.end} desde {args.source}...")
    klines, funding = _fetch_klines_and_funding(args.source, args.symbol, args.interval, start_ms, end_ms)
    print(f"[INFO] kline rows: {len(klines)}, funding rows: {len(funding)}")

    # ordenamos por ts por las dudas
    klines = sorted(klines, key=lambda r: int(r[0]))
    funding = sorted(funding, key=lambda r: int(r["fundingTime"]))

    n_ohlc = _upsert_ohlc(con, args.symbol, args.interval, klines, args.mirror_1m)
    n_fund = _upsert_funding(con, args.symbol, funding)

    # chequeo rápido
    cnt = con.execute(
        "SELECT count(*) FROM ohlc WHERE symbol=? AND tf=?"
        + (" AND ts BETWEEN ? AND ?" if start_ms and end_ms else ""),
        [args.symbol, args.interval] + ([start_ms, end_ms] if start_ms and end_ms else []),
    ).fetchone()[0]

    print(f"[OK] Insertados OHLC: {n_ohlc}, Funding: {n_fund}")
    print(f"[CHECK] ohlc rows ahora: {cnt}")


if __name__ == "__main__":
    main()
