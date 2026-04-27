use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;

// ── Snapshot types (mirrors signal-fetch JSON output) ────────────────────────

#[derive(Deserialize)]
struct Position {
    ticker: String,
    name: String,
    price_usd: f64,
    price_eur: f64,
    oz_of_gold: f64,
    rsi_14: f64,
    zone: String,
    change_percent: String,
}

#[derive(Deserialize)]
struct SignalSnapshot {
    timestamp: String,
    gold_spot_usd: f64,
    gold_spot_eur: f64,
    eurusd_rate: f64,
    positions: Vec<Position>,
}

// ── Anthropic API types ───────────────────────────────────────────────────────

#[derive(Serialize)]
struct ApiMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ApiRequest {
    model: String,
    max_tokens: u32,
    system: String,
    messages: Vec<ApiMessage>,
}

#[derive(Deserialize)]
struct ApiResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
struct ContentBlock {
    text: String,
}

// ── Prompts ───────────────────────────────────────────────────────────────────

fn build_system_prompt() -> String {
    r#"You are the editor of The Signal, a weekly gold-benchmarked investment newsletter.
Write each issue in clear, professional financial prose. Avoid hype and speculation.
Ground every comment in the data provided.

Use this exact Markdown template — do not add or remove sections:

# The Signal — Issue [N]
**[date]**
---
## Framework
[Describe the gold benchmark approach. Include a brief RSI rule table: RSI < 40 = BUY, RSI 40–70 = HOLD, RSI > 70 = NO ENTRY]
## Portfolio snapshot
[Markdown table with columns: Ticker | Price EUR | oz gold | RSI-14 | Zone | Chg%]
## This week
[2–3 paragraphs covering macro context and key events relevant to the portfolio]
## Position signals
[Only positions that need action (BUY or NO ENTRY zone). 2–3 sentences per position. Skip HOLD positions.]
## Actions
[Bullet list of concrete actions, or "No action required this week." if all positions are HOLD]
---
*The Signal — every Monday. Not financial advice.*

Replace [N] with the issue number inferred from the date (week number of the year is acceptable).
Replace [date] with the snapshot date."#.to_string()
}

fn build_user_prompt(snapshot: &SignalSnapshot) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Snapshot timestamp: {}", snapshot.timestamp));
    lines.push(format!("Gold spot: ${:.2} USD / €{:.2} EUR", snapshot.gold_spot_usd, snapshot.gold_spot_eur));
    lines.push(format!("EUR/USD rate: {:.4}", snapshot.eurusd_rate));
    lines.push(String::new());
    lines.push("Positions:".to_string());

    for p in &snapshot.positions {
        lines.push(format!(
            "  {} ({}) — Price: ${:.2} USD / €{:.2} EUR | {:.4} oz gold | RSI-14: {:.1} | Zone: {} | Change: {}%",
            p.ticker, p.name, p.price_usd, p.price_eur, p.oz_of_gold, p.rsi_14, p.zone, p.change_percent
        ));
    }

    lines.push(String::new());
    lines.push("Please write the full newsletter issue using the template.".to_string());

    lines.join("\n")
}

// ── Anthropic API call ────────────────────────────────────────────────────────

fn call_anthropic(system: &str, user: &str) -> Result<String> {
    let api_key = std::env::var("ANTHROPIC_API_KEY")
        .context("ANTHROPIC_API_KEY not set. Add it to .env or set it as an env var.")?;

    let client = reqwest::blocking::Client::new();

    let body = ApiRequest {
        model: "claude-sonnet-4-6".to_string(),
        max_tokens: 2048,
        system: system.to_string(),
        messages: vec![ApiMessage {
            role: "user".to_string(),
            content: user.to_string(),
        }],
    };

    let resp: ApiResponse = client
        .post("https://api.anthropic.com/v1/messages")
        .header("x-api-key", &api_key)
        .header("anthropic-version", "2023-06-01")
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .context("HTTP request to Anthropic API failed")?
        .json()
        .context("Failed to parse Anthropic API response")?;

    resp.content
        .into_iter()
        .next()
        .map(|b| b.text)
        .context("Anthropic returned an empty content array")
}

// ── Main ──────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let _ = dotenvy::dotenv();

    let raw = fs::read_to_string("data.json").context("Failed to read data.json")?;
    let snapshot: SignalSnapshot = serde_json::from_str(&raw).context("Failed to parse data.json")?;

    let system = build_system_prompt();
    let user = build_user_prompt(&snapshot);

    eprintln!("Calling Anthropic API...");
    let draft = call_anthropic(&system, &user)?;

    fs::create_dir_all("drafts").context("Failed to create drafts/ directory")?;

    // Extract YYYY-MM-DD from "2026-04-21 07:00 UTC"
    let date_str = snapshot
        .timestamp
        .split_whitespace()
        .next()
        .unwrap_or("unknown");

    let output_path = format!("drafts/issue-{}.md", date_str);
    fs::write(&output_path, &draft).context("Failed to write draft file")?;

    eprintln!("Draft written to {}", output_path);
    Ok(())
}
