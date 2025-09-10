# gateway/ws/stream.py
"""
WebSocket multiplexer
- Expone dos paths compatibles: `/stream` y `/ws/stream` (alias)
- Acepta query `streams` como en Binance (no filtra por stream; se emiten todos los mensajes)
- Registra/desregistra clientes contra `SimState` (en app.state.sim)
"""
from __future__ import annotations

from fastapi import FastAPI, WebSocket, WebSocketDisconnect, Query


def mount(app: FastAPI) -> None:
    """
    Registra rutas WS en la app. Debe invocarse **después** de setear `app.state.sim`.
    Ej.: 
        app.state.sim = SimState(cfg)
        mount(app)
    """
    # Validación temprana para evitar 404 silenciosos si falta el estado
    if not hasattr(app.state, "sim"):
        raise RuntimeError("app.state.sim no está inicializado; setéalo antes de montar el WS.")

    async def _handler(ws: WebSocket, streams: str = Query("")) -> None:
        """
        `streams` queda por compatibilidad (btcusdt@kline_1h/...), 
        pero el servidor emite a todos los clientes conectados.
        """
        sim = app.state.sim  # SimState
        await ws.accept()
        # registrar
        try:
            # algunos SimState exponen register_ws (async); otros podrían ser sync.
            reg = getattr(sim, "register_ws", None)
            if callable(reg):
                res = reg(ws)
                if hasattr(res, "__await__"):  # es coroutine
                    await res
            else:
                # compat: register_ws_sync
                reg2 = getattr(sim, "register_ws_sync", None)
                if callable(reg2):
                    reg2(ws)
        except Exception:
            try:
                await ws.close()
            finally:
                # best-effort unregister
                unreg = getattr(sim, "unregister_ws", None) or getattr(sim, "unregister_ws_sync", None)
                if callable(unreg):
                    try:
                        res = unreg(ws)
                        if hasattr(res, "__await__"):
                            await res
                    except Exception:
                        pass
            return

        # mantener vivo el socket
        try:
            while True:
                # No esperamos mensajes del cliente; consumimos para mantener la conexión
                # (aceptamos TEXT o BINARY por compatibilidad)
                msg = await ws.receive()
                if msg["type"] in ("websocket.disconnect", "websocket.close"):
                    break
        except WebSocketDisconnect:
            pass
        except Exception:
            # swallow; haremos un unregister abajo
            pass
        finally:
            unreg = getattr(sim, "unregister_ws", None) or getattr(sim, "unregister_ws_sync", None)
            if callable(unreg):
                try:
                    res = unreg(ws)
                    if hasattr(res, "__await__"):
                        await res
                except Exception:
                    pass

    # Rutas compatibles con Binance y con la versión previa del gateway
    app.add_api_websocket_route("/stream", _handler)
    app.add_api_websocket_route("/ws/stream", _handler)
