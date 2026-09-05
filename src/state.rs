use crate::models::MarketEvent;

/// All runtime state needed by the strategy engine
#[derive(Debug, Default)]
pub struct MarketState {
    pub active_market_id:    Option<i64>,
    pub active_round_number: Option<String>,
    pub market_status:       String,       // "trading" | "frozen" | "settled"

    // Oracle (raw 10^-8 USD integer units, same as strike)
    pub current_btc_price:   f64,
    pub previous_btc_price:  Option<f64>,

    // Round math (populated from WS open event + computed freeze_ts)
    pub strike_raw: Option<f64>, // raw 10^-8 USD, same units as price
    pub open_ts:    Option<u64>, // nanoseconds epoch
    pub freeze_ts:  Option<u64>, // nanoseconds epoch (open_ts + 60s if not from server)
}

impl MarketState {
    pub fn new() -> Self { Self::default() }

    pub fn process_event(&mut self, event: MarketEvent) {
        match event {
            MarketEvent::OracleUpdate { price } => {
                if self.current_btc_price > 0.0 {
                    self.previous_btc_price = Some(self.current_btc_price);
                }
                self.current_btc_price = price;
            }

            MarketEvent::RoundUpdate { market_id, round_number, status, strike_raw, freeze_ts, open_ts } => {
                match status.as_str() {
                    "trading" => {
                        self.active_market_id    = Some(market_id);
                        self.active_round_number = Some(round_number);
                        self.market_status       = "trading".to_string();
                        if let Some(s) = strike_raw { self.strike_raw = Some(s); }
                        if let Some(o) = open_ts    { self.open_ts    = Some(o); }
                        if let Some(f) = freeze_ts  { 
                            self.freeze_ts = Some(f);
                        } else if let Some(o) = self.open_ts {
                            // Derive freeze_ts: open_ts + 60 seconds (in nanoseconds)
                            self.freeze_ts = Some(o + 60_000_000_000u64);
                        }
                    }
                    "frozen" => {
                        self.market_status = "frozen".to_string();
                    }
                    "settled" => {
                        if self.active_market_id == Some(market_id) {
                            self.active_market_id    = None;
                            self.active_round_number = None;
                            self.strike_raw          = None;
                            self.freeze_ts           = None;
                            self.open_ts             = None;
                        }
                        self.market_status = "settled".to_string();
                    }
                    _ => {}
                }
            }
        }
    }
}