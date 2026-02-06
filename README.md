# Stock & Crypto Status Widget

This Rust program fetches market data from two sources and outputs formatted data suitable for status bars or widgets:

- **Stocks (Tiingo API):**  
  Fetches stock data, caches API responses (with different cache durations for weekdays and weekends), calculates percentage changes based on current and previous close prices, and classifies the result using globally defined thresholds.

- **Cryptocurrencies (Kraken API):**  
  Fetches crypto data from Kraken, retrieves OHLC and ticker data, calculates percentage changes using yesterday's candle data, and reuses the global thresholds for classification.

## Operating Modes

The program supports three operating modes:

1. **Single Output Mode (Default):** Outputs a one-line JSON object for the current instrument based on rotation timing.
2. **Continuous Mode (`--continuous`):** Continuously rotates through instruments, outputting JSON for each at regular intervals.
3. **Ticker Mode (`--ticker`):** Runs as a long-lived process, displaying a scrolling ticker window with all instruments. Outputs one JSON line per second with Pango markup in the `text` field. Data is refreshed from APIs every `refresh_seconds`.

### Output Format

**Single/Continuous Mode:** JSON object with the following keys:
- **text:** A summary (ticker or symbol, price, and percentage change).
- **tooltip:** Additional details (such as cache age for stocks or current crypto info).
- **class:** A classification label (`critdown`, `down`, `up`, or `wayup`) based on configurable thresholds.

**Ticker Mode:** JSON lines (one per second) with a `text` field containing Pango markup showing a scrolling window of all instruments with color-coded formatting. Runs as a long-lived process.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install) (Rust 1.50 or later recommended)
- Cargo (Rust’s package manager)

## Installation

1. **Clone the Repository:**

   ```bash
   git clone https://github.com/its-a-unixsystem/barstocker.git
   cd barstocker
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

# Optional: Custom colors for ticker mode (hex color codes)
# up_color = "#00FF00"
# wayup_color = "#008000"
# down_color = "#FF0000"
# waydown_color = "#800000"

# --- Crypto (via Kraken) Settings ---
[crypto]
trade_pairs = ["DOTEUR", "TBTCEUR", "XETHZEUR"]
trade_signs = ["DOT", "₿", "⟠"]  # Symbols shown before price. Use "" for pair name fallback
chart_interval = 5            # Candle interval in minutes
cache_max_age = 120           # Maximum cache age for crypto data

# --- Ticker Mode Settings (optional, required for --ticker mode) ---
[ticker]
window_size = 50              # Number of visible characters in the scrolling window
separator = " - "             # Separator between instruments
refresh_seconds = 60          # How often to re-fetch data from APIs (in seconds)
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
  - **trade_signs:** A list of corresponding symbols for display. Use empty string `""` to display the pair name instead.
  - **chart_interval:** The candle interval (in minutes) for Kraken OHLC data.
  - **cache_max_age:** Maximum cache age (in seconds) for crypto data.

- **[thresholds] - Color Customization (Optional):**  
  Custom hex color codes for ticker mode display:
  - **up_color:** Color for positive changes below `wayup` threshold (default: `#00FF00`).
  - **wayup_color:** Color for changes above `wayup` threshold (default: `#008000`).
  - **down_color:** Color for negative changes above `critdown` threshold (default: `#FF0000`).
  - **waydown_color:** Color for changes below `critdown` threshold (default: `#800000`).

- **[ticker] (Optional, required for `--ticker` mode):**
  Settings for ticker mode display:
  - **window_size:** Number of visible characters in the scrolling window.
  - **separator:** Text separator between instruments (e.g., `" - "`).
  - **refresh_seconds:** How often (in seconds) to re-fetch data from APIs. The ticker scrolls every second regardless of this value.

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

### Command-Line Usage

The program supports several command-line flags to control its behavior:

#### Single Output Mode (Default)

Outputs one JSON object for the current instrument based on rotation timing:

```bash
cargo run --release
```

#### Continuous Mode

Continuously rotates through instruments, outputting JSON at regular intervals:

```bash
cargo run --release -- --continuous
```

#### Ticker Mode

Runs a long-lived process that scrolls a ticker window of all instruments, outputting one JSON line per second:

```bash
cargo run --release -- --ticker
```

#### Filter Modes

Restrict output to only stocks or only crypto (works with all modes):

```bash
# Show only stocks
cargo run --release -- --stock

# Show only crypto
cargo run --release -- --crypto

# Combine with ticker mode
cargo run --release -- --ticker --crypto
```

#### Custom Configuration File

Specify a different configuration file path:

```bash
cargo run --release path/to/your/config.toml
cargo run --release path/to/your/config.toml -- --continuous
```

## Output

### Single/Continuous Mode Output

The program outputs a single-line JSON object. For example:

**Stock:**
```json
{"text":"NVDA $123.45 (2.34%)","tooltip":"Cache Age: 45 seconds (Max allowed: 60 seconds)","class":"up"}
```

**Crypto:**
```json
{"text":"₿ €45678.90 (1.23%)","tooltip":"€45678.90 (1.23%)","class":"up"}
```

- **text:** Displays the ticker (or crypto symbol), its price, and the percentage change.
- **tooltip:** Provides additional details (such as cache age for stocks or current price for crypto).
- **class:** The classification (`critdown`, `down`, `up`, or `wayup`) based on the percentage change and global thresholds.

### Ticker Mode Output

Ticker mode runs as a long-lived process, outputting one JSON line per second. Each line contains Pango markup in the `text` field:

```json
{"text":"<span color='#00FF00'><b>NVDA $123.45 (2.34%)</b></span> - <span color='#008000'><b>₿ €45678.90 (5.67%)</b></span>","tooltip":"Stock Ticker","class":"ticker"}
```

The scrolling window advances one character per second. Data is re-fetched from APIs every `refresh_seconds` (configured in `[ticker]`).

#### Waybar Integration

In waybar, use `restart-interval` instead of `interval` so the process runs continuously:

```json
"custom/stock": {
    "exec": "stocker --ticker",
    "restart-interval": 60,
    "return-type": "json",
    "on-click": ""
}
```

## Dependencies

This project uses the following Rust crates:

- [reqwest](https://crates.io/crates/reqwest) for HTTP requests.
- [serde](https://crates.io/crates/serde) and [serde_json](https://crates.io/crates/serde_json) for JSON (de)serialization.
- [toml](https://crates.io/crates/toml) for parsing the configuration file.
- [chrono](https://crates.io/crates/chrono) for date and time handling.
- [dotenvy](https://crates.io/crates/dotenvy) for loading environment variables from `.env.local`.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
