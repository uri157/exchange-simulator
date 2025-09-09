# scripts/make_local_data.py
from __future__ import annotations
import os, csv, argparse
from datetime import datetime, timezone
from typing import List, Dict, Any

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

def _ensure_dir(p: str) -> None:
    os.makedirs(p, exist_ok=True)

def _write_klines_candidates(symbol: str, interval: str, rows: List[List[Any]], start: str, end: str) -> List[str]:
    # Candidatos de ruta (escribimos a todos para cubrir distintos loaders)
    candidates = [
        f"data/files/klines/{symbol}_{interval}.csv",
        f"data/files/klines/{symbol}/{interval}.csv",
        f"data/files/klines/{symbol}/{interval}_{start}_{end}.csv",
    ]
    for path in candidates:
        _ensure_dir(os.path.dirname(path))
        with open(path, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["openTime","open","high","low","close","volume","closeTime"])
            for r in rows:
                # Binance: [openTime, open, high, low, close, volume, closeTime, ...]
                if len(r) < 7: 
                    continue
                w.writerow([r[0], r[1], r[2], r[3], r[4], r[5], r[6]])
    return candidates

def _write_funding_candidates(symbol: str, rows: List[Dict[str, Any]], start: str, end: str) -> List[str]:
    candidates = [
        f"data/files/funding/{symbol}.csv",
        f"data/files/funding/{symbol}_{start}_{end}.csv",
    ]
    for path in candidates:
        _ensure_dir(os.path.dirname(path))
        with open(path, "w", newline="") as f:
            w = csv.writer(f)
            w.writerow(["fundingTime","fundingRate"])
            for fr in sorted(rows, key=lambda x: int(x["fundingTime"])):
                w.writerow([fr["fundingTime"], fr["fundingRate"]])
    return candidates

def main(symbol: str, interval: str, start: str, end: str, verbose: bool=False) -> None:
    start_ms = _parse_date_ms(start)
    end_ms = _parse_date_ms(end)

    if verbose:
        print(f"[INFO] Descargando {symbol} {interval} {start} -> {end} desde API...")

    klines = binance_api.get_klines(symbol, interval, startTime=start_ms, endTime=end_ms)
    funding = binance_api.get_funding_rates(symbol, startTime=start_ms, endTime=end_ms)

    if verbose:
        print(f"[INFO] kline rows: {len(klines)}, funding rows: {len(funding)}")

    kpaths = _write_klines_candidates(symbol, interval, klines, start, end)
    fpaths = _write_funding_candidates(symbol, funding, start, end)

    print("[OK] Escribí klines en:")
    for p in kpaths: print(" -", p)
    print("[OK] Escribí funding en:")
    for p in fpaths: print(" -", p)

    # Verificación: ¿binance_files los puede leer?
    try:
        ds = binance_files.BinanceFileData()
        test_kl = ds.get_klines(symbol, interval, startTime=start_ms, endTime=end_ms)
        print(f"[CHECK] binance_files.get_klines(...) -> {len(test_kl)} filas")
        if len(test_kl) == 0:
            print("[WARN] El loader no encontró datos. Listado de archivos en data/files/klines:")
            for root, _, files in os.walk("data/files/klines"):
                for name in files:
                    print("  *", os.path.join(root, name))
    except Exception as e:
        print("[WARN] No pude verificar con binance_files:", e)

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("symbol")
    ap.add_argument("interval")
    ap.add_argument("start")
    ap.add_argument("end")
    ap.add_argument("--verbose", action="store_true")
    args = ap.parse_args()
    main(args.symbol, args.interval, args.start, args.end, verbose=args.verbose)
