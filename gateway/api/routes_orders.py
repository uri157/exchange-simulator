"""
Rutas de órdenes / cuenta (subset Binance USDⓈ-M):
- POST   /fapi/v1/order             → MARKET/LIMIT/STOP_MARKET (form/json/query)
- DELETE /fapi/v1/order             → cancela por orderId
- DELETE /fapi/v1/allOpenOrders     → cancela todas por símbolo
- GET    /fapi/v1/openOrders        → órdenes vivas
- GET    /fapi/v3/ticker/bookTicker → bid/ask sintético
"""
from __future__ import annotations

from typing import Any, Dict, List, Optional

from fastapi import APIRouter, Depends, Request, Query
from fastapi.responses import JSONResponse

from .deps import get_sim
from ..core.models import now_ms

router = APIRouter()

# ---- ID auxiliar para STOP_MARKET placeholder
_AUX_ID = 10_000_000
def _next_aux_id() -> int:
    global _AUX_ID
    _AUX_ID += 1
    return _AUX_ID


def _to_bool(v: Any) -> bool:
    if isinstance(v, bool):
        return v
    if v is None:
        return False
    s = str(v).strip().lower()
    return s in ("1", "true", "yes", "y", "on")


async def _read_params(request: Request) -> Dict[str, Any]:
    """
    Acepta parámetros en:
      - JSON (application/json)
      - FORM (application/x-www-form-urlencoded, multipart/form-data)
      - QUERY (también firmado: timestamp, signature, etc.) → se ignoran extras desconocidos
    Funde QUERY primero y luego BODY para dar prioridad al body si hay colisión.
    """
    out: Dict[str, Any] = dict(request.query_params)

    ct = (request.headers.get("content-type") or "").split(";")[0].strip().lower()
    # JSON
    if ct == "application/json":
        try:
            body = await request.json()
            if isinstance(body, dict):
                out.update(body)
        except Exception:
            pass
    # FORM (urlencoded o multipart)
    elif ct in ("application/x-www-form-urlencoded", "multipart/form-data"):
        try:
            form = await request.form()
            # FormData -> dict toma el primer valor por clave (igual que binance-connector)
            out.update(dict(form))
        except Exception:
            pass
    else:
        # Algunos clientes envían el payload urlencoded con content-type vacío;
        # intentamos parsear como form si hay body.
        try:
            body_bytes = await request.body()
            if body_bytes:
                from urllib.parse import parse_qsl
                parsed = dict(parse_qsl(body_bytes.decode("utf-8", "ignore")))
                out.update(parsed)
        except Exception:
            pass

    return out


@router.post("/fapi/v1/order")
async def post_order(request: Request, sim=Depends(get_sim)):
    p = await _read_params(request)

    symbol = (p.get("symbol") or "").upper()
    side = (p.get("side") or "").upper()
    # admitir orderType además de type
    type_ = (p.get("type") or p.get("orderType") or "").upper()
    # alias: algunos SDKs envían "STOP" → lo tratamos como STOP_MARKET
    if type_ == "STOP":
        type_ = "STOP_MARKET"

    tif = (p.get("timeInForce") or "GTC").upper()
    client_id = p.get("newClientOrderId") or p.get("clientOrderId") or None

    # cantidad: Binance acepta 'quantity' y también 'origQty' (según SDK/endpoint).
    qty_raw = p.get("quantity") or p.get("origQty") or p.get("qty")
    price_raw = p.get("price")
    stop_price_raw = p.get("stopPrice")
    reduce_only = _to_bool(p.get("reduceOnly"))

    if not symbol:
        return JSONResponse(
            {"code": -1102, "msg": "Mandatory parameter 'symbol' was not sent, was empty/null, or malformed."},
            status_code=400,
        )
    if not side:
        return JSONResponse(
            {"code": -1102, "msg": "Mandatory parameter 'side' was not sent, was empty/null, or malformed."},
            status_code=400,
        )
    if not type_:
        return JSONResponse(
            {"code": -1102, "msg": "Mandatory parameter 'type' was not sent, was empty/null, or malformed."},
            status_code=400,
        )
    if qty_raw is None:
        return JSONResponse(
            {"code": -1102, "msg": "Mandatory parameter 'quantity' was not sent, was empty/null, or malformed."},
            status_code=400,
        )

    # parseos numéricos
    try:
        qty = float(qty_raw)
    except Exception:
        return JSONResponse({"code": -1013, "msg": "Invalid quantity"}, status_code=400)

    price: Optional[float] = None
    if price_raw is not None and str(price_raw).strip() != "":
        try:
            price = float(price_raw)
        except Exception:
            return JSONResponse({"code": -1013, "msg": "Invalid price"}, status_code=400)

    stop_price: Optional[float] = None
    if stop_price_raw is not None and str(stop_price_raw).strip() != "":
        try:
            stop_price = float(stop_price_raw)
        except Exception:
            return JSONResponse({"code": -1013, "msg": "Invalid stopPrice"}, status_code=400)

    # ------ Tipos soportados ------
    SUPPORTED = ("MARKET", "LIMIT", "STOP_MARKET")
    if type_ not in SUPPORTED:
        return JSONResponse({"code": -1116, "msg": "Unsupported order type"}, status_code=400)

    # Precio actual (para MARKET / bookTicker / y validaciones)
    cur_px = sim.cur_price if sim.cur_price > 0 else (sim.replayer._bars[0][1] if sim.replayer._bars else 0.0)

    # ------ STOP_MARKET (placeholder) ------
    # Aceptamos el stop para que el bot no falle. Lo dejamos como NEW.
    # (Si querés trigger real por markPrice, se puede agregar luego en SimState.)
    if type_ == "STOP_MARKET":
        if stop_price is None or stop_price <= 0:
            return JSONResponse({"code": -1013, "msg": "Invalid stopPrice for STOP_MARKET"}, status_code=400)

        order_id = _next_aux_id()
        if not hasattr(sim, "_stop_placeholders"):
            sim._stop_placeholders = {}
        sim._stop_placeholders[order_id] = {
            "symbol": symbol, "side": side, "qty": qty, "stopPrice": stop_price,
            "timeInForce": tif, "clientOrderId": client_id,
            "reduceOnly": reduce_only, "created": now_ms(),
        }

        resp = {
            "symbol": symbol,
            "orderId": order_id,
            "clientOrderId": client_id or f"stop-{order_id}",
            "transactTime": now_ms(),
            "price": "0.00000000",
            "origQty": f"{qty:.8f}",
            "executedQty": "0.00000000",
            "status": "NEW",
            "timeInForce": tif,
            "type": "STOP_MARKET",
            "side": side,
            "stopPrice": f"{stop_price:.8f}",
        }
        return resp

    # ------ MARKET / LIMIT por el executor del simulador ------
    if type_ == "LIMIT" and (price is None or price <= 0):
        return JSONResponse({"code": -1013, "msg": "Invalid price for LIMIT"}, status_code=400)
    if qty <= 0:
        return JSONResponse({"code": -1013, "msg": "Invalid quantity"}, status_code=400)

    od = sim.exec.place_order(
        symbol=symbol, side=side, type_=type_, qty=qty,
        price=price, tif=tif, cur_price=cur_px
    )

    resp = {
        "symbol": symbol,
        "orderId": od.order_id,
        "clientOrderId": client_id or od.client_order_id,
        "transactTime": now_ms(),
        "price": f"{(od.price or 0):.8f}",
        "origQty": f"{od.qty:.8f}",
        "executedQty": f"{od.executed_qty:.8f}",
        "status": od.status,
        "timeInForce": od.tif,
        "type": type_,
        "side": side,
    }
    if od.fills:
        resp["fills"] = od.fills
    return resp


@router.delete("/fapi/v1/order")
async def delete_order(symbol: str = Query(...), orderId: int = Query(...), sim=Depends(get_sim)):
    # Si es un STOP_MARKET placeholder, “cancela” acá también.
    if hasattr(sim, "_stop_placeholders") and orderId in getattr(sim, "_stop_placeholders"):
        sim._stop_placeholders.pop(orderId, None)
        return {"symbol": symbol.upper(), "orderId": orderId, "status": "CANCELED"}

    ok = sim.exec.cancel(orderId)
    if not ok:
        return JSONResponse({"code": -2011, "msg": "Unknown order sent."}, status_code=400)
    return {"symbol": symbol.upper(), "orderId": orderId, "status": "CANCELED"}


@router.delete("/fapi/v1/allOpenOrders")
async def delete_all_open_orders(symbol: str = Query(...), sim=Depends(get_sim)):
    # Borra placeholders de STOP_MARKET
    if hasattr(sim, "_stop_placeholders"):
        for oid, data in list(sim._stop_placeholders.items()):
            if (data or {}).get("symbol") == symbol.upper():
                sim._stop_placeholders.pop(oid, None)

    to_cancel: List[int] = []
    for oid, od in list(sim.exec.open_orders.items()):
        if od.symbol == symbol.upper() and od.status == "NEW":
            to_cancel.append(oid)
    for oid in to_cancel:
        sim.exec.cancel(oid)
    return {"code": 200, "msg": "All open orders canceled."}


@router.get("/fapi/v1/openOrders")
async def open_orders(symbol: Optional[str] = None, sim=Depends(get_sim)):
    lst = []
    # executor (LIMIT/pendientes)
    for od in sim.exec.open_orders.values():
        if symbol and od.symbol != symbol.upper():
            continue
        lst.append({
            "symbol": od.symbol, "orderId": od.order_id, "clientOrderId": od.client_order_id,
            "price": f"{(od.price or 0):.8f}", "origQty": f"{od.qty:.8f}",
            "executedQty": f"{od.executed_qty:.8f}", "status": od.status,
            "timeInForce": od.tif, "type": od.type, "side": od.side,
            "updateTime": od.ts,
        })
    # placeholders STOP_MARKET
    if hasattr(sim, "_stop_placeholders"):
        for oid, data in getattr(sim, "_stop_placeholders").items():
            if symbol and (data or {}).get("symbol") != symbol.upper():
                continue
            lst.append({
                "symbol": (data or {}).get("symbol",""),
                "orderId": oid,
                "clientOrderId": (data or {}).get("clientOrderId",""),
                "price": "0.00000000",
                "origQty": f"{float((data or {}).get('qty',0.0)):.8f}",
                "executedQty": "0.00000000",
                "status": "NEW",
                "timeInForce": (data or {}).get("timeInForce","GTC"),
                "type": "STOP_MARKET",
                "side": (data or {}).get("side",""),
                "updateTime": (data or {}).get("created", now_ms()),
                "stopPrice": f"{float((data or {}).get('stopPrice',0.0)):.8f}",
            })
    return lst


@router.get("/fapi/v3/ticker/bookTicker")
async def book_ticker(symbol: str, sim=Depends(get_sim)):
    px = sim.cur_price if sim.cur_price > 0 else (sim.replayer._bars[0][1] if sim.replayer._bars else 0.0)
    bid = px * (1 - 0.0002)
    ask = px * (1 + 0.0002)
    return {
        "symbol": symbol.upper(),
        "bidPrice": f"{bid:.8f}", "bidQty": "1.00000000",
        "askPrice": f"{ask:.8f}", "askQty": "1.00000000",
    }
