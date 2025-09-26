import asyncio
import json
import os
import sys
from datetime import datetime, timedelta, timezone
from typing import Any, Dict, List, Optional, Tuple

import httpx
import websockets
from pydantic import BaseModel, validator
from rich.console import Console
from rich.panel import Panel
from rich.table import Table

console = Console()


def ms(ts: datetime) -> int:
    """Convert datetime to epoch milliseconds."""
    return int(ts.timestamp() * 1000)


def now_ms() -> int:
    """Return current UTC timestamp in milliseconds."""
    return ms(datetime.now(timezone.utc))


def calc_range(minutes: int) -> Tuple[int, int]:
    """Calculate (startMs, endMs) covering the past `minutes`."""
    end = datetime.now(timezone.utc)
    start = end - timedelta(minutes=minutes)
    return ms(start), ms(end)


async def first_kline(ws_url: str, symbol: str, timeout: float = 20.0) -> Dict[str, Any]:
    """Listen to a websocket and return the first kline event for the given symbol."""
    target_symbol = symbol.upper()
    message_count = 0
    async with websockets.connect(ws_url) as ws:
        while True:
            raw = await asyncio.wait_for(ws.recv(), timeout=timeout)
            message_count += 1
            data = json.loads(raw)
            payload = data.get("data", data)
            if isinstance(payload, dict) and payload.get("e") == "kline":
                symbol_code = (payload.get("s") or payload.get("symbol") or "").upper()
                if symbol_code == target_symbol:
                    payload["_stream"] = data.get("stream") or symbol_code.lower()
                    payload["_messages"] = message_count
                    return payload


class Config(BaseModel):
    base_url: str
    ws_base: str
    session: Optional[str]
    symbols: List[str]
    interval: str
    range_minutes: int
    speed: float
    http_timeout: float = 10.0
    ws_timeout: float = 20.0

    @validator("symbols", pre=True)
    def ensure_symbols(cls, value: Any) -> List[str]:
        if isinstance(value, str):
            items = [item.strip() for item in value.split(",") if item.strip()]
            if not items:
                raise ValueError("At least one symbol must be provided")
            return [item.upper() for item in items]
        return value

    @classmethod
    def from_env(cls) -> "Config":
        return cls(
            base_url=os.getenv("BASE_URL", "http://localhost:3001"),
            ws_base=os.getenv("WS_BASE", "ws://localhost:3001"),
            session=os.getenv("SESSION") or None,
            symbols=os.getenv("SYMBOLS", "BTCUSDT,ETHBTC"),
            interval=os.getenv("INTERVAL", "1m"),
            range_minutes=int(os.getenv("RANGE_MINUTES", "120")),
            speed=float(os.getenv("SPEED", "1.0")),
        )


def ensure_numeric(order_id: Any) -> int:
    if isinstance(order_id, int):
        return order_id
    if isinstance(order_id, str) and order_id.isdigit():
        return int(order_id)
    raise AssertionError(f"orderId is not numeric: {order_id!r}")


async def create_session(client: httpx.AsyncClient, cfg: Config) -> str:
    start_ms, end_ms = calc_range(cfg.range_minutes)
    payload = {
        "symbols": cfg.symbols,
        "interval": cfg.interval,
        "startTime": start_ms,
        "endTime": end_ms,
        "speed": cfg.speed,
    }
    console.log("Creating new session", payload)
    response = await client.post("/api/v1/sessions", json=payload)
    response.raise_for_status()
    data = response.json()
    session_id = data.get("id") or data.get("sessionId")
    if not session_id:
        raise AssertionError("Session creation response missing id")
    console.log(f"Session created: {session_id}")
    return str(session_id)


async def start_session(client: httpx.AsyncClient, session_id: str) -> None:
    console.log(f"Starting session {session_id}")
    response = await client.post(f"/api/v1/sessions/{session_id}/start")
    response.raise_for_status()


async def poll_get(
    client: httpx.AsyncClient,
    path: str,
    params: Dict[str, Any],
    attempts: int = 5,
    delay: float = 1.0,
) -> httpx.Response:
    for attempt in range(1, attempts + 1):
        response = await client.get(path, params=params)
        try:
            response.raise_for_status()
            return response
        except httpx.HTTPStatusError:
            if attempt == attempts:
                raise
        await asyncio.sleep(delay)
    raise RuntimeError("poll_get exhausted attempts")


async def main() -> int:
    cfg = Config.from_env()
    primary_symbol = cfg.symbols[0]
    console.rule("E2E Binance-like Runner")
    async with httpx.AsyncClient(base_url=cfg.base_url, timeout=cfg.http_timeout) as client:
        session_id = cfg.session
        if not session_id:
            session_id = await create_session(client, cfg)
        else:
            console.log(f"Reusing session {session_id}")

        await start_session(client, session_id)

        streams = "/".join(f"{symbol.lower()}@kline_{cfg.interval}" for symbol in cfg.symbols)
        ws_url = f"{cfg.ws_base}/stream?streams={streams}&sessionId={session_id}"
        console.log(f"Connecting to WS: {ws_url}")
        kline_payload = await first_kline(ws_url, primary_symbol, timeout=cfg.ws_timeout)
        console.log("First kline received", kline_payload)

        order_ids: List[int] = []
        timestamp = now_ms()
        limit_order_payload = {
            "symbol": primary_symbol,
            "side": "BUY",
            "type": "LIMIT",
            "timeInForce": "GTC",
            "quantity": "1",
            "price": "0.01",
            "timestamp": str(timestamp),
            "sessionId": session_id,
        }
        limit_resp = await client.post("/api/v3/order", data=limit_order_payload)
        limit_resp.raise_for_status()
        limit_order = limit_resp.json()
        limit_order_id = ensure_numeric(limit_order.get("orderId"))
        order_ids.append(limit_order_id)
        console.log(f"LIMIT order placed: {limit_order_id}")

        market_payload = {
            "symbol": primary_symbol,
            "side": "SELL",
            "type": "MARKET",
            "quoteOrderQty": "100",
            "timestamp": str(now_ms()),
            "sessionId": session_id,
        }
        market_resp = await client.post("/api/v3/order", data=market_payload)
        market_resp.raise_for_status()
        market_order = market_resp.json()
        market_order_id = ensure_numeric(market_order.get("orderId"))
        order_ids.append(market_order_id)
        console.log(f"MARKET order placed: {market_order_id}")

        get_order_params = {
            "symbol": primary_symbol,
            "orderId": limit_order_id,
            "timestamp": str(now_ms()),
            "sessionId": session_id,
        }
        get_order_resp = await client.get("/api/v3/order", params=get_order_params)
        get_order_resp.raise_for_status()
        order_info = get_order_resp.json()
        assert order_info.get("symbol") == primary_symbol, "Unexpected symbol in order info"
        assert order_info.get("type") == "LIMIT", "Order type mismatch"
        assert order_info.get("timeInForce") == "GTC", "timeInForce mismatch"
        console.log("GET order validated", order_info)

        open_orders_params = {
            "symbol": primary_symbol,
            "timestamp": str(now_ms()),
            "sessionId": session_id,
        }
        open_orders_resp = await poll_get(client, "/api/v3/openOrders", open_orders_params)
        open_orders = open_orders_resp.json()
        assert isinstance(open_orders, list), "openOrders should return a list"
        for item in open_orders:
            ensure_numeric(item.get("orderId"))
        console.log(f"openOrders fetched: {len(open_orders)} items")

        my_trades_params = {
            "symbol": primary_symbol,
            "timestamp": str(now_ms()),
            "sessionId": session_id,
        }
        my_trades_resp = await poll_get(client, "/api/v3/myTrades", my_trades_params)
        my_trades = my_trades_resp.json()
        assert isinstance(my_trades, list), "myTrades should return a list"
        console.log(f"myTrades fetched: {len(my_trades)} items")

        cancel_params = {
            "symbol": primary_symbol,
            "orderId": limit_order_id,
            "timestamp": str(now_ms()),
            "sessionId": session_id,
        }
        cancel_resp = await client.delete("/api/v3/order", params=cancel_params)
        cancel_resp.raise_for_status()
        cancel_info = cancel_resp.json()
        status = cancel_info.get("status")
        assert status == "CANCELED", f"Expected status CANCELED, got {status}"
        console.log("Order canceled", cancel_info)

        openapi_resp = await client.get("/api-docs/openapi.v3.json")
        openapi_resp.raise_for_status()
        openapi_doc = openapi_resp.json()
        content = (
            openapi_doc
            .get("paths", {})
            .get("/api/v3/order", {})
            .get("post", {})
            .get("requestBody", {})
            .get("content", {})
        )
        assert "application/x-www-form-urlencoded" in content, "OpenAPI lacks form-urlencoded"
        console.log("OpenAPI validation passed")

        event_time = kline_payload.get("E") or kline_payload.get("k", {}).get("t")
        if isinstance(event_time, int):
            event_dt = datetime.fromtimestamp(event_time / 1000, tz=timezone.utc)
            event_time_str = event_dt.isoformat()
        else:
            event_time_str = str(event_time)

        summary_table = Table(title="E2E Summary", show_lines=True)
        summary_table.add_column("Key", style="cyan", no_wrap=True)
        summary_table.add_column("Value", style="magenta")
        summary_table.add_row("Session ID", session_id)
        summary_table.add_row("WS Streams", streams)
        summary_table.add_row("WS Messages", str(kline_payload.get("_messages", 1)))
        summary_table.add_row("First Kline Event Time", event_time_str)
        summary_table.add_row("First Kline Close", json.dumps(kline_payload.get("k", {})))
        summary_table.add_row("Order IDs", ", ".join(str(x) for x in order_ids))
        summary_table.add_row("openOrders Count", str(len(open_orders)))
        summary_table.add_row("myTrades Count", str(len(my_trades)))
        summary_table.add_row("Cancel Status", status)

        console.print(Panel(summary_table, title="Binance-like Exchange Simulator"))
    return 0


if __name__ == "__main__":
    try:
        exit_code = asyncio.run(main())
    except AssertionError as exc:
        console.print_exception()
        exit_code = 1
    except Exception:
        console.print_exception()
        exit_code = 1
    sys.exit(exit_code)
