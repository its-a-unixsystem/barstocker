use chrono::prelude::*;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::process;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Thresholds for classifying the percentage change (for both stocks and crypto).
#[derive(Debug, Deserialize)]
struct Thresholds {
    critdown: f64, // if price change < critdown then mark as "critdown"
    down: f64,     // if price change < down (but >= critdown) then mark as "down"
    wayup: f64,    // if price change > wayup then mark as "wayup"
}

/// Kraken (crypto) configuration.
#[derive(Debug, Deserialize)]
struct KrakenConfig {
    trade_pairs: Vec<String>,
    trade_signs: Vec<String>,
    rotation_seconds: u64,
    chart_interval: u64,
}

/// Top-level configuration.
#[derive(Debug, Deserialize)]
struct Config {
    // --- Tiingo (Stock) Settings ---
    api_key: String,
    tickers: Vec<String>,
    rotation_seconds: u64,
    cache_max_age: u64,
    weekend_cache_max_age: u64,
    thresholds: Thresholds,

    // --- Kraken (Crypto) Settings (Optional) ---
    kraken: Option<KrakenConfig>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Determine the configuration file and mode.
    // The first argument (if not a flag) is the config file; use default "config.toml" otherwise.
    let args: Vec<String> = env::args().collect();
    let config_file = if args.len() > 1 && !args[1].starts_with("--") {
        &args[1]
    } else {
        "config.toml"
    };
    // If any argument equals "--crypto", run in crypto mode.
    let use_crypto = args.iter().any(|arg| arg == "--crypto");

    // Load configuration.
    let config_contents = fs::read_to_string(config_file).map_err(|err| {
        eprintln!("Error: Could not read config file '{}': {}", config_file, err);
        err
    })?;
    let config: Config = toml::from_str(&config_contents).map_err(|err| {
        eprintln!("Error: Could not parse config file '{}': {}", config_file, err);
        err
    })?;

    if use_crypto {
        if config.kraken.is_none() {
            eprintln!("Error: Kraken configuration not found in config file.");
            process::exit(1);
        }
        run_kraken(&config)?;
    } else {
        run_tiingo(&config)?;
    }

    Ok(())
}

/// Runs the Tiingo (stock) branch.
fn run_tiingo(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Compute the ticker to use based on time-based rotation.
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let ticker_index = ((now / config.rotation_seconds) % config.tickers.len() as u64) as usize;
    let selected_ticker = &config.tickers[ticker_index];

    // Determine effective cache max age based on day of the week.
    let local_now = Local::now();
    let today = local_now.weekday();
    let effective_cache_max_age = if today == Weekday::Sat || today == Weekday::Sun {
        config.weekend_cache_max_age
    } else {
        config.cache_max_age
    };

    // Check cache (stored in a file named "cache_<ticker>.json").
    let cache_file = format!("cache_{}.json", selected_ticker);
    let use_cache = if let Ok(metadata) = fs::metadata(&cache_file) {
        if let Ok(modified) = metadata.modified() {
            let elapsed = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(u64::MAX));
            elapsed < Duration::from_secs(effective_cache_max_age)
        } else {
            false
        }
    } else {
        false
    };

    let tiingo_url = format!("https://api.tiingo.com/iex/{}", selected_ticker);
    let client = Client::new();
    let response_text = if use_cache {
        fs::read_to_string(&cache_file)?
    } else {
        let response = client
            .get(&tiingo_url)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Token {}", config.api_key))
            .send()?;
        if !response.status().is_success() {
            eprintln!("Error: Failed to fetch data from: {}", tiingo_url);
            process::exit(1);
        }
        let text = response.text()?;
        fs::write(&cache_file, &text)?;
        text
    };

    // Calculate the cache age.
    let cache_age = {
        let metadata = fs::metadata(&cache_file)?;
        let modified = metadata.modified()?;
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::new(0, 0))
            .as_secs()
    };

    // Parse the JSON response.
    let json: Value = serde_json::from_str(&response_text)?;
    let first_entry = json
        .get(0)
        .ok_or("Invalid API response: missing array element")?;
    let last_price = first_entry
        .get("tngoLast")
        .and_then(|v| v.as_f64())
        .ok_or("Invalid tngoLast field")?;
    let prev_close = first_entry
        .get("prevClose")
        .and_then(|v| v.as_f64())
        .ok_or("Invalid prevClose field")?;
    if prev_close == 0.0 {
        eprintln!(
            "Error: Previous close is zero for ticker: {} (cannot calculate % change).",
            selected_ticker
        );
        process::exit(1);
    }
    let price_change_pct = ((last_price - prev_close) / prev_close) * 100.0;

    // Determine output class using the global thresholds.
    let class = if price_change_pct < config.thresholds.down {
        if price_change_pct < config.thresholds.critdown {
            "critdown"
        } else {
            "down"
        }
    } else if price_change_pct > config.thresholds.wayup {
        "wayup"
    } else {
        "up"
    };

    // Build the JSON output (single line, no line breaks).
    let output = json!({
        "text": format!("{} ${:.2} ({:.2}%)", selected_ticker, last_price, price_change_pct),
        "tooltip": format!("Cache Age: {}/{} seconds", cache_age, effective_cache_max_age),
        "class": class,
    });

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// Runs the Kraken (crypto) branch.
fn run_kraken(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let kraken = config.kraken.as_ref().unwrap();

    // Determine which trade pair/sign to use based on time-based rotation.
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let pair_index = ((now / kraken.rotation_seconds) % kraken.trade_pairs.len() as u64) as usize;
    let selected_pair = &kraken.trade_pairs[pair_index];
    let selected_sign = &kraken.trade_signs[pair_index];

    // Calculate "yesterday" as 24 hours ago.
    let yesterday_timestamp = now - 86_400;

    // Kraken public API endpoints.
    let kraken_api = "https://api.kraken.com/0/public";
    let ohlc_url = format!(
        "{}/OHLC?pair={}&interval={}",
        kraken_api, selected_pair, kraken.chart_interval
    );
    let ticker_url = format!("{}/Ticker?pair={}", kraken_api, selected_pair);

    let client = Client::new();

    // Fetch OHLC data.
    let ohlc_response = client
        .get(&ohlc_url)
        .header("Accept", "application/json")
        .send()?;
    if !ohlc_response.status().is_success() {
        eprintln!("Error: Failed to fetch OHLC data from: {}", ohlc_url);
        process::exit(1);
    }
    let ohlc_text = ohlc_response.text()?;
    let ohlc_json: Value = serde_json::from_str(&ohlc_text)?;

    // Fetch Ticker data.
    let ticker_response = client
        .get(&ticker_url)
        .header("Accept", "application/json")
        .send()?;
    if !ticker_response.status().is_success() {
        eprintln!("Error: Failed to fetch Ticker data from: {}", ticker_url);
        process::exit(1);
    }
    let ticker_text = ticker_response.text()?;
    let ticker_json: Value = serde_json::from_str(&ticker_text)?;

    // Extract current price from the ticker JSON.
    // Path: .result[selected_pair].p[0]
    let current_value = ticker_json
        .get("result")
        .and_then(|r| r.get(selected_pair))
        .and_then(|pair| pair.get("p"))
        .and_then(|p| p.get(0))
        .and_then(|val| val.as_str())
        .and_then(|s| s.parse::<f64>().ok());
    let current_value = match current_value {
        Some(val) => val,
        None => {
            eprintln!("Error: Could not retrieve current price for {}.", selected_pair);
            process::exit(1);
        }
    };

    // Retrieve the OHLC candles array for the selected pair.
    let candles = ohlc_json
        .get("result")
        .and_then(|r| r.get(selected_pair))
        .and_then(|v| v.as_array())
        .ok_or("Error: Could not retrieve OHLC candles array")?;

    // Find the last candle from or before "yesterday."
    let mut old_vwap: Option<f64> = None;
    for candle in candles {
        // Each candle is expected to be an array:
        // [time, open, high, low, close, vwap, volume, count]
        if let Some(ts) = candle.get(0).and_then(|v| v.as_i64()) {
            if ts <= yesterday_timestamp as i64 {
                if let Some(close) = candle
                    .get(4)
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                {
                    old_vwap = Some(close);
                }
            }
        }
    }
    // Default to current_value if no valid candle is found.
    let old_vwap = old_vwap.unwrap_or(current_value);

    // Calculate the percentage change.
    let change_percentage_opt = if old_vwap == 0.0 {
        None
    } else {
        Some(((current_value - old_vwap) / old_vwap) * 100.0)
    };
    let change_percentage_str = match change_percentage_opt {
        Some(val) => format!("{:.2}", val),
        None => "NA".to_string(),
    };

    // Use the global thresholds for classification.
    let status_class = if let Some(change) = change_percentage_opt {
        if change < config.thresholds.down {
            if change < config.thresholds.critdown {
                "critdown"
            } else {
                "down"
            }
        } else if change > config.thresholds.wayup {
            "wayup"
        } else {
            "up"
        }
    } else {
        "up"
    };

    let current_value_str = format!("{:.2}", current_value);

    // Build the JSON output (single line, no line breaks).
    let output = json!({
        "text": format!("{} €{} ({}%)", selected_sign, current_value_str, change_percentage_str),
        "tooltip": format!("€{} ({}%)", current_value_str, change_percentage_str),
        "class": status_class,
    });
    println!("{}", serde_json::to_string(&output)?);

    Ok(())
}
