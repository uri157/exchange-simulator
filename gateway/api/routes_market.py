"""
Rutas de mercado (lectura de datos):
- GET /fapi/v1/klines           → array estilo Binance
- GET /fapi/v1/fundingRate      → historial de funding
- GET /fapi/v1/premiumIndex     → markPrice + lastFundingRate
- GET /fapi/v1/time             → serverTime
"""
from __future__ import annotations

from typing import Any, List, Optional

from fastapi import APIRouter, Depends, Query, Request

from .deps import get_sim
from ..core.models import table_for_interval, now_ms

router = APIRouter()


@router.get("/fapi/v1/time")
def server_time():
    return {"serverTime": now_ms()}


@router.get("/fapi/v1/klines")
def get_klines(
    symbol: str = Query(...),
    interval: str = Query(...),
    startTime: Optional[int] = Query(None),
    endTime: Optional[int] = Query(None),
    limit: Optional[int] = Query(None),
    req: Request = None,
    sim = Depends(get_sim),
):
    symbol_u = symbol.upper()
    tbl = table_for_interval(interval)
    sql = f"SELECT ts, open, high, low, close, volume, close_ts FROM {tbl} WHERE symbol = ?"
    params: List[Any] = [symbol_u]
    if startTime is not None:
        sql += " AND ts >= ?"; params.append(int(startTime))
    if endTime is not None:
        sql += " AND ts <= ?"; params.append(int(endTime))
    sql += " ORDER BY ts ASC"
    if limit is not None:
        sql += " LIMIT ?"; params.append(int(limit))
    rows = sim.con.sql(sql, params=params).fetchall()
    return [[int(a), float(b), float(c), float(d), float(e), float(f), int(g)] for a, b, c, d, e, f, g in rows]


@router.get("/fapi/v1/fundingRate")
def get_funding_rate(
    symbol: str,
    startTime: Optional[int] = None,
    endTime: Optional[int] = None,
    limit: Optional[int] = None,
    sim = Depends(get_sim),
):
    sql = "SELECT funding_time, funding_rate FROM funding WHERE symbol = ?"
    params: List[Any] = [symbol.upper()]
    if startTime is not None:
        sql += " AND funding_time >= ?"; params.append(int(startTime))
    if endTime is not None:
        sql += " AND funding_time <= ?"; params.append(int(endTime))
    sql += " ORDER BY funding_time ASC"
    if limit is not None:
        sql += " LIMIT ?"; params.append(int(limit))
    rows = sim.con.sql(sql, params=params).fetchall()
    return [{"symbol": symbol.upper(), "fundingTime": int(t), "fundingRate": float(r)} for (t, r) in rows]


@router.get("/fapi/v1/premiumIndex")
def premium_index(symbol: str, sim = Depends(get_sim)):
    px = sim.cur_price if sim.cur_price > 0 else (
        sim.replayer._bars[0][1] if sim.replayer._bars else 0.0
    )
    row = sim.con.sql(
        """
        SELECT funding_rate FROM funding
        WHERE symbol = ? AND funding_time <= ?
        ORDER BY funding_time DESC LIMIT 1
        """,
        params=[symbol.upper(), now_ms()],
    ).fetchone()
    fr = float(row[0]) if row else 0.0
    return {"symbol": symbol.upper(), "markPrice": f"{px:.8f}", "lastFundingRate": f"{fr:.8f}"}
