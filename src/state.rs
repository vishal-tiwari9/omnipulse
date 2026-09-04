use crate::models::{MarketEvent,DesiredOrder};

#[derive(Debug,Default)]

pub struct MarketState{
    pub active_market_id:Option<i64>,
    pub current_btc_price:f64,
    pub available_balance:i64,
    pub inventory_yes:i64,
    pub market_status:String,
}

impl MarketState {
    pub fn new()->Self{
        Self::default()

    }

    pub fn process_event(&mut self , event:MarketEvent){
        match event {
            MarketEvent::OracleUpdate{price}=>{
                self.current_btc_price=price;

            }

            MarketEvent::RoundUpdate{
                market_id,status
            } =>{
                self.active_market_id=Some(market_id);
                self.market_status=status;
                println!("Round Status Changed! ID :{},Status:{}",market_id,self.market_status);
            }
            MarketEvent::UserInventoryUpdate{
                available_balance,current_positions
            }=> {
                self.available_balance=available_balance;
            }
        }
    }
}