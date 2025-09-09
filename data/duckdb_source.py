# data/duckdb_source.py
from __future__ import annotations

from typing import List, Optional, Dict, Any
import os
import duckdb


class DuckDBSource:
    """
    Loader de datos desde DuckDB con la misma interfaz que binance_api/binance_files:
      - get_klines(...) -> lista de [openTime, open, high, low, close, volume, closeTime]
      - get_funding_rates(...) -> lista de dicts {"fundingTime": ..., "fundingRate": ...}

    Nota: por defecto abrimos la DB en modo escritura (read_only=False) para
    que otras rutinas (p.ej. guardado automático del bt_runner) puedan escribir
    en la misma base sin conflicto de configuraciones.
    """

    def __init__(
        self,
        db_path: str = "data/duckdb/exsim.duckdb",
        read_only: bool = False,
        config: Optional[Dict[str, Any]] = None,
    ):
        if not os.path.exists(db_path):
            raise FileNotFoundError(f"DuckDB no encontrado: {db_path}")

        self.db_path = db_path
        self.read_only = bool(read_only)
        self._config: Dict[str, Any] = dict(config or {})

        # Abrimos la conexión con los mismos kwargs que deberían usarse en cualquier
        # otra conexión al mismo archivo para evitar:
        #   "Can't open a connection to same database file with a different configuration"
        self.con = duckdb.connect(
            database=db_path,
            read_only=self.read_only,
            config=self._config,
        )

    # ------------------------------------------------------------------ #
    # Helpers
    # ------------------------------------------------------------------ #
    def _table_for_interval(self, interval: str) -> str:
        i = (interval or "").lower()
        if i in ("1m", "1min", "1"):
            return "ohlc_1m"
        # El resto de temporalidades van a la tabla 'ohlc' (ej. 1h, 4h, 1d)
        return "ohlc"

    # ------------------------------------------------------------------ #
    # API pública estilo binance_*
    # ------------------------------------------------------------------ #
    def get_klines(
        self,
        symbol: str,
        interval: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
        limit: Optional[int] = None,
    ) -> List[List[float]]:
        table = self._table_for_interval(interval)

        sql = f"""
            SELECT ts, open, high, low, close, volume, close_ts
            FROM {table}
            WHERE symbol = ?
        """
        params: List[Any] = [symbol]

        if startTime is not None:
            sql += " AND ts >= ?"
            params.append(int(startTime))
        if endTime is not None:
            sql += " AND ts <= ?"
            params.append(int(endTime))

        sql += " ORDER BY ts ASC"
        if limit is not None:
            sql += " LIMIT ?"
            params.append(int(limit))

        rows = self.con.execute(sql, params).fetchall()

        # Formato Binance-like
        return [
            [int(ts), float(o), float(h), float(l), float(c), float(v), int(cts)]
            for (ts, o, h, l, c, v, cts) in rows
        ]

    def get_funding_rates(
        self,
        symbol: str,
        startTime: Optional[int] = None,
        endTime: Optional[int] = None,
    ) -> List[Dict[str, Any]]:
        sql = """
            SELECT funding_time, funding_rate
            FROM funding
            WHERE symbol = ?
        """
        params: List[Any] = [symbol]

        if startTime is not None:
            sql += " AND funding_time >= ?"
            params.append(int(startTime))
        if endTime is not None:
            sql += " AND funding_time <= ?"
            params.append(int(endTime))

        sql += " ORDER BY funding_time ASC"

        rows = self.con.execute(sql, params).fetchall()
        return [{"fundingTime": int(t), "fundingRate": float(r)} for (t, r) in rows]

    # ------------------------------------------------------------------ #
    # Gestión de recurso
    # ------------------------------------------------------------------ #
    def close(self) -> None:
        try:
            self.con.close()
        except Exception:
            pass

    def __enter__(self) -> "DuckDBSource":
        return self

    def __exit__(self, exc_type, exc, tb) -> None:
        self.close()
