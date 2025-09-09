# backtests/bt_runner.py
from __future__ import annotations
"""
Backtest Runner (plugin-friendly) con soporte 'duckdb'.

Ejemplos:
  # Datos desde DuckDB
  python3 -m backtests.bt_runner \
    --symbol BTCUSDT --interval 1h \
    --start 2024-07-01 --end 2024-07-05 \
    --data-source duckdb --duckdb-path data/duckdb/exsim.duckdb \
    --fill-model ohlc_up

  # Con estrategia SMA de demo
  python3 -m backtests.bt_runner \
    --symbol BTCUSDT --interval 1h \
    --start 2024-07-01 --end 2024-07-05 \
    --data-source duckdb --duckdb-path data/duckdb/exsim.duckdb \
    --fill-model ohlc_up \
    --strategy backtests.strategies.sma:SMA \
    --strategy-params '{"fast":5,"slow":20,"qty":0.002}'
"""

import argparse
import importlib
import json
import math
import os
import uuid
from datetime import datetime, timezone
from typing import Iterable, List, Tuple, Optional, Dict, Any

from data import binance_api, binance_files
from sim.exchange_sim import SimExchange
from sim.fill_models import OHLCPathFill, RandomOHLC, BookTickerFill
from sim.models import Bar


def _parse_date_ms(d: str) -> int:
    """
    Acepta 'YYYY-MM-DD' o ISO 'YYYY-MM-DDTHH:MM:SS' (con o sin 'Z').
    Devuelve epoch **en milisegundos** (UTC).
    """
    d = d.strip().replace("Z", "")
    try:
        dt = datetime.fromisoformat(d)
    except Exception:
        dt = datetime.strptime(d, "%Y-%m-%d")
    if dt.tzinfo is None:
        dt = dt.replace(tzinfo=timezone.utc)
    return int(dt.timestamp() * 1000)


def _build_fill_model(name: str, seed: int, slippage_bps: float):
    name = name.lower()
    if name == "ohlc_up":
        return OHLCPathFill(up_first=True, slippage_bps=slippage_bps)
    if name == "ohlc_down":
        return OHLCPathFill(up_first=False, slippage_bps=slippage_bps)
    if name == "random":
        return RandomOHLC(seed=seed, slippage_bps=slippage_bps)
    if name == "book":
        return BookTickerFill()
    raise ValueError(f"Unknown fill model: {name}")


def _load_strategy(strategy_path: Optional[str]):
    """
    Carga dinámica de estrategia: 'module.submodule:ClassName'
    Devuelve la clase (no instancia) o None si no se especifica.
    """
    if not strategy_path:
        return None
    if ":" not in strategy_path:
        raise ValueError("Invalid --strategy. Use 'module.submodule:ClassName'")
    mod_name, cls_name = strategy_path.split(":", 1)
    mod = importlib.import_module(mod_name)
    return getattr(mod, cls_name)


def _write_csv(path: str, header: str, rows: Iterable[str]) -> None:
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    with open(path, "w", newline="") as f:
        if header:
            f.write(header.rstrip() + "\n")
        for r in rows:
            f.write(r.rstrip() + "\n")


def _save_run_to_duckdb(
    duckdb_path: str,
    summary: Dict[str, Any],
    trades: List[Dict[str, Any]],
    equity_log: List[Tuple[int, float]],
    existing_con: Optional[object] = None,
) -> Optional[str]:
    """
    Guarda la run en DuckDB. Si se pasa existing_con, la reutiliza para evitar
    conflictos de configuración. Devuelve run_id o None si no se guardó.
    """
    try:
        import duckdb  # type: ignore
    except Exception:
        print("[WARN] duckdb no está instalado; no guardo la run en DB.")
        return None

    created_here = False
    con = None
    try:
        if existing_con is not None:
            con = existing_con
        else:
            con = duckdb.connect(duckdb_path)
            created_here = True

        run_id = str(uuid.uuid4())

        # Insert en tabla runs
        con.execute(
            """
            INSERT INTO runs(run_id, strategy, params_json, feature_set_id, code_hash)
            VALUES (?, ?, ?, NULL, NULL)
            """,
            [
                run_id,
                summary.get("strategy"),
                json.dumps(summary.get("strategy_params", {})),
            ],
        )

        # Insert trades_fills
        if trades:
            rows = [
                (
                    run_id,
                    i,
                    int(t.get("timestamp") or 0),
                    str(t.get("symbol", "")),
                    str(t.get("side", "")),
                    float(t.get("price", 0.0)),
                    float(t.get("quantity", 0.0)),
                    float(t.get("realized_pnl", 0.0)),
                    float(t.get("fee", 0.0)),
                    bool(t.get("is_maker", False)),
                )
                for i, t in enumerate(trades)
            ]
            con.executemany(
                """
                INSERT INTO trades_fills
                    (run_id, seq, ts, symbol, side, price, qty, realized_pnl, fee, is_maker)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                rows,
            )

        # Insert equity_curve
        if equity_log:
            rows = [(run_id, int(ts), float(eq)) for ts, eq in equity_log]
            con.executemany(
                "INSERT INTO equity_curve(run_id, ts, equity) VALUES (?, ?, ?)",
                rows,
            )

        if created_here:
            con.commit()

        print(f"[OK] Guardado run_id={run_id} en {duckdb_path}")
        return run_id

    except Exception as e:
        # Mensaje claro para el caso típico de configuración distinta (read_only, etc.)
        msg = str(e)
        if "different configuration" in msg:
            print("[WARN] No se pudo abrir otra conexión a DuckDB con distinta configuración.")
            print("       Se debe reutilizar la misma conexión o abrir ambas con la misma config.")
            print("       Verificá que DuckDBSource no esté en read_only=True si querés escribir.")
        elif "read-only" in msg or "read only" in msg:
            print("[WARN] La conexión actual a DuckDB es de solo lectura. Abrila sin read_only para poder escribir.")
        else:
            print(f"[WARN] Error guardando en DuckDB: {e}")
        return None
    finally:
        if created_here and con is not None:
            try:
                con.close()
            except Exception:
                pass


def run_backtest(
    symbol: str,
    interval: str,
    start: str,
    end: str,
    data_source: str = "api",                 # api | files | duckdb
    duckdb_path: str = "data/duckdb/exsim.duckdb",
    fill_model_name: str = "ohlc_up",
    seed: int = 42,
    maker_bps: float = 2.0,
    taker_bps: float = 4.0,
    slippage_bps: float = 0.0,
    starting_balance: float = 100_000.0,
    strategy_path: Optional[str] = None,
    strategy_params: Optional[Dict[str, Any]] = None,
) -> None:
    """
    Ejecuta un backtest por lotes, guarda CSVs + summary.json en ./reports
    y si el origen es 'duckdb' guarda la run en la base automáticamente.
    """
    start_ts = _parse_date_ms(start)
    end_ts = _parse_date_ms(end)

    # --- Data ---
    ds_obj = None
    if data_source == "files":
        ds_obj = binance_files.BinanceFileData()
        klines = ds_obj.get_klines(symbol, interval, startTime=start_ts, endTime=end_ts)
        funding_data = ds_obj.get_funding_rates(symbol, startTime=start_ts, endTime=end_ts)
    elif data_source == "duckdb":
        from data.duckdb_source import DuckDBSource  # type: ignore
        ds_obj = DuckDBSource(duckdb_path)
        klines = ds_obj.get_klines(symbol, interval, startTime=start_ts, endTime=end_ts)
        funding_data = ds_obj.get_funding_rates(symbol, startTime=start_ts, endTime=end_ts)
    else:
        klines = binance_api.get_klines(symbol, interval, startTime=start_ts, endTime=end_ts)
        funding_data = binance_api.get_funding_rates(symbol, startTime=start_ts, endTime=end_ts)

    # --- Fill model + simulador ---
    fill_model = _build_fill_model(fill_model_name, seed=seed, slippage_bps=slippage_bps)
    sim = SimExchange(
        starting_balance=starting_balance,
        maker_fee_bps=maker_bps,
        taker_fee_bps=taker_bps,
        fill_model=fill_model,
        hedge_mode=False,
        data_source=ds_obj,  # por si el fill model lo necesita
    )

    # --- Estrategia (opcional) ---
    StrategyCls = _load_strategy(strategy_path)
    strategy = StrategyCls(sim, symbol, interval, **(strategy_params or {})) if StrategyCls else None
    if strategy:
        strategy.on_start()

    equity_log: List[Tuple[int, float]] = []
    # Funding entries deben venir en orden por fundingTime
    funding_events = sorted(funding_data or [], key=lambda x: int(x.get("fundingTime", 0)))
    funding_idx = 0

    # --- Loop principal sobre barras ---
    for rec in klines:
        # Binance kline: [openTime, open, high, low, close, volume, closeTime, ...]
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

        # hint de precio actual para MARKET inmediato
        sim.last_price[symbol] = bar.open

        # Llamar estrategia (si existe)
        if strategy:
            strategy.on_bar(bar)

        # acumular funding que ocurra hasta el close de este bar
        cum_rate = 0.0
        while funding_idx < len(funding_events) and int(funding_events[funding_idx]["fundingTime"]) <= bar.close_time:
            fr = float(funding_events[funding_idx]["fundingRate"])
            cum_rate += fr
            funding_idx += 1

        sim.process_bar(bar, funding_rate=(cum_rate if abs(cum_rate) > 0 else None))
        equity_log.append((bar.close_time, float(sim.get_equity())))

    if strategy:
        strategy.on_finish()

    # --- Outputs ---
    trades = sim.trade_log  # list[dict]

    # trades.csv
    _write_csv(
        "reports/trades.csv",
        "timestamp,symbol,side,price,quantity,realized_pnl,fee,is_maker",
        (
            f"{(t.get('timestamp') or 0)},{t['symbol']},{t['side']},{t['price']},{t['quantity']},"
            f"{t.get('realized_pnl', 0.0)},{t.get('fee', 0.0)},{t.get('is_maker', False)}"
            for t in trades
        ),
    )

    # equity.csv
    _write_csv(
        "reports/equity.csv",
        "timestamp,equity",
        (f"{ts},{eq}" for ts, eq in equity_log),
    )

    # --- Stats básicos sobre trades cerrados (reconstrucción) ---
    closed_trade_pnls: List[float] = []
    wins = losses = 0
    pos_qty = 0.0
    trade_active = False
    current_trade_pnl = 0.0

    for tr in trades:
        side = tr["side"]
        qty = float(tr["quantity"])
        pnl = float(tr.get("realized_pnl", 0.0))

        prev_sign = math.copysign(1.0, pos_qty) if pos_qty != 0 else 0.0
        pos_qty += qty if side == "BUY" else -qty
        new_sign = math.copysign(1.0, pos_qty) if pos_qty != 0 else 0.0

        if not trade_active and pos_qty != 0:
            trade_active = True
            current_trade_pnl = 0.0

        if trade_active:
            current_trade_pnl += pnl

        # cerrado (flat) o flip de signo (close + reverse)
        if trade_active and pos_qty == 0:
            trade_active = False
            closed_trade_pnls.append(current_trade_pnl)
            current_trade_pnl = 0.0
        elif trade_active and prev_sign != 0 and new_sign != 0 and prev_sign != new_sign:
            closed_trade_pnls.append(current_trade_pnl)
            current_trade_pnl = 0.0
            trade_active = True

    for pnl in closed_trade_pnls:
        if pnl > 1e-9:
            wins += 1
        elif pnl < -1e-9:
            losses += 1

    num_trades = len(closed_trade_pnls)
    win_rate = (wins / num_trades * 100.0) if num_trades > 0 else 0.0
    gross_profit = sum(p for p in closed_trade_pnls if p > 0)
    gross_loss = sum(p for p in closed_trade_pnls if p < 0)
    profit_factor = (gross_profit / abs(gross_loss)) if gross_loss != 0 else float("inf")

    # daily returns (UTC)
    daily_equity: Dict[datetime.date, float] = {}
    for ts, eq in equity_log:
        daily_equity[datetime.fromtimestamp(ts / 1000, tz=timezone.utc).date()] = float(eq)

    daily_eq_list = [daily_equity[d] for d in sorted(daily_equity.keys())]
    daily_returns: List[float] = []
    for i in range(1, len(daily_eq_list)):
        daily_returns.append(daily_eq_list[i] / daily_eq_list[i - 1] - 1.0)

    sharpe: Optional[float] = None
    sortino: Optional[float] = None
    if len(daily_returns) >= 2:
        import statistics
        mean_ret = statistics.mean(daily_returns)
        std_ret = statistics.pstdev(daily_returns)
        neg = [r for r in daily_returns if r < 0]
        # semidesviación poblacional (estilo Sortino) usando N total
        down_var = sum(x * x for x in neg) / len(daily_returns) if daily_returns else 0.0
        down_std = math.sqrt(down_var) if down_var > 0 else 0.0

        if std_ret > 0:
            sharpe = (mean_ret / std_ret) * math.sqrt(365)
        if down_std > 0:
            sortino = (mean_ret / down_std) * math.sqrt(365)

    # max drawdown
    peak = equity_log[0][1] if equity_log else 0.0
    max_dd = 0.0
    for _, eq in equity_log:
        if eq > peak:
            peak = eq
        if peak > 0:
            dd = (peak - eq) / peak
            if dd > max_dd:
                max_dd = dd

    # comp returns
    if end_ts <= start_ts or not equity_log:
        avg_weekly_ret = 0.0
        avg_monthly_ret = 0.0
    else:
        total_days = (end_ts - start_ts) / 86_400_000.0
        total_return = equity_log[-1][1] / equity_log[0][1] - 1.0
        daily_ret = (1.0 + total_return) ** (1.0 / total_days) - 1.0 if total_days > 0 else 0.0
        avg_weekly_ret = ((1.0 + daily_ret) ** 7 - 1.0) * 100.0
        avg_monthly_ret = ((1.0 + daily_ret) ** 30 - 1.0) * 100.0

    summary = {
        "trades": num_trades,
        "win_rate": win_rate,
        "profit_factor": profit_factor,
        "sharpe": sharpe,
        "sortino": sortino,
        "max_drawdown": max_dd * 100.0,
        "average_weekly_return": avg_weekly_ret,
        "average_monthly_return": avg_monthly_ret,
        "starting_balance": starting_balance,
        "ending_equity": equity_log[-1][1] if equity_log else starting_balance,
        "symbol": symbol,
        "interval": interval,
        "start": start,
        "end": end,
        "data_source": data_source,
        "duckdb_path": duckdb_path if data_source == "duckdb" else None,
        "fill_model": fill_model_name,
        "maker_bps": maker_bps,
        "taker_bps": taker_bps,
        "slippage_bps": slippage_bps,
        "seed": seed,
        "strategy": strategy_path,
        "strategy_params": strategy_params or {},
    }

    os.makedirs("reports", exist_ok=True)
    with open("reports/summary.json", "w") as f:
        json.dump(summary, f, indent=4)

    # Guardado automático en DuckDB (si corresponde)
    run_id = None
    if data_source == "duckdb":
        existing_con = getattr(ds_obj, "con", None)  # reutilizamos conexión si está expuesta
        run_id = _save_run_to_duckdb(
            duckdb_path=duckdb_path,
            summary=summary,
            trades=trades,
            equity_log=equity_log,
            existing_con=existing_con,
        )

    # Console summary
    print(f"Trades: {num_trades}, Win rate: {win_rate:.2f}%, Profit Factor: {profit_factor:.2f}")
    if sharpe is not None or sortino is not None:
        sharpe_s = f"{sharpe:.2f}" if sharpe is not None else "n/a"
        sortino_s = f"{sortino:.2f}" if sortino is not None else "n/a"
        print(f"Sharpe: {sharpe_s}, Sortino: {sortino_s}")
    print(
        f"Max Drawdown: {max_dd*100:.2f}%, "
        f"Avg Weekly Return: {avg_weekly_ret:.2f}%, Avg Monthly Return: {avg_monthly_ret:.2f}%"
    )
    if run_id:
        print(f"Run ID: {run_id}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Backtest Runner")
    parser.add_argument("--symbol", required=True, help="Symbol, e.g. BTCUSDT")
    parser.add_argument("--interval", required=True, help="Interval, e.g. 1h")
    parser.add_argument("--start", required=True, help="Start date (YYYY-MM-DD or ISO datetime)")
    parser.add_argument("--end", required=True, help="End date (YYYY-MM-DD or ISO datetime)")
    parser.add_argument(
        "--data-source", choices=["api", "files", "duckdb"], default="api",
        help="Origen de datos"
    )
    parser.add_argument(
        "--duckdb-path", type=str, default="data/duckdb/exsim.duckdb",
        help="Ruta a la DB DuckDB (si --data-source duckdb)"
    )
    parser.add_argument(
        "--fill-model",
        choices=["ohlc_up", "ohlc_down", "random", "book"],
        default="ohlc_up",
        help="Fill model",
    )
    parser.add_argument("--seed", type=int, default=42, help="Random seed (for random fill model)")
    parser.add_argument("--maker-bps", type=float, default=2.0, help="Maker fee (bps, e.g. 2.0 => 0.02%)")
    parser.add_argument("--taker-bps", type=float, default=4.0, help="Taker fee (bps, e.g. 4.0 => 0.04%)")
    parser.add_argument("--slippage-bps", type=float, default=0.0, help="Slippage (bps)")
    parser.add_argument("--starting-balance", type=float, default=100_000.0, help="Starting USDT balance")

    # Plugin de estrategia
    parser.add_argument("--strategy", type=str, default=None, help="Ruta 'modulo.submodulo:Clase'. Ej: backtests.strategies.sma:SMA")
    parser.add_argument("--strategy-params", type=str, default=None, help="JSON con parámetros para la estrategia")

    args = parser.parse_args()
    params = json.loads(args.strategy_params) if args.strategy_params else None

    run_backtest(
        symbol=args.symbol,
        interval=args.interval,
        start=args.start,
        end=args.end,
        data_source=args.data_source,
        duckdb_path=args.duckdb_path,
        fill_model_name=args.fill_model,
        seed=args.seed,
        maker_bps=args.maker_bps,
        taker_bps=args.taker_bps,
        slippage_bps=args.slippage_bps,
        starting_balance=args.starting_balance,
        strategy_path=args.strategy,
        strategy_params=params,
    )
