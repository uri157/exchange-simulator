# routes_meta.py
from __future__ import annotations
import threading
from fastapi import APIRouter, Depends
from ..core.models import now_ms
from .deps import get_sim

router = APIRouter()
_duck_lock = threading.Lock()  # <<< NUEVO

@router.get("/fapi/v1/exchangeInfo")
def exchange_info(sim = Depends(get_sim)):
    symbol = sim.symbol.upper()  # p.ej. BTCUSDT
    base, quote = symbol[:-4], symbol[-4:]

    # defaults del sim por si la consulta a DuckDB falla
    tick_size = getattr(sim, "tick_size", 0.1)
    step_size = getattr(sim, "step_size", 0.0001)

    # Si tenés meta en DuckDB, leelo protegido por lock
    try:
        con = sim.duck  # ajustá al atributo real de tu estado
        with _duck_lock:
            # OJO: usar execute()+fetchone() dentro del try
            row = con.execute(
                """
                SELECT tick_size, step_size
                FROM meta_symbol
                WHERE symbol = ?
                """,
                [symbol],
            ).fetchone()
        if row and len(row) >= 2:
            tick_size = float(row[0])
            step_size = float(row[1])
    except Exception as e:
        # logueá y seguí con defaults
        if hasattr(sim, "logger"):
            sim.logger.warning("exchangeInfo meta fallback: %r", e)

    return {
        "timezone": "UTC",
        "serverTime": now_ms(),
        "symbols": [
            {
                "symbol": symbol,
                "pair": symbol,
                "status": "TRADING",
                "contractType": "PERPETUAL",
                "baseAsset": base,
                "quoteAsset": quote,
                "filters": [
                    {"filterType": "PRICE_FILTER", "tickSize": f"{tick_size:.8f}"},
                    {"filterType": "LOT_SIZE", "stepSize": f"{step_size:.8f}"},
                ],
            }
        ],
    }
