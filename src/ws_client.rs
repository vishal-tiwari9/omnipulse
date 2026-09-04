use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use serde_json::json;
use crate:models::MarketEvent;


pub async fn start_websocket(api_key: &str, ts: &str, sig: &str) {
    let url = "wss://api.omnibook.xyz/v1/ws"; 
    
    println!("Connecting to Omnibook Live Data with Headers...");
    
    
    let mut request = url.into_client_request().expect("Bad WS URL");
    request.headers_mut().insert("DX-ACCESS-KEY", api_key.parse().unwrap());
    request.headers_mut().insert("DX-ACCESS-TIMESTAMP", ts.parse().unwrap());
    request.headers_mut().insert("DX-ACCESS-SIGNATURE", sig.parse().unwrap());
    
    
    let (ws_stream, _) = connect_async(request).await.expect("Failed to connect to WS");
    println!("WebSocket Connected successfully!");
    
    let (mut sender, mut receiver) = ws_stream.split();
    
    
    let sub_msg = json!({
        "id": 1,
        "cmd": "subscribe",
        "params": {
            "channels": ["oracle","rounds"]
        }
    });
    
    sender.send(Message::Text(sub_msg.to_string())).await.expect("Failed to subscribe");


    println!("Subscribed to Private User Data! Waiting for live events...");

    while let Some(msg) = receiver.next().await {
        if let Ok(msg) = msg {
            if msg.is_text() {
                let text;
                let parsed:serde_json::Value= serde_json::from_str(&text).unwrap();
                println!("Live Data: {}", msg.into_text().unwrap());
            }
        }
    }
}