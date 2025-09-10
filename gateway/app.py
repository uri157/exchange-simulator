"""
ExSim — FastAPI factory
Arma la app, inyecta el estado del simulador y conecta routers + websockets.
"""
from __future__ import annotations

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from .core.models import ReplayConfig
from .core.state import SimState

# Routers HTTP
from .api.routes_market import router as market_router   # /fapi/v1/time, /klines, /fundingRate, /premiumIndex
from .api.routes_meta import router as meta_router       # /fapi/v1/exchangeInfo
from .api.routes_orders import router as orders_router   # /fapi/v1/order, /allOpenOrders, /ticker/bookTicker, /openOrders
from .api.routes_account import router as account_router # /fapi/v2/positionRisk, /fapi/v2/balance, leverage/margin/dual, listenKey
from .api.routes_admin import router as admin_router     # /admin/status, /admin/replay  (rutas absolutas en el router)

# WebSocket multiplexor (expone /stream y /ws/stream)
from .ws.stream import mount as mount_ws


def create_app(cfg: ReplayConfig) -> FastAPI:
    app = FastAPI(
        title="ExSim — Binance Futures subset",
        version="0.1.0",
        docs_url="/docs",
        redoc_url="/redoc",
    )

    # CORS abierto para pruebas locales / docker-compose
    app.add_middleware(
        CORSMiddleware,
        allow_origins=["*"],
        allow_credentials=True,
        allow_methods=["*"],
        allow_headers=["*"],
    )

    # Estado compartido del simulador
    app.state.sim = SimState(cfg)

    # Enrutar HTTP
    app.include_router(market_router)
    app.include_router(meta_router)
    app.include_router(orders_router)
    app.include_router(account_router)
    # OJO: admin_router ya define rutas absolutas (/admin/...), no agregamos prefix aquí
    app.include_router(admin_router)

    # Montar websockets (/stream y /ws/stream). Debe ir después de setear app.state.sim
    mount_ws(app)

    @app.get("/")
    def root():
        return {
            "name": "ExSim — Binance Futures subset",
            "docs": "/docs",
            "routes": "/__routes",
            "ws": ["/stream?streams=btcusdt@kline_1h", "/ws/stream?streams=btcusdt@kline_1h"],
        }

    @app.get("/__routes")
    def __routes():
        return [{"path": r.path, "methods": list(r.methods or [])} for r in app.router.routes]

    @app.on_event("startup")
    async def _startup():
        await app.state.sim.start()

    @app.on_event("shutdown")
    async def _shutdown():
        await app.state.sim.stop()

    return app
