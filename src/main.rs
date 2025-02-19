use chrono::prelude::*;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::process;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Global thresholds used for both stocks and crypto.
#[derive(Debug, Deserialize)]
struct Thresholds {
    critdown: f64, // if percentage change < critdown then "critdown"
    down: f64,     // if percentage change < down (but >= critdown) then "down"
    wayup: f64,    // if percentage change > wayup then "wayup"
}

/// Stock (Tiingo) configuration (optional).
#[derive(Debug, Deserialize)]
struct StockConfig {
    api_key: String, // Can be overridden by the TIINGO_API_KEY env variable.
    tickers: Vec<String>,
    cache_max_age: u64,         // Cache age for weekdays.
    weekend_cache_max_age: u64, // Cache age for weekends.
}

/// Crypto configuration.
#[derive(Debug, Deserialize)]
struct CryptoConfig {
    trade_pairs: Vec<String>,
    trade_signs: Vec<String>,
    chart_interval: u64,
    cache_max_age: u64, // Cache age (in seconds) for crypto data.
}

/// Top-level configuration.
#[derive(Debug, Deserialize)]
struct Config {
    rotation_seconds: u64, // Global rotation interval for the combined list.
    stock: Option<StockConfig>,
    thresholds: Thresholds,
    crypto: Option<CryptoConfig>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command-line arguments.
    // If an argument (not starting with "--") is provided, it's the config file.
    // The "--continuous" flag makes the application loop indefinitely.
    let args: Vec<String> = env::args().collect();
    let mut config_file = "config.toml".to_string();
    let mut continuous = false;
    for arg in args.iter().skip(1) {
        if arg == "--continuous" {
            continuous = true;
        } else if !arg.starts_with("--") {
            config_file = arg.clone();
        }
    }

    // Load configuration.
    let config_contents = fs::read_to_string(&config_file).map_err(|err| {
        eprintln!(
            "Error: Could not read config file '{}': {}",
            config_file, err
        );
        err
    })?;
    let config: Config = toml::from_str(&config_contents).map_err(|err| {
        eprintln!(
            "Error: Could not parse config file '{}': {}",
            config_file, err
        );
        err
    })?;

    if continuous {
        loop {
            output_current_instrument(&config)?;
            thread::sleep(Duration::from_secs(config.rotation_seconds));
        }
    } else {
        output_current_instrument(&config)?;
    }
    Ok(())
}

/// Combines available stock and crypto instruments, rotates through them,
/// fetches data for the current instrument, and prints the JSON output on one line.
/// If neither are defined, the program exits with an error.
fn output_current_instrument(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let mut instruments: Vec<(&str, &str, &str)> = Vec::new();

    // Add stock instruments if defined and if tickers are provided.
    if let Some(stock) = &config.stock {
        if !stock.tickers.is_empty() {
            for ticker in &stock.tickers {
                instruments.push(("stock", ticker, ""));
            }
        }
    }

    // Add crypto instruments if defined and if trade pairs are provided.
    if let Some(crypto) = &config.crypto {
        if !crypto.trade_pairs.is_empty() {
            for (i, pair) in crypto.trade_pairs.iter().enumerate() {
                let sign = crypto.trade_signs.get(i).map(|s| s.as_str()).unwrap_or("");
                instruments.push(("crypto", pair, sign));
            }
        }
    }

    if instruments.is_empty() {
        eprintln!("Error: No instruments defined in the configuration.");
        process::exit(1);
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let index = (now / config.rotation_seconds) % (instruments.len() as u64);
    let (inst_type, symbol, sign) = instruments[index as usize];

    let output = if inst_type == "stock" {
        run_tiingo_for_ticker(symbol, config)?
    } else {
        run_crypto_for_pair(symbol, sign, config)?
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// Fetches stock data from Tiingo for a given ticker, using caching.
/// The environment variable `TIINGO_API_KEY` (if set) overrides the API key in the config.
fn run_tiingo_for_ticker(
    ticker: &str,
    config: &Config,
) -> Result<Value, Box<dyn std::error::Error>> {
    // It is safe to unwrap because this function is only called if a stock instrument is selected.
    let stock_config = config.stock.as_ref().expect("Stock configuration missing");
    let api_key = match env::var("TIINGO_API_KEY") {
        Ok(key) if !key.trim().is_empty() => key,
        _ => stock_config.api_key.clone(),
    };

    let local_now = Local::now();
    let effective_cache_max_age =
        if local_now.weekday() == Weekday::Sat || local_now.weekday() == Weekday::Sun {
            stock_config.weekend_cache_max_age
        } else {
            stock_config.cache_max_age
        };

    let cache_file = format!("cache_{}.json", ticker);
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

    let tiingo_url = format!("https://api.tiingo.com/iex/{}", ticker);
    let client = Client::new();
    let response_text = if use_cache {
        fs::read_to_string(&cache_file)?
    } else {
        let response = client
            .get(&tiingo_url)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Token {}", api_key))
            .send()?;
        if !response.status().is_success() {
            eprintln!("Error: Failed to fetch data from: {}", tiingo_url);
            process::exit(1);
        }
        let text = response.text()?;
        fs::write(&cache_file, &text)?;
        text
    };

    let cache_age = {
        let metadata = fs::metadata(&cache_file)?;
        let modified = metadata.modified()?;
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::new(0, 0))
            .as_secs()
    };

    let json_data: Value = serde_json::from_str(&response_text)?;
    let first_entry = json_data
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
            ticker
        );
        process::exit(1);
    }
    let price_change_pct = ((last_price - prev_close) / prev_close) * 100.0;

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

    Ok(json!({
        "text": format!("{} ${:.2} ({:.2}%)", ticker, last_price, price_change_pct),
        "tooltip": format!("Cache Age: {} seconds (Max allowed: {} seconds)", cache_age, effective_cache_max_age),
        "class": class,
    }))
}

/// Fetches crypto data from Kraken (renamed to Crypto) for a given trade pair,
/// using caching for both OHLC and ticker endpoints.
fn run_crypto_for_pair(
    pair: &str,
    sign: &str,
    config: &Config,
) -> Result<Value, Box<dyn std::error::Error>> {
    let crypto = match &config.crypto {
        Some(c) if !c.trade_pairs.is_empty() => c,
        _ => {
            eprintln!("Error: No crypto configuration available.");
            process::exit(1);
        }
    };

    let cache_max_age = crypto.cache_max_age;
    let cache_file_ohlc = format!("cache_crypto_{}_ohlc.json", pair);
    let cache_file_ticker = format!("cache_crypto_{}_ticker.json", pair);

    let use_cache_ohlc = if let Ok(metadata) = fs::metadata(&cache_file_ohlc) {
        if let Ok(modified) = metadata.modified() {
            let elapsed = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(u64::MAX));
            elapsed < Duration::from_secs(cache_max_age)
        } else {
            false
        }
    } else {
        false
    };

    let use_cache_ticker = if let Ok(metadata) = fs::metadata(&cache_file_ticker) {
        if let Ok(modified) = metadata.modified() {
            let elapsed = SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(u64::MAX));
            elapsed < Duration::from_secs(cache_max_age)
        } else {
            false
        }
    } else {
        false
    };

    let kraken_api = "https://api.kraken.com/0/public";
    let ohlc_url = format!(
        "{}/OHLC?pair={}&interval={}",
        kraken_api, pair, crypto.chart_interval
    );
    let ticker_url = format!("{}/Ticker?pair={}", kraken_api, pair);
    let client = Client::new();

    let ohlc_text = if use_cache_ohlc {
        fs::read_to_string(&cache_file_ohlc)?
    } else {
        let response = client
            .get(&ohlc_url)
            .header("Accept", "application/json")
            .send()?;
        if !response.status().is_success() {
            eprintln!("Error: Failed to fetch OHLC data from: {}", ohlc_url);
            process::exit(1);
        }
        let text = response.text()?;
        fs::write(&cache_file_ohlc, &text)?;
        text
    };

    let ticker_text = if use_cache_ticker {
        fs::read_to_string(&cache_file_ticker)?
    } else {
        let response = client
            .get(&ticker_url)
            .header("Accept", "application/json")
            .send()?;
        if !response.status().is_success() {
            eprintln!("Error: Failed to fetch Ticker data from: {}", ticker_url);
            process::exit(1);
        }
        let text = response.text()?;
        fs::write(&cache_file_ticker, &text)?;
        text
    };

    let ticker_json: Value = serde_json::from_str(&ticker_text)?;
    let current_value = ticker_json
        .get("result")
        .and_then(|r| r.get(pair))
        .and_then(|pair| pair.get("p"))
        .and_then(|p| p.get(0))
        .and_then(|val| val.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or("Error: Could not retrieve current price for crypto")?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let yesterday_timestamp = now - 86_400;
    let ohlc_json: Value = serde_json::from_str(&ohlc_text)?;
    let candles = ohlc_json
        .get("result")
        .and_then(|r| r.get(pair))
        .and_then(|v| v.as_array())
        .ok_or("Error: Could not retrieve OHLC candles array")?;
    let mut old_vwap: Option<f64> = None;
    for candle in candles {
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
    let old_vwap = old_vwap.unwrap_or(current_value);
    let change_percentage_opt = if old_vwap == 0.0 {
        None
    } else {
        Some(((current_value - old_vwap) / old_vwap) * 100.0)
    };
    let change_percentage_str = match change_percentage_opt {
        Some(val) => format!("{:.2}", val),
        None => "NA".to_string(),
    };
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
    Ok(json!({
        "text": format!("{} €{} ({}%)", sign, current_value_str, change_percentage_str),
        "tooltip": format!("€{} ({}%)", current_value_str, change_percentage_str),
        "class": status_class,
    }))
}
