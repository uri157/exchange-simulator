"""
Persistencia mínima en DuckDB para el simulador.
Provee `RunStore` con:
- new_run(strategy, params)
- log_fill(ts, symbol, side, price, qty, realized, fee, is_maker)
- log_equity(ts, equity)

Tolera esquemas con o sin `created_at` en `runs`.
Asume que las tablas existen (creadas por scripts.init_duckdb),
pero expone `ensure_min_schema()` para entornos de prueba.
"""
from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Dict, Optional
import json
import uuid

import duckdb


@dataclass
class RunStore:
    con: duckdb.DuckDBPyConnection
    run_id: Optional[str] = None
    _seq: int = 0

    # ---------------- introspección / utils ----------------
    def _has_table(self, name: str) -> bool:
        try:
            row = self.con.sql(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_name = ?",
                params=[name],
            ).fetchone()
            return bool(row and row[0] > 0)
        except Exception:
            return False

    def _has_column(self, table: str, column: str) -> bool:
        try:
            rows = self.con.sql(f"PRAGMA table_info('{table}')").fetchall()
            return any(r[1] == column for r in rows)
        except Exception:
            return False

    def ensure_min_schema(self) -> None:
        """Crea tablas mínimas si no existen (útil para tests rápidos)."""
        self.con.sql(
            """
            CREATE TABLE IF NOT EXISTS runs (
                run_id VARCHAR PRIMARY KEY,
                created_at TIMESTAMP DEFAULT now(),
                strategy VARCHAR,
                params_json JSON,
                feature_set_id VARCHAR,
                code_hash VARCHAR
            );
            """
        )
        self.con.sql(
            """
            CREATE TABLE IF NOT EXISTS trades_fills (
                run_id VARCHAR,
                seq BIGINT,
                ts BIGINT,
                symbol VARCHAR,
                side VARCHAR,
                price DOUBLE,
                qty DOUBLE,
                realized_pnl DOUBLE,
                fee DOUBLE,
                is_maker BOOLEAN
            );
            """
        )
        self.con.sql(
            """
            CREATE TABLE IF NOT EXISTS equity_curve (
                run_id VARCHAR,
                ts BIGINT,
                equity DOUBLE
            );
            """
        )

    # ---------------- API pública -------------------------
    def new_run(self, strategy: str, params: Dict[str, Any]) -> str:
        rid = str(uuid.uuid4())
        self.run_id = rid
        self._seq = 0
        # Soporta esquemas con/ sin created_at explícito
        has_created = self._has_column("runs", "created_at")
        if has_created:
            self.con.sql(
                """
                INSERT INTO runs (run_id, created_at, strategy, params_json, feature_set_id, code_hash)
                VALUES (?, now(), ?, ?, NULL, NULL)
                """,
                params=[rid, strategy, json.dumps(params)],
            )
        else:
            self.con.sql(
                """
                INSERT INTO runs (run_id, strategy, params_json, feature_set_id, code_hash)
                VALUES (?, ?, ?, NULL, NULL)
                """,
                params=[rid, strategy, json.dumps(params)],
            )
        return rid

    def log_fill(
        self,
        ts: int,
        symbol: str,
        side: str,
        price: float,
        qty: float,
        realized: float,
        fee: float,
        is_maker: bool,
    ) -> None:
        if not self.run_id:
            return
        self._seq += 1
        self.con.sql(
            """
            INSERT INTO trades_fills
                (run_id, seq, ts, symbol, side, price, qty, realized_pnl, fee, is_maker)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            """,
            params=[self.run_id, self._seq, ts, symbol, side, price, qty, realized, fee, is_maker],
        )

    def log_equity(self, ts: int, equity: float) -> None:
        if not self.run_id:
            return
        self.con.sql(
            """
            INSERT INTO equity_curve (run_id, ts, equity)
            VALUES (?, ?, ?)
            """,
            params=[self.run_id, ts, equity],
        )
