"""
Rutas de cuenta / riesgo (subset Binance UM-Futures):
- GET  /fapi/v2/balance
- GET  /fapi/v2/positionRisk (alias v1 también)
- POST /fapi/v1/leverage
- POST /fapi/v1/marginType
- POST /fapi/v1/positionSide/dual
- POST /fapi/v1/listenKey
"""
from __future__ import annotations

from typing import Optional

from fastapi import APIRouter, Depends, Query

from .deps import get_sim
from ..core.models import now_ms

router = APIRouter()


@router.get("/fapi/v2/balance")
def balance(sim = Depends(get_sim)):
    px = sim.cur_price if sim.cur_price > 0 else (
        sim.replayer._bars[0][1] if sim.replayer._bars else 0.0
    )
    equity = sim.account.mark_to_market(px)
    return [{
        "accountAlias": "SIM",
        "asset": "USDT",
        "balance": f"{equity:.8f}",
        "crossWalletBalance": f"{equity:.8f}",
        "availableBalance": f"{sim.account.cash:.8f}",
        "maxWithdrawAmount": f"{sim.account.cash:.8f}",
        "updateTime": now_ms()
    }]


@router.get("/fapi/v1/positionRisk")
@router.get("/fapi/v2/positionRisk")
def position_risk(symbol: Optional[str] = None, sim = Depends(get_sim)):
    px = sim.cur_price if sim.cur_price > 0 else (
        sim.replayer._bars[0][1] if sim.replayer._bars else 0.0
    )
    pos = sim.account.position
    upnl = (px - pos.avg_price) * pos.qty if pos.qty != 0 else 0.0
    return [{
        "symbol": sim.symbol,
        "positionAmt": f"{pos.qty:.8f}",
        "entryPrice": f"{pos.avg_price:.8f}",
        "unRealizedProfit": f"{upnl:.8f}",
        "markPrice": f"{px:.8f}",
        "leverage": str(sim.leverage),
        "marginType": sim.margin_type,
        "updateTime": now_ms(),
        "positionSide": "BOTH" if not sim.dual_side else "BOTH",  # compat: usamos BOTH en este subset
    }]


@router.post("/fapi/v1/leverage")
def set_leverage(symbol: str = Query(...), leverage: int = Query(...), sim = Depends(get_sim)):
    sim.leverage = int(leverage)
    return {"leverage": sim.leverage, "symbol": symbol, "maxNotionalValue": "0"}


@router.post("/fapi/v1/marginType")
def set_margin_type(symbol: str = Query(...), marginType: str = Query(...), sim = Depends(get_sim)):
    sim.margin_type = marginType.upper()
    return {"symbol": symbol, "marginType": sim.margin_type}


@router.post("/fapi/v1/positionSide/dual")
def set_dual(dualSidePosition: bool = Query(...), sim = Depends(get_sim)):
    sim.dual_side = bool(dualSidePosition)
    return {"dualSidePosition": sim.dual_side}


@router.post("/fapi/v1/listenKey")
def listen_key():
    import uuid
    return {"listenKey": f"sim-{uuid.uuid4().hex[:16]}"}
