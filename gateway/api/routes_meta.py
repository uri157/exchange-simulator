"""
Rutas de metadatos del exchange (subset Binance UM-Futures):
- GET /fapi/v1/exchangeInfo
"""
from __future__ import annotations

from typing import List

from fastapi import APIRouter, Depends, Request

from .deps import get_sim
from ..core.models import now_ms

router = APIRouter()


@router.get("/fapi/v1/exchangeInfo")
def exchange_info(req: Request, sim = Depends(get_sim)):
    # Descubrir símbolos disponibles desde las tablas OHLC
    rows = sim.con.sql(
        """
        SELECT DISTINCT symbol FROM ohlc
        UNION
        SELECT DISTINCT symbol FROM ohlc_1m
        """
    ).fetchall()
    symbols: List[str] = [r[0] for r in rows] or [sim.symbol]

    # Defaults simples; se pueden ajustar por env o por símbolo si querés granularidad
    import os
    tick = os.getenv("TICK_SIZE_DEFAULT", "0.1")          # precio mínimo
    step = os.getenv("STEP_SIZE_DEFAULT", "0.001")         # tamaño mínimo de lote
    min_qty = os.getenv("MIN_QTY_DEFAULT", "0.0001")
    max_qty = os.getenv("MAX_QTY_DEFAULT", "100000")
    price_prec = int(os.getenv("PRICE_PRECISION_DEFAULT", "8"))
    qty_prec = int(os.getenv("QUANTITY_PRECISION_DEFAULT", "8"))

    out = []
    for s in symbols:
        base = s[:-4] if s.endswith("USDT") and len(s) > 4 else s
        out.append({
            "symbol": s,
            "status": "TRADING",
            "contractType": "PERPETUAL",
            "baseAsset": base,
            "quoteAsset": "USDT",
            "pricePrecision": price_prec,
            "quantityPrecision": qty_prec,
            "filters": [
                {"filterType": "PRICE_FILTER", "tickSize": str(tick)},
                {"filterType": "LOT_SIZE", "stepSize": str(step), "minQty": str(min_qty), "maxQty": str(max_qty)},
            ],
        })

    return {"timezone": "UTC", "serverTime": now_ms(), "symbols": out}
