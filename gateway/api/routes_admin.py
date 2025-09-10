"""
Rutas administrativas (sin auth por ahora):
- GET  /admin/status  → snapshot de estado
- POST /admin/replay  → reconfigura rango/velocidad/símbolo y reinicia el stream
"""
from __future__ import annotations

from typing import Optional

from fastapi import APIRouter, Depends
from pydantic import BaseModel

from .deps import get_sim
from ..core.models import parse_interval_str
from ..core.models import Account
from ..core.executor import Executor

router = APIRouter()


class ReplayBody(BaseModel):
    symbol: Optional[str] = None
    interval: Optional[str] = None
    start_ts: Optional[int] = None
    end_ts: Optional[int] = None
    speed_bars_per_sec: Optional[float] = None
    starting_balance: Optional[float] = None
    maker_bps: Optional[float] = None
    taker_bps: Optional[float] = None
    slippage_bps: Optional[float] = None


@router.get("/admin/status")
def admin_status(sim = Depends(get_sim)):
    px = sim.cur_price if sim.cur_price > 0 else (
        sim.replayer._bars[0][1] if sim.replayer._bars else 0.0
    )
    return {
        "symbol": sim.symbol,
        "interval": sim.interval,
        "run_id": sim.store.run_id,
        "ws_clients": len(sim.ws_clients),
        "bars_loaded": sim.replayer.bars_count,
        "equity_now": sim.account.mark_to_market(px) if px > 0 else sim.account.cash,
        "position": {"qty": sim.account.position.qty, "avg_price": sim.account.position.avg_price},
        "leverage": sim.leverage,
        "margin_type": sim.margin_type,
        "dual_side": sim.dual_side,
    }


@router.post("/admin/replay")
async def admin_replay(cfg_in: ReplayBody, sim = Depends(get_sim)):
    # Actualizar cfg/base
    if cfg_in.symbol is not None:
        sim.symbol = cfg_in.symbol.upper()
    if cfg_in.interval is not None:
        sim.interval = parse_interval_str(cfg_in.interval)
    if cfg_in.start_ts is not None:
        sim.cfg.start_ts = int(cfg_in.start_ts)
    if cfg_in.end_ts is not None:
        sim.cfg.end_ts = int(cfg_in.end_ts)
    if cfg_in.speed_bars_per_sec is not None:
        sim.cfg.speed_bars_per_sec = float(cfg_in.speed_bars_per_sec)
    if cfg_in.maker_bps is not None:
        sim.cfg.maker_bps = float(cfg_in.maker_bps)
    if cfg_in.taker_bps is not None:
        sim.cfg.taker_bps = float(cfg_in.taker_bps)
    if cfg_in.slippage_bps is not None:
        sim.cfg.slippage_bps = float(cfg_in.slippage_bps)

    # Reset de cuenta si cambió el starting_balance
    if cfg_in.starting_balance is not None:
        sim.cfg.starting_balance = float(cfg_in.starting_balance)
        sim.account = Account(cash=sim.cfg.starting_balance)
        sim.exec = Executor(
            account=sim.account,
            store=sim.store,
            maker_bps=sim.cfg.maker_bps,
            taker_bps=sim.cfg.taker_bps,
            slippage_bps=sim.cfg.slippage_bps,
        )

    # Reconfigurar replayer
    sim.replayer.set_params(
        symbol=sim.symbol,
        interval=sim.interval,
        start_ts=sim.cfg.start_ts,
        end_ts=sim.cfg.end_ts,
        bars_per_sec=sim.cfg.speed_bars_per_sec,
    )

    # Nuevo run y restart del stream para aplicar cambios
    params = {
        "symbol": sim.symbol,
        "interval": sim.interval,
        "start_ts": sim.cfg.start_ts,
        "end_ts": sim.cfg.end_ts,
        "maker_bps": sim.cfg.maker_bps,
        "taker_bps": sim.cfg.taker_bps,
        "slippage_bps": sim.cfg.slippage_bps,
        "starting_balance": sim.cfg.starting_balance,
        "speed_bars_per_sec": sim.cfg.speed_bars_per_sec,
    }
    run_id = sim.store.new_run(strategy="gateway/binance-sim", params=params)

    # Reiniciar para asegurar que el generador toma la nueva configuración
    if sim.running:
        await sim.stop()
    await sim.start()

    return {"ok": True, "run_id": run_id, "bars": sim.replayer.bars_count}
