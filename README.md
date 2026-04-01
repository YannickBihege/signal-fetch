# signal-fetch

Rust CLI that fetches price, RSI(14), and gold-denominated values for [The Signal](https://substack.com) — a weekly data-driven investment newsletter.

## Requirements

- Rust 1.75+
- Alpha Vantage API key (free at [alphavantage.co](https://www.alphavantage.co/support/#api-key))

## Setup

```bash
git clone https://github.com/YannickBihege/signal-fetch.git
cd signal-fetch
cp .env.example .env        # add your API key to .env
cargo build --release
```

## Usage

```bash
# Pretty table (default)
./target/release/signal-fetch

# JSON output (for pipeline)
./target/release/signal-fetch --json

# Custom tickers
./target/release/signal-fetch --tickers TSM,NVDA,ASML
```

## Output

```
THE SIGNAL — Portfolio Snapshot
2026-04-01 07:00 UTC | Gold: $3012.45 | EUR/USD: 1.0836

TICKER  NAME    PRICE USD  PRICE EUR  OZ GOLD  RSI 14  ZONE
──────────────────────────────────────────────────────────
TSM     TSMC       162.44     149.90   0.0539    52.3  HOLD
NVDA    NVIDIA     875.22     807.71   0.2906    61.8  HOLD
```

Zone: **BUY** RSI < 40 · **HOLD** RSI 40–70 · **NO ENTRY** RSI > 70

## Automation

Runs every Monday at 07:00 UTC via GitHub Actions.
Add `ALPHA_VANTAGE_API_KEY` as a repository secret to activate.

## API budget

12 calls per run (gold + EUR/USD + 2 per ticker × 5 positions).
Free tier limit: 25 calls/day.