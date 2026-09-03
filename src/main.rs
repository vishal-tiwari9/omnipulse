use base64::{engine::general_purpose::STANDARD, Engine};
use hmac::{Hmac,Mac,KeyInit};
use sha2::Sha256;
use std::time::{SystemTime,UNIX_EPOCH};
use dotenvy::dotenv;
use std::env;
use reqwest::header::{HeaderMap,HeaderValue};


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
    Ok(())


}



