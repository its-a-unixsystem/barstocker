use chrono::prelude::*;
use reqwest::blocking::Client;
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const SECONDS_PER_DAY: u64 = 86_400;

/// Global thresholds used for both stocks and crypto.
#[derive(Debug, Deserialize)]
struct Thresholds {
    critdown: f64, // if percentage change < critdown then "critdown"
    down: f64,     // if percentage change < down (but >= critdown) then "down"
    wayup: f64,    // if percentage change > wayup then "wayup"
    #[serde(default)]
    up_color: Option<String>,
    #[serde(default)]
    wayup_color: Option<String>,
    #[serde(default)]
    down_color: Option<String>,
    #[serde(default)]
    waydown_color: Option<String>,
}

/// Stock (Tiingo) configuration (optional).
#[derive(Debug, Deserialize)]
struct StockConfig {
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
    ticker: Option<TickerConfig>,
}

/// Ticker mode configuration.
#[derive(Debug, Deserialize)]
struct TickerConfig {
    window_size: usize, // Number of visible characters
    separator: String,  // Separator between items (e.g., " - ")
    #[allow(dead_code)]
    refresh_seconds: u64, // How often to refresh data
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load environment variables from .env.local (if it exists)
    // This allows loading API keys without hardcoding them
    let _ = dotenvy::from_filename(".env.local");

    // Parse command-line arguments.
    // If an argument (not starting with "--") is provided, it's the config file.
    // The "--continuous" flag makes the application loop indefinitely.
    // The "--ticker" flag enables scrolling ticker mode.
    // The "--crypto" flag shows only crypto instruments (ticker mode only).
    // The "--stock" flag shows only stock instruments (ticker mode only).
    let args: Vec<String> = env::args().collect();
    let mut config_file = "config.toml".to_string();
    let mut continuous = false;
    let mut ticker_mode = false;
    let mut filter_mode: Option<&str> = None;

    for arg in args.iter().skip(1) {
        if arg == "--continuous" {
            continuous = true;
        } else if arg == "--ticker" {
            ticker_mode = true;
        } else if arg == "--crypto" {
            filter_mode = Some("crypto");
        } else if arg == "--stock" {
            filter_mode = Some("stock");
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

    // Create a single HTTP client to reuse across all requests.
    // This enables connection pooling and DNS caching, dramatically reducing DNS queries.
    let client = Client::new();

    if ticker_mode {
        run_ticker_mode(&config, filter_mode, &client)?;
    } else if continuous {
        loop {
            output_current_instrument(&config, filter_mode, &client)?;
            thread::sleep(Duration::from_secs(config.rotation_seconds));
        }
    } else {
        output_current_instrument(&config, filter_mode, &client)?;
    }
    Ok(())
}

/// Combines available stock and crypto instruments, rotates through them,
/// fetches data for the current instrument, and prints the JSON output on one line.
/// If neither are defined, the program exits with an error.
/// The filter_mode parameter can restrict to only "stock" or "crypto" instruments.
fn output_current_instrument(
    config: &Config,
    filter_mode: Option<&str>,
    client: &Client,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut instruments: Vec<(&str, &str, &str)> = Vec::new();

    // Add stock instruments if defined and if tickers are provided.
    if filter_mode.is_none() || filter_mode == Some("stock") {
        if let Some(stock) = &config.stock {
            if !stock.tickers.is_empty() {
                for ticker in &stock.tickers {
                    instruments.push(("stock", ticker, ""));
                }
            }
        }
    }

    // Add crypto instruments if defined and if trade pairs are provided.
    if filter_mode.is_none() || filter_mode == Some("crypto") {
        if let Some(crypto) = &config.crypto {
            if !crypto.trade_pairs.is_empty() {
                for (i, pair) in crypto.trade_pairs.iter().enumerate() {
                    let sign = crypto.trade_signs.get(i).map(|s| s.as_str()).unwrap_or("");
                    instruments.push(("crypto", pair, sign));
                }
            }
        }
    }

    if instruments.is_empty() {
        return Err("No instruments defined in the configuration".into());
    }

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let index = (now / config.rotation_seconds) % (instruments.len() as u64);
    let (inst_type, symbol, sign) = instruments[index as usize];

    let output = if inst_type == "stock" {
        run_tiingo_for_ticker(symbol, config, client)?
    } else {
        run_crypto_for_pair(symbol, sign, config, client)?
    };

    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

/// Checks if a cache file is still valid based on its age.
fn is_cache_valid(cache_file: &str, max_age_secs: u64) -> bool {
    fs::metadata(cache_file)
        .and_then(|m| m.modified())
        .map(|modified| {
            SystemTime::now()
                .duration_since(modified)
                .unwrap_or(Duration::from_secs(u64::MAX))
                < Duration::from_secs(max_age_secs)
        })
        .unwrap_or(false)
}

/// Calculates percentage change between two values.
/// Returns None if the base value is zero to avoid division by zero.
fn calculate_percentage_change(current: f64, base: f64) -> Option<f64> {
    if base == 0.0 {
        None
    } else {
        Some(((current - base) / base) * 100.0)
    }
}

/// Atomically writes content to a file using a temporary file and rename.
fn atomic_write(file_path: &str, content: &str) -> Result<(), Box<dyn std::error::Error>> {
    let temp_file = format!("{}.tmp", file_path);
    fs::write(&temp_file, content)?;
    fs::rename(&temp_file, file_path)?;
    Ok(())
}

/// Fetches stock data from Tiingo for a given ticker, using caching.
/// The environment variable `TIINGO_API_KEY` is required.
fn run_tiingo_for_ticker(
    ticker: &str,
    config: &Config,
    client: &Client,
) -> Result<Value, Box<dyn std::error::Error>> {
    let stock_config = config.stock.as_ref().ok_or("Stock configuration missing")?;

    let api_key = env::var("TIINGO_API_KEY").map_err(|_| {
        "TIINGO_API_KEY environment variable not set. Please set it with your Tiingo API key."
    })?;

    if api_key.trim().is_empty() {
        return Err("TIINGO_API_KEY environment variable is empty".into());
    }

    let local_now = Local::now();
    let effective_cache_max_age =
        if local_now.weekday() == Weekday::Sat || local_now.weekday() == Weekday::Sun {
            stock_config.weekend_cache_max_age
        } else {
            stock_config.cache_max_age
        };

    let cache_file = format!("cache_{}.json", ticker);
    let use_cache = is_cache_valid(&cache_file, effective_cache_max_age);

    let tiingo_url = format!("https://api.tiingo.com/iex/{}", ticker);
    let response_text = if use_cache {
        fs::read_to_string(&cache_file)?
    } else {
        let response = client
            .get(&tiingo_url)
            .header(CONTENT_TYPE, "application/json")
            .header(AUTHORIZATION, format!("Token {}", api_key))
            .send()?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to fetch data from Tiingo for ticker {}: HTTP status {}",
                ticker,
                response.status()
            )
            .into());
        }

        let text = response.text()?;
        atomic_write(&cache_file, &text)?;
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
    let first_entry = json_data.get(0).ok_or_else(|| {
        format!(
            "Invalid API response for ticker {}: missing array element",
            ticker
        )
    })?;

    let last_price = first_entry
        .get("tngoLast")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| {
            format!(
                "Invalid tngoLast field for ticker {}: {:?}",
                ticker, first_entry
            )
        })?;

    let prev_close = first_entry
        .get("prevClose")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| {
            format!(
                "Invalid prevClose field for ticker {}: {:?}",
                ticker, first_entry
            )
        })?;

    let price_change_pct =
        calculate_percentage_change(last_price, prev_close).ok_or_else(|| {
            format!(
                "Previous close is zero for ticker {}, cannot calculate percentage change",
                ticker
            )
        })?;

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

/// Fetches crypto data from Kraken for a given trade pair,
/// using caching for both OHLC and ticker endpoints.
fn run_crypto_for_pair(
    pair: &str,
    sign: &str,
    config: &Config,
    client: &Client,
) -> Result<Value, Box<dyn std::error::Error>> {
    let crypto = config
        .crypto
        .as_ref()
        .ok_or("Crypto configuration missing")?;

    if crypto.trade_pairs.is_empty() {
        return Err("No crypto trade pairs configured".into());
    }

    let cache_max_age = crypto.cache_max_age;
    let cache_file_ohlc = format!("cache_crypto_{}_ohlc.json", pair);
    let cache_file_ticker = format!("cache_crypto_{}_ticker.json", pair);

    let use_cache_ohlc = is_cache_valid(&cache_file_ohlc, cache_max_age);
    let use_cache_ticker = is_cache_valid(&cache_file_ticker, cache_max_age);

    let kraken_api = "https://api.kraken.com/0/public";
    let ohlc_url = format!(
        "{}/OHLC?pair={}&interval={}",
        kraken_api, pair, crypto.chart_interval
    );
    let ticker_url = format!("{}/Ticker?pair={}", kraken_api, pair);

    let ohlc_text = if use_cache_ohlc {
        fs::read_to_string(&cache_file_ohlc)?
    } else {
        let response = client
            .get(&ohlc_url)
            .header("Accept", "application/json")
            .send()?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to fetch OHLC data from Kraken for pair {}: HTTP status {}",
                pair,
                response.status()
            )
            .into());
        }

        let text = response.text()?;
        atomic_write(&cache_file_ohlc, &text)?;
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
            return Err(format!(
                "Failed to fetch Ticker data from Kraken for pair {}: HTTP status {}",
                pair,
                response.status()
            )
            .into());
        }

        let text = response.text()?;
        atomic_write(&cache_file_ticker, &text)?;
        text
    };

    let ticker_json: Value = serde_json::from_str(&ticker_text)?;
    let current_value = ticker_json
        .get("result")
        .and_then(|r| r.get(pair))
        .and_then(|pair_data| pair_data.get("p"))
        .and_then(|p| p.get(0))
        .and_then(|val| val.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| format!("Could not retrieve current price for crypto pair {}", pair))?;

    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let yesterday_timestamp = now - SECONDS_PER_DAY;

    let ohlc_json: Value = serde_json::from_str(&ohlc_text)?;
    let candles = ohlc_json
        .get("result")
        .and_then(|r| r.get(pair))
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Could not retrieve OHLC candles array for pair {}", pair))?;

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
    let change_percentage_opt = calculate_percentage_change(current_value, old_vwap);

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

    // Format text: use sign if provided, otherwise use pair name
    let display_name = if sign.is_empty() {
        pair.to_string()
    } else {
        sign.to_string()
    };

    Ok(json!({
        "text": format!("{} €{} ({}%)", display_name, current_value_str, change_percentage_str),
        "tooltip": format!("€{} ({}%)", current_value_str, change_percentage_str),
        "class": status_class,
    }))
}

/// Runs ticker mode: displays a scrolling window of all instruments.
fn run_ticker_mode(
    config: &Config,
    filter_mode: Option<&str>,
    client: &Client,
) -> Result<(), Box<dyn std::error::Error>> {
    let ticker_config = config
        .ticker
        .as_ref()
        .ok_or("Ticker configuration missing. Add [ticker] section to config.toml")?;

    let position_file = ".ticker_position";
    let content_hash_file = ".ticker_content_hash";

    // Build ticker string
    let ticker_string = build_ticker_string(config, filter_mode, &ticker_config.separator, client)?;
    let ticker_length = get_plain_text_length(&ticker_string);

    if ticker_length == 0 {
        return Err("Ticker string is empty".into());
    }

    // Calculate hash of ticker content to detect changes
    let content_hash = format!("{:x}", Sha256::digest(ticker_string.as_bytes()));

    // Load previous position
    let mut position: usize = 0;
    let previous_hash = fs::read_to_string(content_hash_file).unwrap_or_default();

    // Only restore position if content hasn't changed
    if previous_hash.trim() == content_hash.trim() {
        if let Ok(pos_str) = fs::read_to_string(position_file) {
            position = pos_str.trim().parse().unwrap_or(0);
            // Ensure position is valid for current content
            if position >= ticker_length {
                position = 0;
            }
        }
    }

    // Output current window (raw markup for wrapper script to wrap in JSON)
    let window = get_ticker_window(&ticker_string, position, ticker_config.window_size);
    println!("{}", window);

    // Advance position and wrap around
    position = (position + 1) % ticker_length;

    // Save position and content hash
    if let Err(e) = fs::write(position_file, position.to_string()) {
        eprintln!("Failed to write {}: {}", position_file, e);
    }
    if let Err(e) = fs::write(content_hash_file, &content_hash) {
        eprintln!("Failed to write {}: {}", content_hash_file, e);
    }

    Ok(())
}

/// Builds the complete ticker string with all instruments and formatting.
fn build_ticker_string(
    config: &Config,
    filter_mode: Option<&str>,
    separator: &str,
    client: &Client,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut items = Vec::new();

    // Collect stock data
    if filter_mode.is_none() || filter_mode == Some("stock") {
        if let Some(stock) = &config.stock {
            for ticker in &stock.tickers {
                match run_tiingo_for_ticker(ticker, config, client) {
                    Ok(data) => {
                        let text = data["text"].as_str().unwrap_or("");
                        let class = data["class"].as_str().unwrap_or("up");
                        let color = get_color_for_class(class, config);
                        let escaped_text = escape_markup(text);
                        items.push(format!(
                            "<span color='{}'><b>{}</b></span>",
                            color, escaped_text
                        ));
                    }
                    Err(e) => eprintln!("Error fetching {}: {}", ticker, e),
                }
            }
        }
    }

    // Collect crypto data
    if filter_mode.is_none() || filter_mode == Some("crypto") {
        if let Some(crypto) = &config.crypto {
            for (i, pair) in crypto.trade_pairs.iter().enumerate() {
                let sign = crypto.trade_signs.get(i).map(|s| s.as_str()).unwrap_or("");
                match run_crypto_for_pair(pair, sign, config, client) {
                    Ok(data) => {
                        let text = data["text"].as_str().unwrap_or("");
                        let class = data["class"].as_str().unwrap_or("up");
                        let color = get_color_for_class(class, config);
                        let escaped_text = escape_markup(text);
                        items.push(format!(
                            "<span color='{}'><b>{}</b></span>",
                            color, escaped_text
                        ));
                    }
                    Err(e) => eprintln!("Error fetching {}: {}", pair, e),
                }
            }
        }
    }

    if items.is_empty() {
        return Err("No data available for ticker".into());
    }

    Ok(items.join(separator))
}

/// Gets the color for a given class from config.
fn get_color_for_class(class: &str, config: &Config) -> String {
    match class {
        "critdown" => config
            .thresholds
            .waydown_color
            .clone()
            .unwrap_or_else(|| "#800000".to_string()),
        "down" => config
            .thresholds
            .down_color
            .clone()
            .unwrap_or_else(|| "#FF0000".to_string()),
        "wayup" => config
            .thresholds
            .wayup_color
            .clone()
            .unwrap_or_else(|| "#008000".to_string()),
        _ => config
            .thresholds
            .up_color
            .clone()
            .unwrap_or_else(|| "#00FF00".to_string()),
    }
}

/// Escapes markup-sensitive characters so ticker text cannot inject markup.
fn escape_markup(s: &str) -> String {
    let mut escaped = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '\'' => escaped.push_str("&apos;"),
            '"' => escaped.push_str("&quot;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Extracts a window from the ticker string, handling wrapping and preserving complete markup.
fn get_ticker_window(full_string: &str, position: usize, window_size: usize) -> String {
    let plain_text = strip_markup(full_string);
    let plain_chars: Vec<char> = plain_text.chars().collect();
    let plain_len = plain_chars.len();

    if plain_len == 0 {
        return String::new();
    }

    // Build a mapping of plain text positions to their formatting
    let char_formats = collect_char_formats(full_string);

    let mut result = String::new();
    let mut last_color: Option<String> = None;

    for i in 0..window_size {
        let pos = (position + i) % plain_len;

        if let Some(format_info) = char_formats.get(&pos) {
            // Check if color changed
            if last_color != format_info.color {
                // Close previous format if any
                if last_color.is_some() {
                    result.push_str("</b></span>");
                }
                // Open new format
                if let Some(color) = &format_info.color {
                    result.push_str(&format!("<span color='{}'>", color));
                    result.push_str("<b>");
                }
                last_color = format_info.color.clone();
            }

            result.push(format_info.character);
        } else if let Some(ch) = plain_chars.get(pos) {
            // No format for this character, close any open tags
            if last_color.is_some() {
                result.push_str("</b></span>");
                last_color = None;
            }
            result.push(*ch);
        }
    }

    // Close any remaining open tags
    if last_color.is_some() {
        result.push_str("</b></span>");
    }

    result
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct FormatInfo {
    character: char,
    color: Option<String>,
}

/// Collects character format information from the markup string.
fn collect_char_formats(s: &str) -> std::collections::HashMap<usize, FormatInfo> {
    use std::collections::HashMap;

    let mut result: HashMap<usize, FormatInfo> = HashMap::new();
    let mut position = 0;
    let mut color_stack: Vec<String> = Vec::new();
    let mut current_color: Option<String> = None;
    let mut in_tag = false;
    let mut current_tag = String::new();

    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
            current_tag.clear();
            current_tag.push(ch);
        } else if ch == '>' {
            current_tag.push(ch);
            in_tag = false;

            let tag_lower = current_tag.to_ascii_lowercase();
            if tag_lower.starts_with("<span") {
                match extract_color_value_from_tag(&current_tag) {
                    Some(color) => {
                        color_stack.push(color.clone());
                        current_color = Some(color);
                    }
                    None => eprintln!("Malformed <span> tag encountered: {}", current_tag),
                }
            } else if tag_lower == "</span>" {
                if color_stack.pop().is_none() {
                    eprintln!("Unmatched </span> tag encountered in ticker text: {}", s);
                }
                current_color = color_stack.last().cloned();
            } else if tag_lower == "</b>" {
                // Ignored – bold tags do not affect color tracking.
            }
        } else if in_tag {
            current_tag.push(ch);
        } else {
            result.insert(
                position,
                FormatInfo {
                    character: ch,
                    color: current_color.clone(),
                },
            );
            position += 1;
        }
    }

    result
}

fn extract_color_value_from_tag(tag: &str) -> Option<String> {
    const COLOR_ATTR: &str = "color=";

    let attr_pos = tag.find(COLOR_ATTR)?;
    let mut rest = &tag[attr_pos + COLOR_ATTR.len()..];
    rest = rest.trim_start();

    let mut chars = rest.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        eprintln!("Color attribute missing quotes in tag: {}", tag);
        return None;
    }

    let remainder = chars.as_str();
    if let Some(end_idx) = remainder.find(quote) {
        Some(remainder[..end_idx].to_string())
    } else {
        eprintln!("Color attribute missing closing quote in tag: {}", tag);
        None
    }
}

/// Strips markup tags from a string to get plain text length.
fn strip_markup(s: &str) -> String {
    let mut result = String::new();
    let mut in_tag = false;

    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }

    result
}

/// Gets the plain text length (excluding markup).
fn get_plain_text_length(s: &str) -> usize {
    strip_markup(s).chars().count()
}
