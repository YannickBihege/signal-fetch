use anyhow::{Context, Result};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "signal-fetch")]
#[command(about = "Fetch price + RSI + gold-denominated values for The Signal")]
struct Cli {
    /// Output JSON instead of a pretty table
    #[arg(long)]
    json: bool,

    /// Comma-separated tickers to fetch (overrides defaults)
    #[arg(long, value_delimiter = ',')]
    tickers: Option<Vec<String>>,
}

// ── Config ───────────────────────────────────────────────────────────────────

const DEFAULT_TICKERS: &[(&str, &str)] = &[
    ("TSM",  "TSMC"),
    ("NVDA", "NVIDIA"),
    ("AMD",  "AMD"),
    ("ASML", "ASML"),
    ("OKLO", "OKLO"),
];

// Gold spot via the physical gold ETF proxy on Alpha Vantage
// XAUUSD is available as a forex pair on Alpha Vantage
const GOLD_SYMBOL: &str = "XAUUSD";

// ── Alpha Vantage response types ─────────────────────────────────────────────

#[derive(Deserialize)]
struct GlobalQuote {
    #[serde(rename = "Global Quote")]
    global_quote: QuoteData,
}

#[derive(Deserialize)]
struct QuoteData {
    #[serde(rename = "05. price")]
    price: String,
    #[serde(rename = "02. open")]
    open: String,
    #[serde(rename = "09. change")]
    change: String,
    #[serde(rename = "10. change percent")]
    change_percent: String,
}

#[derive(Deserialize)]
struct RsiResponse {
    #[serde(rename = "Technical Analysis: RSI")]
    technical_analysis: HashMap<String, RsiPoint>,
}

#[derive(Deserialize)]
struct RsiPoint {
    #[serde(rename = "RSI")]
    rsi: String,
}

#[derive(Deserialize)]
struct ForexResponse {
    #[serde(rename = "Realtime Currency Exchange Rate")]
    exchange_rate: ForexData,
}

#[derive(Deserialize)]
struct ForexData {
    #[serde(rename = "5. Exchange Rate")]
    rate: String,
}

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct PositionOutput {
    ticker: String,
    name: String,
    price_usd: f64,
    price_eur: f64,
    oz_of_gold: f64,
    rsi_14: f64,
    zone: String,
    change_percent: String,
}

#[derive(Serialize)]
struct SignalSnapshot {
    timestamp: String,
    gold_spot_usd: f64,
    gold_spot_eur: f64,
    eurusd_rate: f64,
    positions: Vec<PositionOutput>,
}

// ── Alpha Vantage client ──────────────────────────────────────────────────────

struct AlphaVantage {
    api_key: String,
    client: reqwest::blocking::Client,
}

impl AlphaVantage {
    fn new(api_key: String) -> Self {
        Self {
            api_key,
            client: reqwest::blocking::Client::new(),
        }
    }

    fn base_url(&self) -> &str {
        "https://www.alphavantage.co/query"
    }

    fn fetch_price(&self, symbol: &str) -> Result<(f64, String)> {
        let url = format!(
            "{}?function=GLOBAL_QUOTE&symbol={}&apikey={}",
            self.base_url(),
            symbol,
            self.api_key
        );
        let resp: GlobalQuote = self
            .client
            .get(&url)
            .send()
            .context("HTTP request failed")?
            .json()
            .context("Failed to parse price response")?;

        let price = resp
            .global_quote
            .price
            .parse::<f64>()
            .context("Failed to parse price")?;
        let change_pct = resp.global_quote.change_percent.trim_end_matches('%').to_string();

        Ok((price, change_pct))
    }

    fn fetch_rsi(&self, symbol: &str) -> Result<f64> {
        let url = format!(
            "{}?function=RSI&symbol={}&interval=daily&time_period=14&series_type=close&apikey={}",
            self.base_url(),
            symbol,
            self.api_key
        );
        let resp: RsiResponse = self
            .client
            .get(&url)
            .send()
            .context("HTTP request failed")?
            .json()
            .context("Failed to parse RSI response")?;

        // Get the most recent RSI value (first key when sorted descending)
        let rsi = resp
            .technical_analysis
            .iter()
            .max_by_key(|(date, _)| date.clone())
            .and_then(|(_, point)| point.rsi.parse::<f64>().ok())
            .context("No RSI data found")?;

        Ok(rsi)
    }

    fn fetch_gold_usd(&self) -> Result<f64> {
        // Alpha Vantage forex endpoint for XAU/USD
        let url = format!(
            "{}?function=CURRENCY_EXCHANGE_RATE&from_currency=XAU&to_currency=USD&apikey={}",
            self.base_url(),
            self.api_key
        );
        let resp: ForexResponse = self
            .client
            .get(&url)
            .send()
            .context("HTTP request failed")?
            .json()
            .context("Failed to parse gold response")?;

        resp.exchange_rate
            .rate
            .parse::<f64>()
            .context("Failed to parse gold rate")
    }

    fn fetch_eurusd(&self) -> Result<f64> {
        let url = format!(
            "{}?function=CURRENCY_EXCHANGE_RATE&from_currency=EUR&to_currency=USD&apikey={}",
            self.base_url(),
            self.api_key
        );
        let resp: ForexResponse = self
            .client
            .get(&url)
            .send()
            .context("HTTP request failed")?
            .json()
            .context("Failed to parse EUR/USD response")?;

        resp.exchange_rate
            .rate
            .parse::<f64>()
            .context("Failed to parse EUR/USD rate")
    }
}

// ── RSI zone classification ───────────────────────────────────────────────────

fn rsi_zone(rsi: f64) -> &'static str {
    if rsi < 40.0 {
        "BUY"
    } else if rsi <= 70.0 {
        "HOLD"
    } else {
        "NO ENTRY"
    }
}

// ── Pretty table printer ──────────────────────────────────────────────────────

fn print_table(snapshot: &SignalSnapshot) {
    println!();
    println!("  THE SIGNAL — Portfolio Snapshot");
    println!("  {}", snapshot.timestamp);
    println!("  Gold spot: ${:.2} USD  |  €{:.2} EUR  |  EUR/USD: {:.4}",
             snapshot.gold_spot_usd, snapshot.gold_spot_eur, snapshot.eurusd_rate);
    println!();
    println!("  {:<6}  {:<8}  {:>10}  {:>10}  {:>10}  {:>8}  {:>9}  {:>8}",
             "TICKER", "NAME", "PRICE USD", "PRICE EUR", "OZ GOLD", "RSI 14", "ZONE", "CHG %");
    println!("  {}", "─".repeat(80));

    for p in &snapshot.positions {
        let zone_display = match p.zone.as_str() {
            "BUY"      => format!("\x1b[32m{:<9}\x1b[0m", p.zone),
            "HOLD"     => format!("\x1b[33m{:<9}\x1b[0m", p.zone),
            "NO ENTRY" => format!("\x1b[31m{:<9}\x1b[0m", p.zone),
            _          => format!("{:<9}", p.zone),
        };

        println!("  {:<6}  {:<8}  {:>10.2}  {:>10.2}  {:>10.4}  {:>8.1}  {}  {:>7}%",
                 p.ticker,
                 p.name,
                 p.price_usd,
                 p.price_eur,
                 p.oz_of_gold,
                 p.rsi_14,
                 zone_display,
                 p.change_percent,
        );
    }
    println!();
    println!("  Zone key:  \x1b[32mBUY\x1b[0m RSI < 40  ·  \x1b[33mHOLD\x1b[0m RSI 40–70  ·  \x1b[31mNO ENTRY\x1b[0m RSI > 70");
    println!();
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    // Load .env file if present (local dev)
    let _ = dotenvy::dotenv();

    let cli = Cli::parse();

    let api_key = std::env::var("ALPHA_VANTAGE_API_KEY")
        .context("ALPHA_VANTAGE_API_KEY not set. Add it to .env or set it as an env var.")?;

    let av = AlphaVantage::new(api_key);

    // Determine tickers to fetch
    let tickers: Vec<(String, String)> = if let Some(custom) = cli.tickers {
        custom.iter().map(|t| (t.clone(), t.clone())).collect()
    } else {
        DEFAULT_TICKERS
            .iter()
            .map(|(t, n)| (t.to_string(), n.to_string()))
            .collect()
    };

    eprintln!("Fetching gold spot (XAU/USD)...");
    let gold_usd = av.fetch_gold_usd()?;

    eprintln!("Fetching EUR/USD rate...");
    let eurusd = av.fetch_eurusd()?;

    let gold_eur = gold_usd / eurusd;

    let mut positions: Vec<PositionOutput> = Vec::new();

    for (ticker, name) in &tickers {
        eprintln!("Fetching {} ({})...", ticker, name);

        let (price_usd, change_pct) = av.fetch_price(ticker)?;
        let rsi = av.fetch_rsi(ticker)?;

        let price_eur = price_usd / eurusd;
        let oz_of_gold = price_usd / gold_usd;
        let zone = rsi_zone(rsi).to_string();

        positions.push(PositionOutput {
            ticker: ticker.clone(),
            name: name.clone(),
            price_usd,
            price_eur,
            oz_of_gold,
            rsi_14: rsi,
            zone,
            change_percent: change_pct,
        });
    }

    let snapshot = SignalSnapshot {
        timestamp: chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string(),
        gold_spot_usd: gold_usd,
        gold_spot_eur: gold_eur,
        eurusd_rate: eurusd,
        positions,
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&snapshot)?);
    } else {
        print_table(&snapshot);
    }

    Ok(())
}