use axum::{
    Json, Router,
    extract::{Query, State},
    routing::{delete, get, post},
};
use tracing::instrument;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    domain::value_objects::{Price, Quantity},
    dto::orders::{
        CancelOrderParams, FillResponse, MyTradesParams, NewOrderRequest, NewOrderResponse,
        OpenOrdersParams, OrderResponse, QueryOrderParams,
    },
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v3/order",
            post(new_order).get(get_order).delete(cancel_order),
        )
        .route("/api/v3/openOrders", get(open_orders))
        .route("/api/v3/myTrades", get(my_trades))
}

#[utoipa::path(
    post,
    path = "/api/v3/order",
    request_body = NewOrderRequest,
    responses((status = 200, body = NewOrderResponse))
)]
#[instrument(skip(state, payload))]
pub async fn new_order(
    State(state): State<AppState>,
    Json(payload): Json<NewOrderRequest>,
) -> ApiResult<Json<NewOrderResponse>> {
    let (order, fills) = state
        .orders_service
        .place_order(
            payload.session_id,
            payload.symbol.clone(),
            payload.side,
            payload.order_type,
            Quantity(payload.quantity),
            payload.price.map(Price::from),
            payload.client_order_id,
        )
        .await?;
    Ok(Json(NewOrderResponse {
        order: order.into(),
        fills: fills.into_iter().map(FillResponse::from).collect(),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v3/order",
    params(QueryOrderParams),
    responses((status = 200, body = OrderResponse))
)]
#[instrument(skip(state, params))]
pub async fn get_order(
    State(state): State<AppState>,
    Query(params): Query<QueryOrderParams>,
) -> ApiResult<Json<OrderResponse>> {
    let order = if let Some(order_id) = params.order_id {
        state
            .orders_service
            .get_order(params.session_id, order_id)
            .await?
    } else if let Some(client) = params.orig_client_order_id {
        state
            .orders_service
            .get_by_client_id(params.session_id, &client)
            .await?
    } else {
        return Err(crate::error::AppError::Validation(
            "orderId or origClientOrderId is required".into(),
        ));
    };
    Ok(Json(order.into()))
}

#[utoipa::path(
    delete,
    path = "/api/v3/order",
    params(CancelOrderParams),
    responses((status = 200, body = OrderResponse))
)]
#[instrument(skip(state, params))]
pub async fn cancel_order(
    State(state): State<AppState>,
    Query(params): Query<CancelOrderParams>,
) -> ApiResult<Json<OrderResponse>> {
    let order = if let Some(id) = params.order_id {
        state
            .orders_service
            .cancel_order(params.session_id, id)
            .await?
    } else if let Some(client_id) = params.orig_client_order_id {
        let order = state
            .orders_service
            .get_by_client_id(params.session_id, &client_id)
            .await?;
        state
            .orders_service
            .cancel_order(params.session_id, order.order_id)
            .await?
    } else {
        return Err(crate::error::AppError::Validation(
            "orderId or origClientOrderId is required".into(),
        ));
    };
    Ok(Json(order.into()))
}

#[utoipa::path(
    get,
    path = "/api/v3/openOrders",
    params(OpenOrdersParams),
    responses((status = 200, body = Vec<OrderResponse>))
)]
#[instrument(skip(state, params))]
pub async fn open_orders(
    State(state): State<AppState>,
    Query(params): Query<OpenOrdersParams>,
) -> ApiResult<Json<Vec<OrderResponse>>> {
    let orders = state
        .orders_service
        .list_open(params.session_id, params.symbol.as_deref())
        .await?;
    Ok(Json(orders.into_iter().map(OrderResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v3/myTrades",
    params(MyTradesParams),
    responses((status = 200, body = Vec<FillResponse>))
)]
#[instrument(skip(state, params))]
pub async fn my_trades(
    State(state): State<AppState>,
    Query(params): Query<MyTradesParams>,
) -> ApiResult<Json<Vec<FillResponse>>> {
    let trades = state
        .orders_service
        .my_trades(params.session_id, &params.symbol)
        .await?;
    Ok(Json(trades.into_iter().map(FillResponse::from).collect()))
}
