use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use serde_json::json;
use crate::models::MarketEvent; 

// YAHAN DEKHO: Humne pehla argument 'tx' add kiya hai!
pub async fn start_websocket(
    tx: tokio::sync::mpsc::Sender<MarketEvent>, 
    api_key: &str, 
    ts: &str, 
    sig: &str
) {
    let url = "wss://api.omnibook.xyz/v1/ws"; 
    
    let mut request = url.into_client_request().expect("Bad WS URL");
    request.headers_mut().insert("DX-ACCESS-KEY", api_key.parse().unwrap());
    request.headers_mut().insert("DX-ACCESS-TIMESTAMP", ts.parse().unwrap());
    request.headers_mut().insert("DX-ACCESS-SIGNATURE", sig.parse().unwrap());
    
    let (ws_stream, _) = connect_async(request).await.expect("Failed to connect to WS");
    println!("WebSocket Connected successfully!");
    
    let (mut sender, mut receiver) = ws_stream.split();
    
    // Oracle aur Rounds subscribe kar rahe hain
    let sub_msg = json!({
        "id": 1,
        "cmd": "subscribe",
        "params": {
            "channels": ["oracle", "rounds"]
        }
    });
    
    sender.send(Message::Text(sub_msg.to_string())).await.expect("Failed to subscribe");
    println!("Subscribed! Waiting for live events...");

    // THE SENIOR ENGINEER'S EVENT LOOP
    while let Some(msg_result) = receiver.next().await {
        
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                println!("⚠️ WebSocket Error: {:?}", e);
                continue; 
            }
        };

        if msg.is_text() {
            let text = msg.into_text().unwrap_or_default();
            
            if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                // Channel check
                if parsed["type"] == "oracle" || parsed["channel"] == "oracle" {
                    if let Some(price_str) = parsed["msg"]["price"].as_str() {
                        if let Ok(price) = price_str.parse::<f64>() {
                            // Pipe mein safely bhej rahe hain
                            if let Err(e) = tx.try_send(MarketEvent::OracleUpdate { price }) {
                                println!("⚠️ Pipe is full or broken! Dropping price update: {:?}", e);
                            }
                        }
                    }
                } 
                else if parsed["type"] == "rounds" || parsed["channel"] == "rounds" {
                    if let (Some(id), Some(status)) = (
                        parsed["msg"]["id"].as_i64(), 
                        parsed["msg"]["status"].as_str()
                    ) {
                        if let Err(e) = tx.try_send(MarketEvent::RoundUpdate { 
                            market_id: id, 
                            status: status.to_string() 
                        }) {
                            println!("⚠️ Failed to send RoundUpdate to pipe: {:?}", e);
                        }
                    }
                }
            }
        }
    }
}