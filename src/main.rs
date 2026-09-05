use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac, Mac, KeyInit};
use sha2::Sha256;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use dotenvy::dotenv;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

mod ws_client;
pub mod models;
pub mod state;
pub mod strategy;

// ─── Auth ─────────────────────────────────────────────────────────────────────

pub fn sign_request(method: &str, target: &str, body: &str, secret_hex: &str) -> (String, String) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH).unwrap()
        .as_millis().to_string();
    let canonical = format!("{}\n{}\n{}\n{}", ts, method, target, body);
    let secret_bytes = hex::decode(secret_hex).expect("Invalid hex secret");
    let mut mac = Hmac::<Sha256>::new_from_slice(&secret_bytes).expect("HMAC error");
    mac.update(canonical.as_bytes());
    (ts, STANDARD.encode(mac.finalize().into_bytes()))
}

// ─── REST: fetch active trading round (has strike + open_ts) ──────────────────

async fn fetch_active_round(
    client: &reqwest::Client,
    api_key: &str,
    api_secret: &str,
) -> Option<(i64, String, f64, u64)> {
    // Returns (market_id, round_number, strike_raw, open_ts_ns)
    let target = "/v1/rounds";
    let (ts, sig) = sign_request("GET", target, "", api_secret);
    let res = client
        .get(format!("https://api.omnibook.xyz{}", target))
        .header("DX-ACCESS-KEY", api_key)
        .header("DX-ACCESS-TIMESTAMP", &ts)
        .header("DX-ACCESS-SIGNATURE", &sig)
        .send().await.ok()?;

    let v: serde_json::Value = res.json().await.ok()?;
    let rounds = v["rounds"].as_array()?;
    let round = rounds.iter().find(|r| r["status"].as_str() == Some("trading"))?;

    let market_id  = round["market_id"].as_i64()?;
    let round_num  = round["round_number"].as_str()
        .map(|s| s.to_string())
        .or_else(|| round["round_number"].as_i64().map(|n| n.to_string()))
        .unwrap_or_default();
    let strike_raw = round["strike"].as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .or_else(|| round["strike"].as_f64())?;
    let open_ts = round["open_ts"].as_str()
        .and_then(|s| s.parse::<u64>().ok())
        .or_else(|| round["open_ts"].as_u64())
        .unwrap_or(0);

    Some((market_id, round_num, strike_raw, open_ts))
}

// ─── Main ────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv().ok();

    let api_key    = env::var("OMNIBOOK_API_KEY").expect("OMNIBOOK_API_KEY not set").trim().to_string();
    let api_secret = env::var("OMNIBOOK_API_SECRET").expect("OMNIBOOK_API_SECRET not set").trim().to_string();

    println!("🔑 API Key: {}...", &api_key[..8.min(api_key.len())]);
    println!("🔐 Secret length: {} chars", api_secret.len());

    let http = reqwest::Client::new();

    // Auth check
    let (ts, sig) = sign_request("GET", "/v1/account/limits", "", &api_secret);
    let auth = http.get("https://api.omnibook.xyz/v1/account/limits")
        .header("DX-ACCESS-KEY", &api_key)
        .header("DX-ACCESS-TIMESTAMP", &ts)
        .header("DX-ACCESS-SIGNATURE", &sig)
        .send().await?;
    println!("Auth check: {}", auth.status());

    // Bootstrap initial state from REST
    let initial = fetch_active_round(&http, &api_key, &api_secret).await;

    // Use a SMALL channel — if Brain can't keep up, drop oracle ticks, not accumulate them
    let (tx, mut rx) = tokio::sync::mpsc::channel::<models::MarketEvent>(256);
    let mut bot_state = state::MarketState::new();

    if let Some((mid, rn, strike, open_ts)) = initial {
        let freeze_ts = open_ts + 60_000_000_000u64; // open_ts + 60s
        bot_state.active_market_id    = Some(mid);
        bot_state.active_round_number = Some(rn.clone());
        bot_state.market_status       = "trading".to_string();
        bot_state.strike_raw          = Some(strike);
        bot_state.open_ts             = Some(open_ts);
        bot_state.freeze_ts           = Some(freeze_ts);
        println!("🚀 Booting: market_id={} round={} strike={:.0} tau_left={:.1}s",
            mid, rn, strike, strategy::tau_s(freeze_ts));
    }

    // ── REST Poller: every 5s refresh active round data ──────────────────────
    let tx_poll  = tx.clone();
    let key_poll = api_key.clone();
    let sec_poll = api_secret.clone();
    tokio::spawn(async move {
        let poll_http = reqwest::Client::new();
        let mut last_id: Option<i64> = None;
        loop {
            tokio::time::sleep(Duration::from_secs(5)).await;
            if let Some((mid, rn, strike, open_ts)) = fetch_active_round(&poll_http, &key_poll, &sec_poll).await {
                let freeze_ts = open_ts + 60_000_000_000u64;
                if Some(mid) != last_id {
                    println!("🔄 Poller: market_id={} round={} strike={:.0}", mid, rn, strike);
                    last_id = Some(mid);
                }
                // Always push to refresh freeze_ts
                let _ = tx_poll.try_send(models::MarketEvent::RoundUpdate {
                    market_id:   mid,
                    round_number: rn,
                    status:      "trading".to_string(),
                    strike_raw:  Some(strike),
                    freeze_ts:   Some(freeze_ts),
                    open_ts:     Some(open_ts),
                });
            }
        }
    });

    // ── Brain: event loop ─────────────────────────────────────────────────────
    let api_key_brain    = api_key.clone();
    let api_secret_brain = api_secret.clone();
    let order_counter = Arc::new(AtomicU64::new(
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis() as u64
    ));

    tokio::spawn(async move {
        println!("🧠 Brain online!");
        let order_http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build().unwrap();

        let mut live_orders: Vec<String> = Vec::new();
        let mut last_quoted_market: Option<i64> = None;
        let mut last_quoted_ticks: Option<(i64, i64)> = None;
        let mut last_log_time = SystemTime::now();

        while let Some(event) = rx.recv().await {
            // ── Detect market change ─────────────────────────────────────────
            let prev_market = bot_state.active_market_id;

            bot_state.process_event(event);

            let curr_market = bot_state.active_market_id;

            // If market changed, flush live_orders (old market's orders are already gone)
            if curr_market != prev_market {
                live_orders.clear();
                last_quoted_ticks = None;
                last_quoted_market = None;
            }

            let spot = bot_state.current_btc_price;
            if spot <= 0.0 { continue; }

            let market_id = match curr_market {
                Some(id) => id,
                None => { 
                    if !live_orders.is_empty() {
                        cancel_batch(&order_http, &api_key_brain, &api_secret_brain, &live_orders).await;
                        live_orders.clear();
                        last_quoted_ticks = None;
                    }
                    continue;
                }
            };

            if bot_state.market_status != "trading" {
                if !live_orders.is_empty() {
                    cancel_batch(&order_http, &api_key_brain, &api_secret_brain, &live_orders).await;
                    live_orders.clear();
                    last_quoted_ticks = None;
                }
                continue;
            }

            // ── Strategy ──────────────────────────────────────────────────────
            match strategy::decide(&bot_state) {
                strategy::Decision::Hold => {}

                strategy::Decision::Kill => {
                    if !live_orders.is_empty() {
                        cancel_batch(&order_http, &api_key_brain, &api_secret_brain, &live_orders).await;
                        live_orders.clear();
                        last_quoted_ticks = None;
                    }
                    last_quoted_market = Some(market_id);
                }

                strategy::Decision::Quote(desired) => {
                    let mut current_yes = 0;
                    let mut current_no = 0;
                    for o in &desired {
                        if o.outcome == "yes" { current_yes = o.tick; }
                        if o.outcome == "no"  { current_no = o.tick;  }
                    }

                    let is_new_market = Some(market_id) != last_quoted_market;
                    let ticks_changed = Some((current_yes, current_no)) != last_quoted_ticks;
                    
                    // Throttle logging to every 5s if ticks haven't changed
                    if !ticks_changed && !is_new_market {
                        if last_log_time.elapsed().unwrap_or(Duration::from_secs(10)).as_secs() > 5 {
                            // Optionally log a heartbeat
                            last_log_time = SystemTime::now();
                        }
                        continue;
                    }

                    // Cancel old quotes
                    if !live_orders.is_empty() {
                        cancel_batch(&order_http, &api_key_brain, &api_secret_brain, &live_orders).await;
                        live_orders.clear();
                    }

                    // Place new quotes
                    for o in desired {
                        let coid = order_counter.fetch_add(1, Ordering::SeqCst).to_string();
                        let body = serde_json::json!({
                            "client_order_id": coid,
                            "market_id":  market_id,
                            "side":       o.side,
                            "outcome":    o.outcome,
                            "type":       "limit",
                            "tif":        "gtc",
                            "qty":        o.qty,
                            "tick":       o.tick,
                            "post_only":  true
                        }).to_string();

                        let target = "/v1/portfolio/orders";
                        let (ts, sig) = crate::sign_request("POST", target, &body, &api_secret_brain);

                        match order_http
                            .post(format!("https://api.omnibook.xyz{}", target))
                            .header("DX-ACCESS-KEY", &api_key_brain)
                            .header("DX-ACCESS-TIMESTAMP", &ts)
                            .header("DX-ACCESS-SIGNATURE", &sig)
                            .header("Content-Type", "application/json")
                            .body(body)
                            .send().await
                        {
                            Ok(res) => {
                                let status = res.status();
                                let rb = res.text().await.unwrap_or_default();
                                if status.is_success() {
                                    if let Ok(j) = serde_json::from_str::<serde_json::Value>(&rb) {
                                        if let Some(oid) = j["order_id"].as_str() {
                                            live_orders.push(oid.to_string());
                                            println!("✅ QUOTE market={} {} {} @{}¢ oid={}",
                                                market_id, o.outcome, o.side, o.tick / 100, oid);
                                        }
                                    }
                                } else {
                                    println!("❌ FAIL {} {}", status, rb);
                                }
                            }
                            Err(e) => println!("⚠️ net error: {:?}", e),
                        }
                    }

                    last_quoted_ticks   = Some((current_yes, current_no));
                    last_quoted_market  = Some(market_id);
                    last_log_time       = SystemTime::now();
                }
            }
        }
    });

    // ── WebSocket: real-time oracle + round events ─────────────────────────────
    tokio::spawn(async move {
        ws_client::start_websocket(tx, api_key, api_secret).await;
    });

    std::future::pending::<()>().await;
    Ok(())
}

// ─── Cancel batch helper ──────────────────────────────────────────────────────

async fn cancel_batch(
    client: &reqwest::Client,
    api_key: &str,
    api_secret: &str,
    order_ids: &[String],
) {
    if order_ids.is_empty() { return; }
    let body = serde_json::json!({ "order_ids": order_ids }).to_string();
    let target = "/v1/portfolio/orders/batch";
    let (ts, sig) = sign_request("DELETE", target, &body, api_secret);
    let _ = client
        .delete(format!("https://api.omnibook.xyz{}", target))
        .header("DX-ACCESS-KEY", api_key)
        .header("DX-ACCESS-TIMESTAMP", &ts)
        .header("DX-ACCESS-SIGNATURE", &sig)
        .header("Content-Type", "application/json")
        .body(body)
        .send().await;
}