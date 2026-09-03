use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use serde_json::json;

pub async fn start_websocket() {
    let url = "wss://api.omnibook.xyz/v1/ws"; 
    
    println!("Connecting to Omnibook Live Data...");
    
    
    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect to WS");
    println!("Connected successfully!");
    
    let (mut sender, mut receiver) = ws_stream.split();
    
    
    let sub_msg = json!({
        "action": "subscribe",
        "channels": ["oracle", "rounds"]
    });
    
    sender.send(Message::Text(sub_msg.to_string())).await.expect("Failed to subscribe");
    println!("Subscribed! Waiting for data...");

    
    while let Some(msg) = receiver.next().await {
        if let Ok(msg) = msg {
            if msg.is_text() {
                let text = msg.into_text().unwrap();
                // Abhi ke liye bas print kar rahe hain
                println!("Live Data: {}", text);
            }
        }
    }
}