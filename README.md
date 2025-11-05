# Stock & Crypto Status Widget

This Rust program fetches market data from two sources and outputs a one‑line JSON object suitable for status bars or widgets:

- **Stocks (Tiingo API):**  
  The default mode rotates through a list of stock tickers, caches API responses (with different cache durations for weekdays and weekends), calculates percentage changes based on current and previous close prices, and classifies the result using globally defined thresholds.

- **Cryptocurrencies (Kraken API):**  
  When run with the `--crypto` flag, the program rotates through a list of crypto trade pairs, fetches OHLC and ticker data from Kraken, calculates percentage changes using yesterday’s candle data, and reuses the global thresholds for classification.

The output is a single-line JSON object with the following keys:
- **text:** A summary (ticker or symbol, price, and percentage change).
- **tooltip:** Additional details (such as cache age for stocks or current crypto info).
- **class:** A classification label (`critdown`, `down`, `up`, or `wayup`) based on configurable thresholds.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (Rust 1.50 or later recommended)
- Cargo (Rust’s package manager)

## Installation

1. **Clone the Repository:**

   ```bash
   git clone https://github.com/its-a-unixsystem/barstocker.git
   cd stock-crypto-status-widget
   ```

2. **Build the Project:**

   ```bash
   cargo build --release
   ```

## Configuration

The program uses a TOML configuration file. By default, it looks for a file named `config.toml` in the project root, but you can specify a different file path as the first argument when running the program.

### Example `config.toml`

```toml
# config.toml
rotation_seconds = 70

# --- Tiingo (Stock) Settings ---
[stock]
# Note: API key must be set via TIINGO_API_KEY environment variable
# Example: export TIINGO_API_KEY="your_api_key_here"
tickers = ["NVDA", "BRK-A", "SPY"]

# Cache settings (in seconds) for Tiingo data:
cache_max_age = 60           # Maximum cache age on weekdays
weekend_cache_max_age = 120  # Maximum cache age on Saturdays and Sundays

[thresholds]
# Global thresholds for classifying percentage change for both stocks and crypto:
# - If the percentage change is less than `down`, then:
#     * If it is also less than `critdown`, it is classified as "critdown"
#     * Otherwise, as "down"
# - If the percentage change is greater than `wayup`, it is classified as "wayup"
# - Otherwise, it is classified as "up"
critdown = -10.0
down = 0.0
wayup = 5.0

# --- Crypto (via Kraken) Settings ---
[crypto]
trade_pairs = ["DOTEUR", "TBTCEUR", "XETHZEUR"]
trade_signs = ["", "", "⟠"]
chart_interval = 5            # Candle interval in minutes
```

### Configuration Fields Explained

- **TIINGO_API_KEY (Environment Variable):**  
  Your Tiingo API key. **Required** for stock functionality. Two options:
  
  1. **Recommended**: Create a `.env.local` file (see `.env.example`)
  2. **Alternative**: Set environment variable directly:
     ```bash
     export TIINGO_API_KEY="your_actual_api_key_here"
     ```

- **tickers:**  
  A list of stock tickers to monitor. The program rotates through these based on `rotation_seconds`.

- **rotation_seconds (Stocks):**  
  The time interval (in seconds) for rotating through stock tickers.

- **cache_max_age / weekend_cache_max_age:**  
  The maximum allowed age (in seconds) for the cached stock API response on weekdays and weekends, respectively.

- **[thresholds]:**  
  Global thresholds used for both stocks and crypto to classify percentage changes:
  - **critdown:** If the percentage change is less than this value, the classification is `"critdown"`.
  - **down:** If the percentage change is less than 0 (but not below `critdown`), it is `"down"`.
  - **wayup:** If the percentage change is greater than this value, it is `"wayup"`.
  - Any change that doesn’t meet the above conditions is classified as `"up"`.

- **[crypto]:**  
  Settings for fetching cryptocurrency data from Kraken:
  - **trade_pairs:** A list of crypto trade pairs to monitor.
  - **trade_signs:** A list of corresponding symbols for display.
  - **rotation_seconds (Crypto):** The interval (in seconds) for rotating through crypto pairs.
  - **chart_interval:** The candle interval (in minutes) for Kraken OHLC data.

## Running the Program

### Setup

1. **Copy the example configuration:**
   ```bash
   cp config.toml.example config.toml
   ```

2. **Set your Tiingo API key:**
   
   **Option A: Using .env.local file (Recommended):**
   ```bash
   cp .env.example .env.local
   # Edit .env.local and replace 'your_api_key_here' with your actual key
   nano .env.local  # or use your preferred editor
   ```
   
   **Option B: Using environment variable:**
   ```bash
   export TIINGO_API_KEY="your_actual_api_key_here"
   ```
   
   Or add it to your shell profile (`~/.bashrc`, `~/.zshrc`, etc.) for persistence:
   ```bash
   echo 'export TIINGO_API_KEY="your_actual_api_key_here"' >> ~/.bashrc
   ```

### Stock (Tiingo) Mode (Default)

To run the program in stock mode (which is the default), simply execute:

```bash
cargo run --release
```

If your configuration file is located elsewhere or has a different name, specify its path as the first argument:

```bash
cargo run --release path/to/your/config.toml -- --crypto
```

## Output

The program outputs a single-line JSON object. For example:

```json
{"text":"NVDA $123.45 (2.34%)","tooltip":"Cache Age: 45 seconds (Max allowed: 60 seconds)","class":"up"}
```

- **text:** Displays the ticker (or crypto symbol), its price, and the percentage change.
- **tooltip:** Provides additional details (such as cache age for stocks).
- **class:** The classification (`critdown`, `down`, `up`, or `wayup`) based on the percentage change and global thresholds.

## Dependencies

This project uses the following Rust crates:

- [reqwest](https://crates.io/crates/reqwest) for HTTP requests.
- [serde](https://crates.io/crates/serde) and [serde_json](https://crates.io/crates/serde_json) for JSON (de)serialization.
- [toml](https://crates.io/crates/toml) for parsing the configuration file.
- [chrono](https://crates.io/crates/chrono) for date and time handling.
- [dotenvy](https://crates.io/crates/dotenvy) for loading environment variables from `.env.local`.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
