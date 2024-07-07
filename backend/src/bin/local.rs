use anyhow::Result;
use std::{env, fs};

#[tokio::main]
async fn main() -> Result<()> {
    let bedrock_client = receipt_analyzer::create_bedrock_client().await;

    let image_path = env::args().nth(1).expect("missing image argument");
    let image = fs::read(image_path)?;
    let result = receipt_analyzer::analyze(&bedrock_client, &image).await?;
    println!("{result:#?}");

    Ok(())
}
