use crate::models::MarketEvent;
use std::time::Duration;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use serde_json::json;

pub async fn start_websocket(
    tx: tokio::sync::mpsc::Sender<MarketEvent>,
    api_key: String,
    api_secret: String,
) {
    let mut retry_delay = Duration::from_secs(1);

    loop {
        println!("🔌 Connecting to Omnibook WebSocket...");
        let (ts, sig) = crate::sign_request("GET", "/v1/ws", "", &api_secret);
        let url = "wss://api.omnibook.xyz/v1/ws";

        let mut request = match url.into_client_request() {
            Ok(r) => r,
            Err(e) => { println!("❌ Bad URL: {:?}", e); break; }
        };

        request.headers_mut().insert("DX-ACCESS-KEY", api_key.parse().unwrap());
        request.headers_mut().insert("DX-ACCESS-TIMESTAMP", ts.parse().unwrap());
        request.headers_mut().insert("DX-ACCESS-SIGNATURE", sig.parse().unwrap());

        let ws_stream = match connect_async(request).await {
            Ok((stream, _)) => {
                println!("✅ WebSocket Connected!");
                retry_delay = Duration::from_secs(1);
                stream
            }
            Err(_e) => {
                println!("⚠️ WS Failed, retrying in {:?}...", retry_delay);
                tokio::time::sleep(retry_delay).await;
                retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
                continue;
            }
        };

        let (mut sender, mut receiver) = ws_stream.split();

        let sub_msg = json!({
            "id": 1,
            "cmd": "subscribe",
            "params": { "channels": ["oracle", "rounds"] }
        });

        if let Err(e) = sender.send(Message::Text(sub_msg.to_string())).await {
            println!("⚠️ Subscribe failed: {:?}", e);
            continue;
        }
        println!("📡 Subscribed to oracle + rounds!");

        while let Some(msg_result) = receiver.next().await {
            let msg = match msg_result {
                Ok(m) => m,
                Err(e) => { println!("⚠️ WS error: {:?}", e); break; }
            };

            if !msg.is_text() { continue; }
            let text = msg.into_text().unwrap_or_default();

            let parsed = match serde_json::from_str::<serde_json::Value>(&text) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let msg_type = parsed["type"].as_str().unwrap_or("");

            if msg_type == "oracle" {
                // Oracle price comes as integer in 10^-8 units (e.g. 7963860000000 = $79638.60)
                let price_raw = parsed["msg"]["price"].as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .or_else(|| parsed["msg"]["price"].as_f64())
                    .or_else(|| parsed["msg"]["median"].as_str().and_then(|s| s.parse::<f64>().ok()))
                    .or_else(|| parsed["msg"]["median"].as_f64())
                    .unwrap_or(0.0);
                
                if price_raw > 0.0 {
                    let _ = tx.try_send(MarketEvent::OracleUpdate { price: price_raw });
                }
                continue;
            }

            if msg_type == "subscribed" { continue; }

            if msg_type == "rounds" {
                let msg_data = &parsed["msg"];
                let action = msg_data["action"].as_str().unwrap_or("unknown");
                let market_id = msg_data["market_id"].as_i64();

                let round_number = msg_data["round_number"].as_str().map(|s| s.to_string())
                    .or_else(|| msg_data["round_number"].as_i64().map(|n| n.to_string()))
                    .unwrap_or_default();

                // strike is a string of integer 10^-8 BTC price
                let strike_raw = msg_data["strike"].as_str()
                    .and_then(|s| s.parse::<f64>().ok())
                    .or_else(|| msg_data["strike"].as_f64());
                
                let freeze_ts = msg_data["freeze_ts"].as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| msg_data["freeze_ts"].as_u64());

                let open_ts = msg_data["open_ts"].as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .or_else(|| msg_data["open_ts"].as_u64());

                if action == "open" {
                    println!("🟢 OPEN! market_id={:?} round={} strike={:?}", market_id, round_number, strike_raw);
                } else if action == "freeze" {
                    println!("🔴 FROZEN! market_id={:?}", market_id);
                } else if action == "settle" {
                    println!("⚫ SETTLED! market_id={:?}", market_id);
                } else {
                    println!("📌 Round: action={} market_id={:?}", action, market_id);
                }

                if let Some(id) = market_id {
                    let status = match action {
                        "open"   => "trading",
                        "freeze" => "frozen",
                        "settle" => "settled",
                        "create" => "creating",
                        _        => action,
                    };

                    let _ = tx.try_send(MarketEvent::RoundUpdate {
                        market_id: id,
                        round_number,
                        status: status.to_string(),
                        strike_raw,
                        freeze_ts,
                        open_ts,
                    });
                }
            }
        }

        println!("🔄 WS disconnected, reconnecting in {:?}...", retry_delay);
        tokio::time::sleep(retry_delay).await;
        retry_delay = (retry_delay * 2).min(Duration::from_secs(30));
    }
}