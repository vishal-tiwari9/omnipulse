use crate::models::DesiredOrder;
use crate::state::MarketState;
use std::time::{SystemTime, UNIX_EPOCH};

// ─── Venue constants ─────────────────────────────────────────────────────────
const TWAP_WINDOW_S: f64 = 10.0; // Pull quotes inside last 10s
const SIGMA:         f64 = 0.00009; // Per-second σ ≈ 9 bps (~50% ann vol at $80k BTC)
const SPREAD_CENTS:  f64 = 2.0;    // Half-spread in cents around fair value
const ORDER_QTY:     i64 = 1;      // Contracts per quote side

// ─── Math ────────────────────────────────────────────────────────────────────

fn n_cdf(x: f64) -> f64 {
    0.5 * (1.0 + libm::erf(x / std::f64::consts::SQRT_2))
}

/// Seconds until freeze. freeze_ts is in nanoseconds since epoch.
pub fn tau_s(freeze_ts: u64) -> f64 {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap()
        .as_nanos() as f64;
    ((freeze_ts as f64 - now_ns) / 1_000_000_000.0).max(0.0)
}

/// P(BTC closes above strike). Both spot and strike are raw 10^-8 USD integers.
fn path_fair(spot: f64, strike: f64, sigma: f64, tau: f64) -> f64 {
    if spot <= 0.0 || strike <= 0.0 || sigma <= 0.0 || tau <= 0.0 {
        return if spot > strike { 1.0 } else if spot < strike { 0.0 } else { 0.5 };
    }
    let d = (spot / strike).ln() / (sigma * tau.sqrt());
    n_cdf(d)
}

/// True if price moved >N sigma in one oracle tick (adverse-selection guard)
fn is_jump(spot: f64, prev: f64, sigma: f64, dt_s: f64, n_sigma: f64) -> bool {
    if prev <= 0.0 || spot <= 0.0 { return false; }
    (spot / prev).ln().abs() > n_sigma * sigma * dt_s.max(1e-9).sqrt()
}

/// Clamp probability to valid tick. API takes cents × 100 (50¢ = 5000).
fn prob_to_tick(p: f64) -> i64 {
    let cents = (p * 100.0).round() as i64;
    cents.clamp(1, 99) * 100
}

// ─── Decision ────────────────────────────────────────────────────────────────

pub enum Decision {
    Quote(Vec<DesiredOrder>),
    Kill,
    Hold,
}

pub fn decide(state: &MarketState) -> Decision {
    // Only trade when market is open
    if state.market_status != "trading" {
        return Decision::Kill;
    }

    let spot = state.current_btc_price;
    if spot <= 0.0 { return Decision::Hold; }

    let freeze_ts = match state.freeze_ts {
        Some(f) if f > 0 => f,
        _ => {
            // No freeze_ts yet — use symmetric 50¢ quote as placeholder
            return quote_around(50.0);
        }
    };

    let tau = tau_s(freeze_ts);

    // ── Jump guard ───────────────────────────────────────────────────────────
    if let Some(prev) = state.previous_btc_price {
        if is_jump(spot, prev, SIGMA, 0.25, 3.5) {
            println!("⚡ JUMP: spot={:.0} prev={:.0} — killing", spot, prev);
            return Decision::Kill;
        }
    }

    // ── TWAP window (last 10s): pull all quotes ───────────────────────────────
    if tau <= TWAP_WINDOW_S {
        if tau > 0.1 {
            println!("⏳ TWAP ({:.1}s left) — pulling", tau);
        }
        return Decision::Kill;
    }

    // ── Fair-value computation ────────────────────────────────────────────────
    let strike = match state.strike_raw {
        Some(s) if s > 0.0 => s,
        _ => return quote_around(50.0), // no strike yet
    };

    let fair_prob = path_fair(spot, strike, SIGMA, tau);
    let fair_cents = fair_prob * 100.0;

    println!(
        "📐 fair={:.1}¢  spot={:.0}  strike={:.0}  τ={:.1}s",
        fair_cents, spot, strike, tau
    );

    quote_around(fair_cents)
}

/// Build dual limit bids around fair value (YES bid + NO bid)
fn quote_around(fair_cents: f64) -> Decision {
    let mut orders = Vec::new();

    // YES BID: buy YES at (fair - spread). YES is worth fair_cents.
    let yes_bid = (fair_cents - SPREAD_CENTS).floor();
    if yes_bid >= 1.0 && yes_bid <= 98.0 {
        orders.push(DesiredOrder {
            side:    "buy".to_string(),
            outcome: "yes".to_string(),
            tick:    prob_to_tick(yes_bid / 100.0),
            qty:     ORDER_QTY,
        });
    }

    // NO BID: buy NO at (100 - fair - spread).
    // This is equivalent to offering to sell YES at (fair + spread).
    let no_bid = (100.0 - fair_cents - SPREAD_CENTS).floor();
    if no_bid >= 1.0 && no_bid <= 98.0 {
        orders.push(DesiredOrder {
            side:    "buy".to_string(),
            outcome: "no".to_string(),
            tick:    prob_to_tick(no_bid / 100.0),
            qty:     ORDER_QTY,
        });
    }

    Decision::Quote(orders)
}