use anyhow::Result;
use aws_config::Region;
use aws_sdk_bedrockruntime::primitives::Blob;
use base64::{engine::general_purpose, Engine as _};
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::Cursor;

#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct Receipt {
    pub brand: String,
    pub store: String,
    pub date: String,
    pub items: Vec<ReceiptItem>,
    pub total: String,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct ReceiptItem {
    pub name: String,
    pub price: String,
}

pub async fn resize_image(image_data: &[u8], max_size_bytes: usize) -> Result<Vec<u8>> {
    let image = image::load_from_memory(&image_data)?;
    let mut current_size = image_data.len();
    let mut width = image.width();
    let mut height = image.height();

    while current_size > max_size_bytes && width > 100 && height > 100 {
        width = (width as f32 * 0.9) as u32;
        height = (height as f32 * 0.9) as u32;

        let resized_image = image.resize(width, height, image::imageops::FilterType::Lanczos3);
        let mut cursor = Cursor::new(Vec::new());
        resized_image.write_to(&mut cursor, ImageFormat::Jpeg)?;
        let resized_image_data = cursor.into_inner();
        current_size = resized_image_data.len();

        if current_size <= max_size_bytes {
            return Ok(resized_image_data);
        }

        eprintln!("Resized image size: {} bytes", current_size);
    }

    Err(anyhow::anyhow!("Failed to resize image to target size"))
}

pub async fn analyze(client: &aws_sdk_bedrockruntime::Client, image: &[u8]) -> Result<Receipt> {
    let image = resize_image(image, 1024 * 1024).await?;
    let base64_image = general_purpose::STANDARD.encode(image);

    let example_format = json!({
        "brand": "{店の名前}",
        "store": "{支店名}",
        "date": "{日付 YYYY-MM-DD}",
        "items": [
            {
                "name": "{商品名1}",
                "price": "{価格1}"
            },
            {
                "name": "{商品名2}",
                "price": "{価格2}"
            },
            {
                "name": "外税",
                "price": "{外税の項目があれば、アイテムの総和が税込み合計金額と一致するように調整用の外税の合計金額を入れる。なければ外税の項目自体を削除してよい}"
            }
        ],
        "total": "{税込み合計金額、アイテムから計算せず、レシートに書いてあるとおりを入れる}",
        "confidence": "{どれくらい正確に読み取れたと判断できるか、0.0 から 1.0 の間の数値を入れる}"
    });

    let bedrock_request = json!({
        "anthropic_version": "bedrock-2023-05-31",
        "max_tokens": 1000,
        "messages": [
            {
                "role": "user",
                "content": [
                    {
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": "image/jpeg",
                            "data": base64_image
                        }
                    },
                    {
                        "type": "text",
                        "text": format!(
                            concat!(
                                "画像のレシートを解析し、店名、日付、購入品目と金額、合計金額を抽出してください。\n",
                                "\n",
                                "# 注意\n",
                                "\n",
                                "- レシートではなくクレジットカード売上票や請求書の場合もあります。この場合は内訳は書いていないので、「不明」というアイテムが一つのレシートとみなしてください。\n",
                                "- 円記号を目印にして認識してください。\n",
                                "- 商品は1つしかない場合もあれば複数個ある場合もあります。\n",
                                "- レシートに書いてある合計金額を最も優先し、計算が合わない場合は商品の項目で調整してください。\n",
                                "- 不明な箇所には `不明` と出力してください。\n",
                                "\n",
                                "# 出力フォーマット\n",
                                "\n",
                                "- 以下の形式の JSON フォーマットを返してください。\n",
                                "- 説明は一切不要なので結果のみを返してください。\n",
                                "- Markdown のマーカーブロックでは囲まないでください。\n",
                                "\n",
                                "```json\n",
                                "{example_format}\n",
                                "```\n",
                            ),
                            example_format=example_format
                        ),
                    }
                ]
            }
        ]
    });

    let response = client
        .invoke_model()
        .model_id("anthropic.claude-3-5-sonnet-20240620-v1:0")
        .content_type("application/json")
        .accept("application/json")
        .body(Blob::new(serde_json::to_string(&bedrock_request)?))
        .send()
        .await?;

    let response_body: serde_json::Value = serde_json::from_slice(response.body().as_ref())?;
    let analysis_result = response_body["content"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Failed to extract analysis result"))?;

    let usage = &response_body["usage"];
    eprintln!("usage: {usage}");

    serde_json::from_str(analysis_result).map_err(Into::into)
}

pub async fn create_bedrock_client() -> aws_sdk_bedrockruntime::Client {
    let config = aws_config::load_defaults(aws_config::BehaviorVersion::v2024_03_28())
        .await
        .into_builder()
        .region(Region::new("us-east-1"))
        .build();

    aws_sdk_bedrockruntime::Client::new(&config)
}
