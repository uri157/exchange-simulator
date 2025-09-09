from __future__ import annotations

import argparse
import time
from datetime import datetime
from typing import Optional, List

from sim.exchange_sim import SimExchange
from sim.fill_models import OHLCPathFill
from data import binance_api, binance_files
from sim.models import Bar


_INTERVAL_MS = {
    "1m": 60_000,
    "3m": 3 * 60_000,
    "5m": 5 * 60_000,
    "15m": 15 * 60_000,
    "30m": 30 * 60_000,
    "1h": 60 * 60_000,
    "2h": 2 * 60 * 60_000,
    "4h": 4 * 60 * 60_000,
    "6h": 6 * 60 * 60_000,
    "8h": 8 * 60 * 60_000,
    "12h": 12 * 60 * 60_000,
    "1d": 24 * 60 * 60_000,
}


def _parse_date_ms(d: str) -> int:
    """Accepts 'YYYY-MM-DD' or full ISO datetime. Returns epoch ms (UTC)."""
    try:
        dt = datetime.fromisoformat(d)
    except Exception:
        dt = datetime.strptime(d, "%Y-%m-%d")
    return int(dt.timestamp() * 1000)


def _interval_to_ms(interval: str) -> int:
    if interval in _INTERVAL_MS:
        return _INTERVAL_MS[interval]
    # fallback for e.g. "60" meaning minutes
    try:
        return int(interval) * 60_000
    except Exception:
        return 0


def run_replay(
    symbol: str,
    interval: str,
    start: str,
    end: str,
    data_source: str = "api",
    speed: float = 1.0,
    starting_balance: float = 100_000.0,
) -> None:
    """
    Simple time-accelerated replayer:
      - Streams historical bars to a SimExchange
      - Prints close price and equity after each bar
    """
    start_ts = _parse_date_ms(start)
    end_ts = _parse_date_ms(end)

    # Load historical bars
    if data_source == "files":
        loader = binance_files.BinanceFileData()
        klines = loader.get_klines(symbol, interval, startTime=start_ts, endTime=end_ts)
    else:
        klines = binance_api.get_klines(symbol, interval, startTime=start_ts, endTime=end_ts)

    # Sim exchange (deterministic OHLC path, no funding in replay for simplicity)
    sim = SimExchange(
        starting_balance=starting_balance,
        maker_fee_bps=0.2,
        taker_fee_bps=0.4,
        fill_model=OHLCPathFill(up_first=True),
        hedge_mode=False,
    )

    interval_ms = _interval_to_ms(interval)

    try:
        for rec in klines:
            if len(rec) < 7:
                continue

            open_time = int(rec[0])
            close_time = int(rec[6])

            bar = Bar(
                open_time=open_time,
                open=float(rec[1]),
                high=float(rec[2]),
                low=float(rec[3]),
                close=float(rec[4]),
                volume=float(rec[5]),
                close_time=close_time,
            )

            # No funding application in replay mode (keep it simple)
            sim.process_bar(bar)

            dt = datetime.utcfromtimestamp(bar.close_time / 1000)
            print(f"{dt} - {symbol} close: {bar.close:.2f} | Equity: {sim.get_equity():.2f}", flush=True)

            if speed > 0 and interval_ms > 0:
                time.sleep((interval_ms / 1000.0) / speed)

    except KeyboardInterrupt:
        print("\nReplay interrupted by user.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Market Data Replayer")
    parser.add_argument("--symbol", required=True, help="e.g. BTCUSDT")
    parser.add_argument("--interval", required=True, help="e.g. 1m, 15m, 1h")
    parser.add_argument("--start", required=True, help="YYYY-MM-DD or ISO datetime")
    parser.add_argument("--end", required=True, help="YYYY-MM-DD or ISO datetime")
    parser.add_argument("--data-source", choices=["api", "files"], default="api")
    parser.add_argument(
        "--speed",
        type=float,
        default=1.0,
        help="Replay speed factor (e.g. 2.0 = twice as fast; 0 = no sleep)",
    )
    parser.add_argument(
        "--starting-balance",
        type=float,
        default=100_000.0,
        help="Starting balance in quote currency for the simulator",
    )
    args = parser.parse_args()

    run_replay(
        symbol=args.symbol,
        interval=args.interval,
        start=args.start,
        end=args.end,
        data_source=args.data_source,
        speed=args.speed,
        starting_balance=args.starting_balance,
    )
