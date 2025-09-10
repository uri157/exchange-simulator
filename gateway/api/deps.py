from __future__ import annotations
from fastapi import Request

def get_sim(req: Request):
    """Obtiene el estado compartido (SimState) desde la app."""
    return req.app.state.sim
