use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac,Mac,KeyInit};
use sha2::Sha256;
use std::time::{SystemTime,UNIX_EPOCH};
use dotenvy::dotenv;
use std::env;
mod ws_client;
use reqwest::header::{HeaderMap,HeaderValue};

pub mod models;
pub mod state;


fn sign_request(method:&str,target:&str,body:&str,secret_hex:&str)->(String,String){

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
        .to_string();


    let canonical = format!("{}\n{}\n{}\n{}",ts,method,target,body);

    let secret_bytes=hex::decode(secret_hex).expect("Invalid Hex Secret!");

    let mut mac =Hmac::<Sha256>::new_from_slice(&secret_bytes).expect("HMAC error");
    mac.update(canonical.as_bytes());
    let result= mac.finalize().into_bytes();


    let signature =STANDARD.encode(result);
    (ts,signature)
}



#[tokio::main]
async fn main()-> Result<(),Box<dyn std::error::Error>>{
    dotenv().ok();

    let api_key= env::var("OMNIBOOK_API_KEY").expect("OMNIBOOK_API_KEY not set in .env");
    let api_secret= env::var("OMNIBOOK_API_SECRET").expect("OMNIBOOK_API_SECRET not set in .env");
  let api_key_clone=api_key.clone();

    let method = "GET";
    let target ="/v1/account/limits";
    let body="";


    let(ts,sig)= sign_request(method,target,body,&api_secret);

    let mut headers = HeaderMap::new();
    headers.insert("DX-ACCESS-KEY",HeaderValue::from_str(&api_key)?);
    headers.insert("DX-ACCESS-TIMESTAMP",HeaderValue::from_str(&ts)?);
    headers.insert("DX-ACCESS-SIGNATURE",HeaderValue::from_str(&sig)?);

    println!("Sending Request");

    let client = reqwest::Client::new();
    let res=client
    .request(reqwest::Method::GET,format!("https://api.omnibook.xyz{}",target))
    .headers(headers)
    .send()
    .await?;


    println!("Status:{}",res.status());
    let text = res.text().await?;
    println!("Response:{}",text);
println!("TEsting websocket");
  
        // ---------------------------------------------------------
    // 3. MPSC PIPE AUR STATE MANAGER
    // ---------------------------------------------------------
    // Ek Pipe banate hain jo 10,000 messages hold kar sakti hai
    let (tx, mut rx) = tokio::sync::mpsc::channel::<models::MarketEvent>(10000);
    
    // Apni Memory Cabinet banate hain
    let mut bot_state = state::MarketState::new();

    // Receiver Thread (Consumer): Yeh hamesha pipe se data nikalega aur State update karega
    tokio::spawn(async move {
        println!("Brain is now listening to the pipe...");
        while let Some(event) = rx.recv().await {
            // Memory update karo
            bot_state.process_event(event);
            
            // Abhi ke liye bas print karke dekhte hain ki state theek hai ya nahi
            println!("Current Memory State: BTC Price = {}, Market ID = {:?}", 
                     bot_state.current_btc_price, bot_state.active_market_id);
        }
    });

    // Sender Thread (Producer): Yeh pipe (tx) ko ws_client mein bhejega
    println!("Starting WebSocket...");
    tokio::spawn(async move {
        // Hum ws_client ko apna pipe 'tx' pakda denge taaki wo usme events daal sake
        // Iske liye humein ws_client.rs ko thoda modify karna hoga next step mein!

        let(ws_ts,ws_sig)=sign_request("GET","/v1/ws","",&api_secret);

        ws_client::start_websocket(tx, &api_key_clone, &ws_ts, &ws_sig).await;
    });

    // Program ko zinda rakhne ke liye infinite wait
    std::future::pending::<()>().await;
   
    Ok(())


}



