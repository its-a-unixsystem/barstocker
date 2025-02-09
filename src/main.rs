use chrono::prelude::*;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::Value;
use std::env;
use std::fs;
use std::process;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Thresholds for classifying the percentage price change.
#[derive(Debug, Deserialize)]
struct Thresholds {
    critdown: f64, // if price change < critdown then mark as "critdown"
    down: f64,     // if price change < down (but >= critdown) then mark as "down"
    wayup: f64,    // if price change > wayup then mark as "wayup"
}

/// The configuration file structure loaded from a TOML file.
#[derive(Debug, Deserialize)]
struct Config {
    api_key: String,
    tickers: Vec<String>,
    rotation_seconds: u64,
    cache_max_age: u64,           // in seconds (for weekdays)
    weekend_cache_max_age: u64,   // in seconds (for Saturdays and Sundays)
    thresholds: Thresholds,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ---------------------------------------------------------------------
    // Load Configuration from External File
    // ---------------------------------------------------------------------
    let args: Vec<String> = env::args().collect();
    let config_file = if args.len() > 1 {
        &args[1]
    } else {
        "config.toml"
    };

    let config_contents = fs::read_to_string(config_file).map_err(|err| {
        eprintln!("Error: Could not read config file '{}': {}", config_file, err);
        err
    })?;
    let config: Config = toml::from_str(&config_contents).map_err(|err| {
        eprintln!("Error: Could not parse config file '{}': {}", config_file, err);
        err
    })?;

    if config.api_key.trim().is_empty() {
        eprintln!("Error: API key in config file '{}' is empty.", config_file);
        process::exit(1);
    }
    if config.tickers.is_empty() {
        eprintln!("Error: No tickers provided in config file '{}'.", config_file);
        process::exit(1);
    }

    // ---------------------------------------------------------------------
    // Compute the Ticker to Use Based on Time-Based Rotation
    // ---------------------------------------------------------------------
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs();
    let ticker_index = ((now / config.rotation_seconds) % config.tickers.len() as u64) as usize;
    let selected_ticker = &config.tickers[ticker_index];

    // ---------------------------------------------------------------------
    // Determine the Effective Cache Max Age Based on Day of the Week
    // ---------------------------------------------------------------------
    let local_now = Local::now();
    let today = local_now.weekday();
    let effective_cache_max_age = if today == Weekday::Sat || today == Weekday::Sun {
        config.weekend_cache_max_age
    } else {
        config.cache_max_age
    };

    // ---------------------------------------------------------------------
    // Cache Logic: Check if cached data is fresh enough.
    // ---------------------------------------------------------------------
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

    // ---------------------------------------------------------------------
    // Construct the Tiingo API URL and Fetch Data (from cache or API)
    // ---------------------------------------------------------------------
    let tiingo_url = format!("https://api.tiingo.com/iex/{}", selected_ticker);
    let response_text: String;
    if use_cache {
        // Use cached data if it is fresh.
        response_text = fs::read_to_string(&cache_file).map_err(|err| {
            eprintln!("Error: Failed to read cache file '{}': {}", cache_file, err);
            err
        })?;
    } else {
        // Fetch fresh data from the API.
        let client = Client::new();
        let response = client
            .get(&tiingo_url)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Token {}", config.api_key))
            .send()?;

        if !response.status().is_success() {
            eprintln!("Error: Failed to fetch data from: {}", tiingo_url);
            process::exit(1);
        }

        response_text = response.text()?;
        // Update cache with fresh data.
        fs::write(&cache_file, &response_text).map_err(|err| {
            eprintln!("Error: Failed to write cache file '{}': {}", cache_file, err);
            err
        })?;
    }

    // Get the cache age (in seconds) from the file's modification time.
    let cache_age = {
        let metadata = fs::metadata(&cache_file)?;
        let modified = metadata.modified()?;
        SystemTime::now()
            .duration_since(modified)
            .unwrap_or(Duration::new(0, 0))
            .as_secs()
    };

    // ---------------------------------------------------------------------
    // Parse the JSON Response
    // ---------------------------------------------------------------------
    let json: Value = serde_json::from_str(&response_text)?;
    let first_entry = json.get(0).ok_or_else(|| {
        eprintln!("Error: API response does not contain an array with at least one element.");
        "Invalid API response"
    })?;

    let last_price = first_entry
        .get("tngoLast")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| {
            eprintln!("Error: Missing or invalid 'tngoLast' in API response.");
            "Invalid tngoLast field"
        })?;
    let prev_close = first_entry
        .get("prevClose")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| {
            eprintln!("Error: Missing or invalid 'prevClose' in API response.");
            "Invalid prevClose field"
        })?;

    if prev_close == 0.0 {
        eprintln!(
            "Error: Previous close is zero for ticker: {} (cannot calculate % change).",
            selected_ticker
        );
        process::exit(1);
    }

    // ---------------------------------------------------------------------
    // Calculate the Percentage Change
    // ---------------------------------------------------------------------
    let price_change_pct = ((last_price - prev_close) / prev_close) * 100.0;

    // Use threshold settings from config to determine the CSS class.
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

    // ---------------------------------------------------------------------
    // Output JSON for Consumption by a Status Bar or Widget
    // ---------------------------------------------------------------------
    let output = serde_json::json!({
        "text": format!("{} ${:.2} ({:.2}%)", selected_ticker, last_price, price_change_pct),
        "tooltip": format!(
            "Cache Age: {} seconds (Max allowed: {} seconds)",
            cache_age, effective_cache_max_age
        ),
        "class": class,
    });

    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}
