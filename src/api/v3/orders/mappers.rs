use uuid::Uuid;

use crate::{
    app::bootstrap::AppState,
    domain::models::{Order, OrderSide, OrderStatus, OrderType},
    dto::v3::orders::BinanceOrderDetails,
    error::AppError,
};

use super::types::ORDER_LIST_ID_NONE;

pub fn format_decimal(value: f64) -> String {
    format!("{:.8}", value)
}

pub fn format_status(status: OrderStatus) -> String {
    match status {
        OrderStatus::New => "NEW".to_string(),
        OrderStatus::Filled => "FILLED".to_string(),
        OrderStatus::PartiallyFilled => "PARTIALLY_FILLED".to_string(),
        OrderStatus::Canceled => "CANCELED".to_string(),
    }
}

pub fn format_order_type(order_type: OrderType) -> String {
    match order_type {
        OrderType::Market => "MARKET".to_string(),
        OrderType::Limit => "LIMIT".to_string(),
    }
}

pub fn format_side(side: OrderSide) -> String {
    match side {
        OrderSide::Buy => "BUY".to_string(),
        OrderSide::Sell => "SELL".to_string(),
    }
}

pub async fn build_order_details(
    state: &AppState,
    session_id: Uuid,
    order: Order,
) -> Result<BinanceOrderDetails, AppError> {
    let numeric_id = state
        .order_id_mapping
        .ensure_mapping(session_id, order.order_id)
        .await;
    let fills = state
        .orders_service
        .my_trades(session_id, &order.symbol)
        .await?;
    let relevant_fills: Vec<_> = fills
        .into_iter()
        .filter(|fill| fill.order_id == order.order_id)
        .collect();
    let cumm_quote: f64 = relevant_fills
        .iter()
        .map(|f| f.price.0 * f.quantity.0)
        .sum();

    let status = order.status.clone();
    let side = order.side.clone();
    let order_type = order.order_type.clone();

    Ok(BinanceOrderDetails {
        symbol: order.symbol.clone(),
        order_id: numeric_id,
        order_list_id: ORDER_LIST_ID_NONE,
        client_order_id: order.client_order_id.clone(),
        price: format_decimal(order.price.map(|p| p.0).unwrap_or(0.0)),
        orig_qty: format_decimal(order.quantity.0),
        executed_qty: format_decimal(order.filled_quantity.0),
        cummulative_quote_qty: format_decimal(cumm_quote),
        status: format_status(status.clone()),
        time_in_force: "GTC".to_string(),
        order_type: format_order_type(order_type),
        side: format_side(side.clone()),
        stop_price: format_decimal(0.0),
        iceberg_qty: format_decimal(0.0),
        time: order.created_at.0,
        update_time: order.created_at.0,
        is_working: !matches!(status, OrderStatus::Canceled),
        working_time: order.created_at.0,
        orig_quote_order_qty: format_decimal(
            order.price.map(|p| p.0 * order.quantity.0).unwrap_or(0.0),
        ),
    })
}
