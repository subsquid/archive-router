use base64::{engine::general_purpose, Engine as _};
use ethers::prelude::*;
use reqwest::{multipart, Client};
use serde::Deserialize;

#[derive(Deserialize)]
#[allow(non_snake_case)]
struct IpfsCreateResponse {
    Hash: String,
}

pub async fn write_to_ipfs(client: Client, file: String) -> String {
    let auth_key = get_auth_key().await;
    let form = multipart::Form::new().text("file", file);
    let res = match client
        .post("https://crustipfs.xyz/api/v0/add")
        .multipart(form)
        .bearer_auth(auth_key)
        .send()
        .await
    {
        Ok(res) => res,
        Err(e) => panic!("error sending request: {}", e),
    };
    match res.json::<IpfsCreateResponse>().await {
        Ok(res) => res.Hash,
        Err(e) => panic!("error parsing response: {}", e),
    }
}

async fn get_auth_key() -> String {
    let wallet = Wallet::new(&mut rand::thread_rng());
    let address = format!("{:#?}", wallet.address());
    let sig = match wallet.sign_message(&address).await {
        Ok(sig) => sig,
        Err(e) => panic!("error signing message: {}", e),
    };
    let plain_auth_key = format!("eth-{address}:{sig}");
    return general_purpose::STANDARD.encode(plain_auth_key.as_bytes());
}
