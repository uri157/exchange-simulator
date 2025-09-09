# Simulador Universal de Exchange y Backtester

Este repositorio contiene un _skeleton_ de un **simulador universal de exchange** (exchange simulado) y herramientas de **backtesting** y **replay** para estrategias de trading de cripto (futuros USD-M de Binance). Ha sido diseñado para ser compatible por interfaz con bots existentes (por ejemplo, estrategias de momentum y basis), de forma que se pueda _enchufar_ un `SimExchange` en lugar del exchange real sin cambiar la lógica de la estrategia.

## Instalación

Requiere Python 3.11+. Crear un entorno virtual si se desea, luego instalar dependencias básicas:
```bash
pip install -r requirements.txt
Las dependencias se mantienen al mínimo (principalmente requests para llamadas a la API de Binance).
Estructura del Proyecto
sim/
  exchange_sim.py       # Implementación del exchange simulado (SimExchange)
  fill_models.py        # Modelos de fill/ejecución de órdenes (OHLC, aleatorio, etc.)
  models.py             # Definición de entidades: Order, Fill, Position, etc.
  adapters/
    binance_like.py     # Adapter para interfaz estilo Binance (UMFutures)
data/
  binance_api.py        # Cargador de datos vía API de Binance (klines, funding rates)
  binance_files.py      # Cargador de datos desde archivos históricos (data.binance.vision)
backtests/
  bt_runner.py          # Runner de backtesting batch (ejecuta un backtest completo)
replay/
  replayer.py           # Replay de mercado en "tiempo real" acelerado
reports/                # Carpeta donde se guardan resultados (CSV, JSON)
tests/                  # Tests básicos unitarios
config/
  default.yaml          # Configuración por defecto (ej. fees, paths) - opcional
cli.py                  # CLI unificado (opcional, se puede usar -m backtests.bt_runner)
README.md
Uso: Backtest
Para ejecutar un backtest de ejemplo utilizando datos históricos de la API de Binance, utilizar el módulo backtests.bt_runner. Por ejemplo:
python -m backtests.bt_runner --symbol BTCUSDT --interval 1h \
    --start 2024-01-01 --end 2024-06-30 \
    --data-source api --fill-model ohlc_up --maker-bps 2 --taker-bps 4 --slippage-bps 1 --seed 42
Este comando descargará datos de velas 1h para BTCUSDT desde la API (del 1 de enero al 30 de junio de 2024) y ejecutará un backtest: - --data-source api: usa la API pública de Binance para obtener datos (data/binance_api.py). Alternativamente, se puede usar --data-source files para leer de archivos locales (ver sección siguiente). - --fill-model ohlc_up: modelo de ejecución determinista asumiendo que en cada vela el precio va primero al High y luego al Low. Otras opciones: ohlc_down (Low primero), random (aleatoriza orden H/L con semilla), book (simulación usando spread bid/ask simple). - --maker-bps 2 --taker-bps 4: fees de maker 0.02% y taker 0.04% (en basis points). Estas comisiones se aplicarán a cada fill según corresponda. - --slippage-bps 1: slippage de 0.01% aplicado a precios de ejecución (p. ej. en órdenes de mercado). - --seed 42: semilla RNG para el modelo aleatorio (si se usa random).
Al terminar, el backtester generará archivos en la carpeta reports/: - trades.csv: lista de trades ejecutados (timestamp, side, price, qty, PnL realizado, fee, etc.). - equity.csv: curva de equity a lo largo del tiempo (timestamp, equity). - summary.json: resumen de métricas de desempeño (número de trades, winrate, profit factor, Sharpe, Sortino, drawdown máximo, retornos promedio, etc.). Además, se muestra un resumen por consola con los valores principales.
Uso: Data Files locales
Para usar datos históricos descargados en lugar de la API, pasar --data-source files al correr el backtest. Por ejemplo:
python -m backtests.bt_runner --symbol BTCUSDT --interval 1h \
    --start 2023-01-01 --end 2023-03-31 --data-source files
Asegúrese de descargar los archivos de velas correspondientes desde data.binance.vision. Por ejemplo, para BTCUSDT 1h: - BTCUSDT-1h-2023-01.zip, BTCUSDT-1h-2023-02.zip, etc. (contienen datos de velas 1h por mes). Coloque estos archivos en el directorio ./data/binance (por defecto) o especifique la variable de entorno BINANCE_DATA_DIR apuntando al directorio que contiene los archivos. El cargador BinanceFileData leerá automáticamente los CSV/ZIP necesarios para cubrir el rango de fechas solicitado. Igualmente, se pueden descargar archivos de funding rates (e.g. BTCUSDT-funding-2023-01.zip) para incluir eventos de funding.
Uso: Replay en tiempo acelerado
El módulo replay/replayer.py permite reproducir la serie histórica en "tiempo real" acelerado, para probar un bot de forma end-to-end. Ejemplo:
python -m replay.replayer --symbol BTCUSDT --interval 15m \
    --start 2024-01-01 --end 2024-01-07 --speed 60
Esto cargará velas de 15 minutos de la primera semana de 2024 y las emitirá en consola en tiempo acelerado (60x más rápido que el tiempo real, es decir cada vela de 15m se imprime cada 15s). Puede conectar su estrategia para que consuma estos eventos (por ejemplo instanciándola con el mismo SimExchange y recibiendo ticks). En esta implementación de ejemplo, simplemente se imprime la hora y precio de cierre de cada vela junto con la equity de la cuenta simulada.
Adaptador de Interfaz (Binance-like)
El archivo sim/adapters/binance_like.py contiene un adapter BinanceLikeExchange que envuelve un SimExchange y expone métodos con la misma firma y formato de respuesta que la API de Binance Futures. Esto incluye métodos como new_order, cancel_order, get_open_orders, position_risk, account_info, etc., devolviendo diccionarios con campos equivalentes (por ejemplo, status, orderId, clientOrderId, etc.). Si su bot espera interactuar con un cliente Binance (por ejemplo UMFutures), puede utilizar este adapter para traducir entre la simulación y la interfaz esperada, sin modificar la lógica del bot.
Arquitectura (Mermaid)
flowchart LR
    DataSource -->|velas & funding| Backtester
    Backtester -->|nueva vela| Estrategia/Bot
    Estrategia/Bot -->|órdenes (API Exchange)| ExchangeSimulado
    ExchangeSimulado -->|fills & PnL| Backtester
    Backtester -->|métricas| Reportes
En el flujo de arriba: - El DataSource (API de Binance o archivos locales) provee los datos históricos de mercado (velas y tasas de funding) al Backtester. - El Backtester itera sobre las velas y, por cada nueva vela, notifica a la Estrategia (por ejemplo llamando su método on_new_candle). La estrategia, utilizando el Exchange Simulado, envía órdenes (por ejemplo, new_order, cancel_order, etc.). - El SimExchange procesa las órdenes según el modelo de fill seleccionado, simulando ejecuciones, aplicando fees y funding, y actualizando las posiciones y PnL. Los resultados de fills (trades ejecutados) y cambios en la posición se reportan de vuelta al Backtester. - El Backtester registra los trades, la equity de la cuenta a través del tiempo y calcula métricas de rendimiento, que luego vuelca en los reportes (CSV/JSON) al finalizar.
Notas y Ampliaciones
Modelos de Fill: Se incluyen modelos de ejecución determinísticos basados en el camino OHLC (tanto up-first como down-first), un modelo aleatorio reproducible (RandomOHLC con semilla configurable) y un modelo simplificado basado en book ticker (considerando un spread fijo). Existe la estructura para implementar modelos más sofisticados (por ejemplo usando libro de órdenes L2), dejando # TODO donde correspondería integrar lógica más detallada.
Modo Hedge: Por defecto el simulador maneja posición en modo one-way (posición neta por símbolo). Se incluyó un flag hedge_mode en SimExchange y métodos set_position_mode, pero la simulación actualmente trata la posición de forma neta. Sería posible expandir la lógica para mantener dos posiciones (LONG/SHORT) simultáneamente si se quisiera soportar completamente el modo hedge.
Parciales y latencia: El simulador actualmente llena las órdenes en su totalidad cuando se cumplen las condiciones en una vela. Para simplificar, no se simula partial fills en múltiples velas ni retrasos por latencia; sin embargo, el diseño (especialmente en fill_models.py) deja espacio para introducir proporciones de fill parciales o ejecuciones distribuidas en el tiempo.
Métricas: El resumen calcula win rate, profit factor, Sharpe, Sortino, drawdown, y retornos medios semanales/mensuales. Estos cálculos suponen un seguimiento diario de equity (Sharpe/Sortino anualizados con 365 días). En caso de periodos muy cortos o ausencia de trades, algunas métricas pueden resultar no aplicables (null o "inf" en el JSON).
Pruebas unitarias: En la carpeta tests/ se incluyen pruebas básicas (pytest) para verificar, por ejemplo, que los modelos de fill producen los resultados esperados en escenarios simples (llenado de órdenes limit, stop en distintas trayectorias de precio) y que el cálculo de PnL y lógica reduceOnly funcionan correctamente.
Conclusión
Este esqueleto proporciona los componentes fundamentales para simular un exchange y ejecutar backtests reproducibles de estrategias de trading. Se espera que pueda integrarse con estrategias existentes con mínimos cambios, proporcionando así una plataforma para experimentar y validar algoritmos de trading de manera segura y rápida.
file: requirements.txt
```text
requests

