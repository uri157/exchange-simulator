"""
ExSim — Binance Futures subset
Gateway entrypoint (CLI) → levanta FastAPI con uvicorn.

Uso rápido:
  python -m gateway.main \
    --duckdb-path data/duckdb/exsim.duckdb \
    --symbol BTCUSDT --interval 1m \
    --start 2024-07-01 --end 2024-07-05 \
    --speed 25 --maker-bps 1 --taker-bps 2 --port 9001

Variables de entorno (opcionales):
  HOST=0.0.0.0   PORT=9001
"""
from __future__ import annotations

import argparse
import os
from datetime import datetime, timezone

import uvicorn

from .app import create_app
from .core.models import ReplayConfig


# --------------------------- helpers ---------------------------

def _to_ms(s: str) -> int:
    """Convierte entero/ISO/fecha a epoch ms.
    - Si es dígito: asume s en ms o s (si < 1e10, multiplica por 1_000)
    - ISO 8601 con/ sin zona: "2024-07-01T00:00:00Z" | "2024-07-01 12:34:56"
    - Fecha YYYY-MM-DD (00:00:00Z)
    """
    s = s.strip()
    if s.isdigit():
        v = int(s)
        return v if v > 10_000_000_000 else v * 1000
    # ISO (admite Z)
    s_iso = s.replace("Z", "+00:00").replace(" ", "T")
    try:
        dt = datetime.fromisoformat(s_iso)
    except Exception:
        # Sólo fecha
        dt = datetime.strptime(s, "%Y-%m-%d")
        dt = dt.replace(tzinfo=timezone.utc)
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)


# ----------------------------- CLI ----------------------------

def build_argparser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(description="ExSim — Binance Futures Sim Gateway (REST + WS)")
    ap.add_argument("--duckdb-path", type=str, default=os.getenv("DUCKDB_PATH", "data/duckdb/exsim.duckdb"))
    ap.add_argument("--symbol", type=str, default=os.getenv("SYMBOL", "BTCUSDT"))
    ap.add_argument("--interval", type=str, default=os.getenv("INTERVAL", "1m"))
    ap.add_argument("--start", type=str, required=True, help="YYYY-MM-DD|ISO|epoch(ms|s)")
    ap.add_argument("--end", type=str, required=True, help="YYYY-MM-DD|ISO|epoch(ms|s)")
    ap.add_argument("--speed", type=float, default=float(os.getenv("SPEED", 10.0)), help="velas por segundo")
    ap.add_argument("--maker-bps", type=float, default=float(os.getenv("FEE_MAKER_BPS", 2.0)))
    ap.add_argument("--taker-bps", type=float, default=float(os.getenv("FEE_TAKER_BPS", 4.0)))
    ap.add_argument("--slippage-bps", type=float, default=float(os.getenv("SLIPPAGE_BPS", 0.0)))
    ap.add_argument("--starting-balance", type=float, default=float(os.getenv("STARTING_BALANCE", 100_000.0)))
    ap.add_argument("--host", type=str, default=os.getenv("HOST", "0.0.0.0"))
    ap.add_argument("--port", type=int, default=int(os.getenv("PORT", 9001)))
    ap.add_argument("--reload", action="store_true", help="uvicorn --reload (desarrollo)")
    return ap


def main(argv: list[str] | None = None) -> None:
    ap = build_argparser()
    args = ap.parse_args(argv)

    cfg = ReplayConfig(
        db_path=args.duckdb_path,
        symbol=args.symbol,
        interval=args.interval,
        start_ts=_to_ms(args.start),
        end_ts=_to_ms(args.end),
        speed_bars_per_sec=args.speed,
        maker_bps=args.maker_bps,
        taker_bps=args.taker_bps,
        slippage_bps=args.slippage_bps,
        starting_balance=args.starting_balance,
    )

    app = create_app(cfg)

    uvicorn.run(app, host=args.host, port=args.port, reload=args.reload)


if __name__ == "__main__":
    main()
