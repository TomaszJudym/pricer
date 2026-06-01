use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
struct Config {
    games: Vec<GameConfig>,
}

#[derive(Deserialize)]
struct GameConfig {
    name: String,
    id: u64,
    threshold_pln: f64,
}

#[derive(Deserialize, Debug)]
struct ApiResponse {
    data: HashMap<String, Option<GameData>>,
}

#[derive(Deserialize, Debug)]
struct GameData {
    url: String,
    prices: Prices,
}

#[derive(Deserialize, Debug)]
struct Prices {
    #[serde(rename = "currentRetail")]
    current_retail: Option<String>,
    #[serde(rename = "currentKeyshops")]
    current_keyshops: Option<String>,
    currency: String,
}

async fn send_telegram(client: &reqwest::Client, token: &str, chat_id: &str, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let url = format!("https://api.telegram.org/bot{token}/sendMessage");
    client.post(&url)
        .json(&serde_json::json!({"chat_id": chat_id, "text": text}))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: Config = ron::from_str(&std::fs::read_to_string("config.ron")?)?;
    let key = std::env::var("GGDEALS_KEY")?;
    let tg_token = std::env::var("TELEGRAM_BOT_TOKEN")?;
    let tg_chat_id = std::env::var("TELEGRAM_ADMIN_ID")?;

    let ids: Vec<String> = config.games.iter().map(|g| g.id.to_string()).collect();
    let thresholds: HashMap<u64, (&str, f64)> = config.games.iter()
        .map(|g| (g.id, (g.name.as_str(), g.threshold_pln)))
        .collect();

    let url = format!(
        "https://api.gg.deals/v1/prices/by-steam-app-id/?ids={}&region=pl&key={key}",
        ids.join(",")
    );

    let client = reqwest::Client::new();
    let resp: ApiResponse = client.get(&url).send().await?.json().await?;

    let mut alerts: Vec<String> = Vec::new();

    for (id_str, game) in &resp.data {
        let id: u64 = id_str.parse()?;
        match game {
            Some(g) => {
                let best_price = [&g.prices.current_retail, &g.prices.current_keyshops]
                    .iter()
                    .filter_map(|p| p.as_deref())
                    .filter_map(|p| p.parse::<f64>().ok())
                    .reduce(f64::min);

                let price_str = best_price.map(|p| format!("{:.2} {}", p, g.prices.currency)).unwrap_or("-".into());
                let (name, threshold) = thresholds[&id];
                println!("[{id}] {name} | lowest: {price_str} | {}", g.url);

                if let Some(price) = best_price {
                    if price < threshold {
                        alerts.push(format!("{name}\n{price_str}\n{}", g.url));
                    }
                }
            }
            None => {
                let name = thresholds.get(&id).map(|(n, _)| *n).unwrap_or("unknown");
                println!("[{id}] {name} not found");
            }
        }
    }

    if !alerts.is_empty() {
        let msg = format!("Price alerts:\n\n{}", alerts.join("\n\n"));
        send_telegram(&client, &tg_token, &tg_chat_id, &msg).await?;
        println!("-> Telegram notification sent ({} game(s))", alerts.len());
    }

    Ok(())
}
