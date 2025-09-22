use axum::{extract::Query, routing::get, Extension, Json, Router};
use tracing::instrument;

use crate::{
    api::errors::ApiResult,
    app::bootstrap::AppState,
    dto::account::{AccountQuery, AccountResponse},
};

pub fn router() -> Router {
    Router::new().route("/api/v3/account", get(get_account))
}

#[utoipa::path(
    get,
    path = "/api/v3/account",
    params(AccountQuery),
    responses((status = 200, body = AccountResponse))
)]
#[instrument(skip(state, params))]
pub async fn get_account(
    Extension(state): Extension<AppState>,
    Query(params): Query<AccountQuery>,
) -> ApiResult<Json<AccountResponse>> {
    state
        .account_service
        .ensure_session_account(params.session_id)
        .await?;
    let account = state.account_service.get_account(params.session_id).await?;
    Ok(Json(account.into()))
}
