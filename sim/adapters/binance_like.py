from __future__ import annotations

from typing import Any, Dict, List, Optional, Union

from sim.exchange_sim import SimExchange
from sim.models import Order, Position


class BinanceLikeExchange:
    """
    Adapter que presenta `SimExchange` con una interfaz tipo Binance UM-Futures.

    Objetivo: que las estrategias que esperan un "cliente" estilo Binance
    (métodos y payloads) puedan trabajar contra el simulador sin cambiar su lógica.

    Notas:
    - Convierte objetos internos (Order, Position) a dicts Binance-like.
    - Acepta parámetros string (side/type/timeInForce) y deja que `SimExchange`
      los normalice/valide internamente.
    """

    def __init__(self, sim_exchange: SimExchange):
        self.sim = sim_exchange

    # ------------------------
    # Helpers de serialización
    # ------------------------
    @staticmethod
    def _order_to_binance_dict(o: Order) -> Dict[str, Any]:
        executed_qty = o.filled_quantity or 0.0
        avg_price = o.avg_fill_price or 0.0
        price = o.price or 0.0
        stop_price = o.stop_price or 0.0
        tif = o.time_in_force.name if getattr(o, "time_in_force", None) else "GTC"
        otype = o.type.name if getattr(o, "type", None) else str(getattr(o, "type", "MARKET"))
        side = o.side.value if hasattr(o.side, "value") else str(o.side)
        status = o.status.name if hasattr(o.status, "name") else str(o.status)

        return {
            "symbol": o.symbol,
            "orderId": o.order_id,
            "clientOrderId": o.client_order_id or "",
            "price": f"{price}",
            "origQty": f"{o.quantity}",
            "executedQty": f"{executed_qty}",
            "cumQty": f"{executed_qty}",
            "avgPrice": f"{avg_price}",
            "cumQuote": f"{avg_price * executed_qty}",
            "status": status,
            "timeInForce": tif,
            "type": otype,
            "origType": otype,
            "side": side,
            "reduceOnly": bool(o.reduce_only),
            "stopPrice": f"{stop_price}",
        }

    @staticmethod
    def _position_to_binance_dict(p: Union[Position, Dict[str, Any]], hedge_mode: bool) -> Dict[str, Any]:
        if isinstance(p, dict):
            # Ya es dict (aceptamos llaves internas o Binance-like)
            symbol = p.get("symbol") or p.get("Symbol") or ""
            qty = float(p.get("positionAmt", p.get("qty", 0.0)))
            entry = float(p.get("entryPrice", p.get("entry_price", 0.0)))
            upnl = float(p.get("unRealizedProfit", p.get("unrealized_pnl", 0.0)))
        else:
            symbol = p.symbol
            qty = float(getattr(p, "qty", 0.0))
            entry = float(getattr(p, "entry_price", 0.0))
            upnl = float(getattr(p, "unrealized_pnl", 0.0))

        # Binance-like:
        position_side = "BOTH"
        if hedge_mode:
            # Heurística simple: signo de qty para LONG/SHORT
            position_side = "LONG" if qty >= 0 else "SHORT"

        return {
            "symbol": symbol,
            "positionAmt": f"{qty}",
            "entryPrice": f"{entry}",
            "unRealizedProfit": f"{upnl}",
            "positionSide": position_side,
        }

    # -------------
    # Métodos públicos
    # -------------
    def new_order(
        self,
        symbol: str,
        side: str,
        type: str,
        quantity: float,
        price: Optional[float] = None,
        stopPrice: Optional[float] = None,
        timeInForce: str = "GTC",
        reduceOnly: bool = False,
        newClientOrderId: Optional[str] = None,
    ) -> Dict[str, Any]:
        """
        Crea una orden new-order estilo Binance y retorna un payload Binance-like.
        """
        order: Order = self.sim.new_order(
            symbol=symbol,
            side=side,
            type=type,
            quantity=quantity,
            price=price,
            stop_price=stopPrice,
            time_in_force=timeInForce,
            reduce_only=reduceOnly,
            client_order_id=newClientOrderId,
        )
        return self._order_to_binance_dict(order)

    def cancel_order(
        self,
        symbol: str,
        orderId: Optional[int] = None,
        origClientOrderId: Optional[str] = None,
    ) -> Dict[str, Any]:
        """
        Cancela una orden y retorna un payload Binance-like.
        """
        result = self.sim.cancel_order(symbol=symbol, order_id=orderId, client_order_id=origClientOrderId)

        # `result` puede ser:
        # - dict con {"status": "CANCELED", "orderId": ..., "clientOrderId": ...}
        # - Order ya actualizado
        # - bool/None en fallback
        if isinstance(result, Order):
            return {
                "status": "CANCELED" if str(getattr(result.status, "name", result.status)).upper() == "CANCELED" else str(result.status),
                "orderId": result.order_id,
                "clientOrderId": result.client_order_id or "",
                "symbol": result.symbol,
            }
        if isinstance(result, dict):
            status = result.get("status", "UNKNOWN")
            return {
                "status": status,
                "orderId": result.get("orderId"),
                "clientOrderId": result.get("clientOrderId", ""),
                "symbol": symbol,
            }
        if result:
            return {"status": "CANCELED", "orderId": orderId, "clientOrderId": origClientOrderId or "", "symbol": symbol}
        return {"status": "UNKNOWN", "orderId": orderId, "clientOrderId": origClientOrderId or "", "symbol": symbol}

    def get_open_orders(self, symbol: Optional[str] = None) -> List[Dict[str, Any]]:
        """
        Retorna órdenes abiertas en formato Binance-like.
        """
        orders = self.sim.get_open_orders(symbol)
        out: List[Dict[str, Any]] = []
        for o in orders:
            d = self._order_to_binance_dict(o)
            # Reducimos a campos más comunes de `openOrders`
            out.append(
                {
                    "symbol": d["symbol"],
                    "orderId": d["orderId"],
                    "clientOrderId": d["clientOrderId"],
                    "price": d["price"],
                    "origQty": d["origQty"],
                    "executedQty": d["executedQty"],
                    "status": d["status"],
                    "timeInForce": d["timeInForce"],
                    "type": d["type"],
                    "side": d["side"],
                    "reduceOnly": d["reduceOnly"],
                    "stopPrice": d["stopPrice"],
                }
            )
        return out

    def position_risk(self, symbol: Optional[str] = None) -> Union[Dict[str, Any], List[Dict[str, Any]]]:
        """
        Retorna posición(es) en formato Binance-like.
        """
        pos = self.sim.position_risk(symbol=symbol)
        if pos is None:
            return [] if symbol is None else {}

        if isinstance(pos, list):
            return [self._position_to_binance_dict(p, self.sim.hedge_mode) for p in pos]
        return self._position_to_binance_dict(pos, self.sim.hedge_mode)

    def account_info(self) -> Dict[str, Any]:
        """
        Retorna balances y equity en formato Binance-like (simplificado).
        """
        info = self.sim.account_info()
        balance = float(info.get("balance", 0.0))
        unreal = float(info.get("unrealized_profit", 0.0))
        margin_balance = balance + unreal
        return {
            "assets": [
                {
                    "asset": "USDT",
                    "walletBalance": f"{balance:.8f}",
                    "unrealizedProfit": f"{unreal:.8f}",
                    "marginBalance": f"{margin_balance:.8f}",
                    "availableBalance": f"{margin_balance:.8f}",
                }
            ],
            "feeTier": 0,
            "canTrade": True,
        }

    # -----------------
    # Passthrough de data
    # -----------------
    def get_klines(
        self,
        symbol: str,
        interval: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
        limit: Optional[int] = None,
    ):
        return self.sim.get_klines(symbol, interval, startTime=startTime, endTime=endTime, limit=limit)

    def get_funding_rates(
        self, symbol: str, startTime: Optional[int] = None, endTime: Optional[int] = None
    ):
        return self.sim.get_funding_rates(symbol, startTime=startTime, endTime=endTime)

    # -----------------
    # Config de cuenta
    # -----------------
    def set_leverage(self, symbol: str, leverage: int | float):
        self.sim.set_leverage(symbol, int(leverage))
        return {"leverage": int(leverage), "symbol": symbol}

    def set_position_mode(self, hedge_mode: bool = False):
        self.sim.set_position_mode(bool(hedge_mode))
        # Binance normalmente devuelve {"dualSidePosition": "true/false"} en otro endpoint,
        # pero mantenemos una respuesta simple.
        return {"code": 200, "msg": "success"}
