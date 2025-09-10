"""
Rutas de órdenes / libro simulado:
- POST   /fapi/v1/order              → crea LIMIT/MARKET (acepta quantity u origQty)
- DELETE /fapi/v1/order              → cancela por orderId
- DELETE /fapi/v1/allOpenOrders      → cancela todas por símbolo
- GET    /fapi/v1/openOrders         → lista órdenes vivas
- GET    /fapi/v1/ticker/bookTicker  → bid/ask sintético
"""
from __future__ import annotations

from typing import Any, List, Optional

from fastapi import APIRouter, Body, Depends, Query
from fastapi.responses import JSONResponse
from pydantic import BaseModel

from .deps import get_sim
from ..core.models import now_ms

router = APIRouter()


class OrderBody(BaseModel):
    symbol: str
    side: str
    type: str
    quantity: Optional[float] = None   # lo que normalmente envía binance-connector
    origQty: Optional[float] = None    # compat extra
    price: Optional[float] = None
    timeInForce: Optional[str] = "GTC"
    newClientOrderId: Optional[str] = None
    reduceOnly: Optional[bool] = False
    stopPrice: Optional[float] = None


@router.post("/fapi/v1/order")
def post_order(ob: OrderBody = Body(...), sim = Depends(get_sim)):
    symbol = ob.symbol.upper()
    side = ob.side.upper()
    type_ = ob.type.upper()
    tif = (ob.timeInForce or "GTC").upper()

    # cantidad: quantity u origQty
    qty_val = ob.quantity if ob.quantity is not None else ob.origQty
    if qty_val is None:
        return JSONResponse({"code": -1013, "msg": "Invalid quantity"}, status_code=400)
    qty = float(qty_val)

    if type_ not in ("MARKET", "LIMIT"):
        return JSONResponse({"code": -1116, "msg": "Unsupported order type"}, status_code=400)
    if type_ == "LIMIT":
        if ob.price is None or float(ob.price) <= 0:
            return JSONResponse({"code": -1013, "msg": "Invalid price for LIMIT"}, status_code=400)
        price = float(ob.price)
    else:
        price = None

    if qty <= 0:
        return JSONResponse({"code": -1013, "msg": "Invalid quantity"}, status_code=400)

    cur_px = sim.cur_price if sim.cur_price > 0 else (
        sim.replayer._bars[0][1] if sim.replayer._bars else 0.0
    )

    od = sim.exec.place_order(
        symbol=symbol,
        side=side,
        type_=type_,
        qty=qty,
        price=price,
        tif=tif,
        cur_price=cur_px,
        client_id=ob.newClientOrderId,
        reduce_only=bool(ob.reduceOnly),
        stop_price=float(ob.stopPrice) if ob.stopPrice is not None else None,
    )

    resp = {
        "symbol": symbol,
        "orderId": od.order_id,
        "clientOrderId": od.client_order_id,
        "transactTime": now_ms(),
        "price": f"{od.price or 0:.8f}",
        "origQty": f"{od.qty:.8f}",
        "executedQty": f"{od.executed_qty:.8f}",
        "status": od.status,
        "timeInForce": tif,
        "type": type_,
        "side": side,
    }
    if od.fills:
        resp["fills"] = od.fills
    return resp


@router.delete("/fapi/v1/order")
def delete_order(symbol: str = Query(...), orderId: int = Query(...), sim = Depends(get_sim)):
    ok = sim.exec.cancel(orderId)
    if not ok:
        return JSONResponse({"code": -2011, "msg": "Unknown order sent."}, status_code=400)
    return {"symbol": symbol.upper(), "orderId": orderId, "status": "CANCELED"}


@router.delete("/fapi/v1/allOpenOrders")
def cancel_all_open(symbol: str = Query(...), sim = Depends(get_sim)):
    to_del = [oid for oid, o in list(sim.exec.open_orders.items()) if o.symbol == symbol.upper()]
    for oid in to_del:
        sim.exec.cancel(oid)
    return []


@router.get("/fapi/v1/openOrders")
def open_orders(symbol: Optional[str] = None, sim = Depends(get_sim)):
    lst: List[dict[str, Any]] = []
    for od in sim.exec.open_orders.values():
        if symbol and od.symbol != symbol.upper():
            continue
        lst.append({
            "symbol": od.symbol,
            "orderId": od.order_id,
            "clientOrderId": od.client_order_id,
            "price": f"{(od.price or 0):.8f}",
            "origQty": f"{od.qty:.8f}",
            "executedQty": f"{od.executed_qty:.8f}",
            "status": od.status,
            "timeInForce": od.tif,
            "type": od.type,
            "side": od.side,
            "updateTime": od.ts,
        })
    return lst


@router.get("/fapi/v1/ticker/bookTicker")
def book_ticker(symbol: str = Query(...), sim = Depends(get_sim)):
    px = sim.cur_price if sim.cur_price > 0 else (
        sim.replayer._bars[0][1] if sim.replayer._bars else 0.0
    )
    bid = float(px) * (1 - 0.0002)
    ask = float(px) * (1 + 0.0002)
    return {
        "symbol": symbol.upper(),
        "bidPrice": f"{bid:.8f}", "bidQty": "1.00000000",
        "askPrice": f"{ask:.8f}", "askQty": "1.00000000",
    }
