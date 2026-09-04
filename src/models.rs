#[derive(Debug,Clone)]
pub enum MarketEvent{

    OracleUpdate{ price:f64},
    RoundUpdate{market_id:i64, status:String},
    UserInventoryUpdate{available_balance:i64, current_positions:i64},

}


#[derive(Debug,Clone)]
pub struct DesiredOrder{
    pub side:String, // buy or sell
    pub outcome: String, // yes or no
    pub price:i64,
    pub size:i64,

}