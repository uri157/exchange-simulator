use std::sync::Arc;

use exchange_simulator::domain::traits::OrdersRepo;
use exchange_simulator::domain::{
    models::{AggTrade, Balance, FeeConfig, Liquidity, Order, OrderSide, OrderStatus, OrderType},
    value_objects::{Quantity, TimestampMs},
};
use exchange_simulator::infra::repos::memory::{MemoryAccountsRepo, MemoryOrdersRepo};
use exchange_simulator::services::{account_service::AccountService, matching_spot::SpotMatcher};
use uuid::Uuid;

fn default_fee_config() -> FeeConfig {
    FeeConfig {
        maker_bps: 8,
        taker_bps: 10,
        partial_fills: true,
    }
}

fn trade(symbol: &str, trade_id: i64, price: f64, qty: f64, ts: i64) -> AggTrade {
    AggTrade {
        symbol: symbol.to_string(),
        event_time: TimestampMs(ts),
        trade_id,
        price,
        qty,
        quote_qty: price * qty,
        is_buyer_maker: false,
    }
}

fn base_order(
    session_id: Uuid,
    symbol: &str,
    side: OrderSide,
    order_type: OrderType,
    price: Option<f64>,
    qty: f64,
    maker_taker: Option<Liquidity>,
) -> Order {
    Order {
        id: Uuid::new_v4(),
        session_id,
        client_order_id: None,
        symbol: symbol.to_string(),
        side,
        order_type,
        price: price.map(Into::into),
        quantity: Quantity(qty),
        filled_quantity: Quantity::default(),
        status: OrderStatus::New,
        created_at: TimestampMs(0),
        updated_at: TimestampMs(0),
        maker_taker,
    }
}

async fn setup_matcher(
    fees: FeeConfig,
) -> (
    Arc<SpotMatcher>,
    Arc<MemoryOrdersRepo>,
    Arc<AccountService>,
    Arc<MemoryAccountsRepo>,
) {
    let orders_repo_impl = Arc::new(MemoryOrdersRepo::new());
    let orders_repo: Arc<dyn OrdersRepo> = orders_repo_impl.clone();
    let accounts_repo = Arc::new(MemoryAccountsRepo::new());
    let account_service = Arc::new(AccountService::new(
        accounts_repo.clone(),
        "USDT".to_string(),
        10_000.0,
    ));
    let matcher = Arc::new(SpotMatcher::new(orders_repo, account_service.clone(), fees));
    (matcher, orders_repo_impl, account_service, accounts_repo)
}

#[tokio::test]
async fn market_buy_fills_on_trade() {
    let (matcher, orders_repo, account_service, _accounts_repo) =
        setup_matcher(default_fee_config()).await;
    let session_id = Uuid::new_v4();
    account_service
        .ensure_session_account(session_id)
        .await
        .unwrap();

    let mut order = base_order(
        session_id,
        "BTCUSDT",
        OrderSide::Buy,
        OrderType::Market,
        None,
        0.01,
        Some(Liquidity::Taker),
    );
    orders_repo.create(order.clone()).await.unwrap();

    let t = trade("BTCUSDT", 1, 60_000.0, 0.5, 1);
    matcher.on_trade(session_id, &t).await;

    order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert!((order.filled_quantity.0 - 0.01).abs() < 1e-9);

    let fills = orders_repo
        .list_order_fills(session_id, order.id)
        .await
        .unwrap();
    assert_eq!(fills.len(), 1);
    let fill = &fills[0];
    assert!(!fill.maker);
    assert!((fill.fee - 0.6).abs() < 1e-6);

    let snapshot = account_service.get_account(session_id).await.unwrap();
    let btc = snapshot
        .balances
        .iter()
        .find(|b| b.asset == "BTC")
        .map(|b| b.free.0)
        .unwrap_or_default();
    let usdt = snapshot
        .balances
        .iter()
        .find(|b| b.asset == "USDT")
        .map(|b| b.free.0)
        .unwrap_or_default();
    assert!((btc - 0.01).abs() < 1e-9);
    assert!((usdt - (10_000.0 - 600.0 - 0.6)).abs() < 1e-6);

    // Ensure trade replay doesn't double fill
    matcher.on_trade(session_id, &t).await;
    let fills_after = orders_repo
        .list_order_fills(session_id, order.id)
        .await
        .unwrap();
    assert_eq!(fills_after.len(), 1);
}

#[tokio::test]
async fn limit_buy_maker_fill() {
    let (matcher, orders_repo, account_service, _) = setup_matcher(default_fee_config()).await;
    let session_id = Uuid::new_v4();
    account_service
        .ensure_session_account(session_id)
        .await
        .unwrap();

    let mut order = base_order(
        session_id,
        "BTCUSDT",
        OrderSide::Buy,
        OrderType::Limit,
        Some(59_000.0),
        0.01,
        None,
    );
    orders_repo.create(order.clone()).await.unwrap();

    // First trade above limit - no fill
    let first_trade = trade("BTCUSDT", 1, 60_000.0, 0.5, 1);
    matcher.on_trade(session_id, &first_trade).await;
    order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::New);

    // Second trade crosses limit
    let second_trade = trade("BTCUSDT", 2, 59_000.0, 0.5, 2);
    matcher.on_trade(session_id, &second_trade).await;
    order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.maker_taker, Some(Liquidity::Maker));

    let fills = orders_repo
        .list_order_fills(session_id, order.id)
        .await
        .unwrap();
    assert_eq!(fills.len(), 1);
    let fill = &fills[0];
    assert!(fill.maker);
    assert!((fill.fee - 0.472).abs() < 1e-6);

    let snapshot = account_service.get_account(session_id).await.unwrap();
    let usdt = snapshot
        .balances
        .iter()
        .find(|b| b.asset == "USDT")
        .map(|b| b.free.0)
        .unwrap_or_default();
    assert!((usdt - (10_000.0 - 590.0 - 0.472)).abs() < 1e-6);
}

#[tokio::test]
async fn limit_buy_taker_immediate() {
    let (matcher, orders_repo, account_service, _) = setup_matcher(default_fee_config()).await;
    let session_id = Uuid::new_v4();
    account_service
        .ensure_session_account(session_id)
        .await
        .unwrap();

    let mut order = base_order(
        session_id,
        "BTCUSDT",
        OrderSide::Buy,
        OrderType::Limit,
        Some(61_000.0),
        0.02,
        Some(Liquidity::Taker),
    );
    orders_repo.create(order.clone()).await.unwrap();

    let t = trade("BTCUSDT", 1, 60_000.0, 1.0, 1);
    matcher.on_trade(session_id, &t).await;
    order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.maker_taker, Some(Liquidity::Taker));

    let fills = orders_repo
        .list_order_fills(session_id, order.id)
        .await
        .unwrap();
    assert_eq!(fills.len(), 1);
    let fill = &fills[0];
    assert!(!fill.maker);
    assert!((fill.fee - 1.2).abs() < 1e-6);
}

#[tokio::test]
async fn limit_sell_partial_fill() {
    let (matcher, orders_repo, account_service, accounts_repo) =
        setup_matcher(default_fee_config()).await;
    let session_id = Uuid::new_v4();
    account_service
        .ensure_session_account(session_id)
        .await
        .unwrap();

    // Give account 1 BTC
    let mut snapshot = account_service.get_account(session_id).await.unwrap();
    snapshot.balances.push(Balance {
        asset: "BTC".to_string(),
        free: Quantity(1.0),
        locked: Quantity::default(),
    });
    accounts_repo.save_account(snapshot).await.unwrap();

    let mut order = base_order(
        session_id,
        "BTCUSDT",
        OrderSide::Sell,
        OrderType::Limit,
        Some(61_000.0),
        1.0,
        None,
    );
    orders_repo.create(order.clone()).await.unwrap();

    for (id, qty) in [(1, 0.4), (2, 0.3), (3, 0.3)] {
        let t = trade("BTCUSDT", id, 61_000.0, qty, id as i64);
        matcher.on_trade(session_id, &t).await;
    }

    order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert_eq!(order.maker_taker, Some(Liquidity::Maker));
    assert!((order.filled_quantity.0 - 1.0).abs() < 1e-9);

    let fills = orders_repo
        .list_order_fills(session_id, order.id)
        .await
        .unwrap();
    assert_eq!(fills.len(), 3);
    let total_fee: f64 = fills.iter().map(|f| f.fee).sum();
    assert!((total_fee - 48.8).abs() < 1e-4);

    let snapshot = account_service.get_account(session_id).await.unwrap();
    let btc = snapshot
        .balances
        .iter()
        .find(|b| b.asset == "BTC")
        .map(|b| b.free.0)
        .unwrap_or_default();
    let usdt = snapshot
        .balances
        .iter()
        .find(|b| b.asset == "USDT")
        .map(|b| b.free.0)
        .unwrap_or_default();
    assert!(btc.abs() < 1e-9);
    assert!((usdt - (10_000.0 + 61_000.0 - 48.8)).abs() < 1e-4);
}

#[tokio::test]
async fn limit_orders_expire_on_session_end() {
    let (matcher, orders_repo, account_service, _) = setup_matcher(default_fee_config()).await;
    let session_id = Uuid::new_v4();
    account_service
        .ensure_session_account(session_id)
        .await
        .unwrap();

    let order = base_order(
        session_id,
        "BTCUSDT",
        OrderSide::Buy,
        OrderType::Limit,
        Some(50_000.0),
        0.5,
        None,
    );
    orders_repo.create(order.clone()).await.unwrap();

    matcher.on_session_end(session_id).await;

    let order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Expired);
}

#[tokio::test]
async fn partial_fills_disabled_fill_full_quantity() {
    let fees = FeeConfig {
        maker_bps: 8,
        taker_bps: 10,
        partial_fills: false,
    };
    let (matcher, orders_repo, account_service, _) = setup_matcher(fees).await;
    let session_id = Uuid::new_v4();
    account_service
        .ensure_session_account(session_id)
        .await
        .unwrap();

    let mut order = base_order(
        session_id,
        "BTCUSDT",
        OrderSide::Buy,
        OrderType::Market,
        None,
        0.5,
        Some(Liquidity::Taker),
    );
    orders_repo.create(order.clone()).await.unwrap();

    let t = trade("BTCUSDT", 1, 20_000.0, 0.1, 1);
    matcher.on_trade(session_id, &t).await;

    order = orders_repo.get(session_id, order.id).await.unwrap();
    assert_eq!(order.status, OrderStatus::Filled);
    assert!((order.filled_quantity.0 - 0.5).abs() < 1e-9);

    let fills = orders_repo
        .list_order_fills(session_id, order.id)
        .await
        .unwrap();
    assert_eq!(fills.len(), 1);
    assert!((fills[0].qty.0 - 0.5).abs() < 1e-9);
}
