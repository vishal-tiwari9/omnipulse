#[derive(Debug, Clone)]
pub enum MarketEvent {
    /// Oracle price update (raw integer, 10^-8 USD units)
    OracleUpdate { price: f64 },
    /// Round lifecycle event
    RoundUpdate {
        market_id: i64,
        round_number: String,
        status: String,
        strike_raw: Option<f64>,  // raw 10^-8 integer
        freeze_ts: Option<u64>,   // nanoseconds since epoch
        open_ts: Option<u64>,     // nanoseconds since epoch
    },
}

#[derive(Debug, Clone)]
pub struct DesiredOrder {
    pub side:    String,  // "buy"
    pub outcome: String,  // "yes" or "no"
    pub tick:    i64,     // integer ticks 100-9900 (in 10^-4 dollars = cents * 100)
    pub qty:     i64,
}