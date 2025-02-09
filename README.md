# Stock Status Widget

This is a Rust program that fetches stock price data from the [Tiingo API](https://api.tiingo.com/) and outputs a JSON object for consumption by status bars or widget tools. The program rotates through a list of tickers, caches API responses with configurable cache durations (with separate settings for weekdays and weekends), and classifies the price change with configurable thresholds.

## Features

- **Ticker Rotation:** Automatically rotates through a list of tickers based on a configurable time interval.
- **Configurable Caching:** Caches API responses to reduce API calls. Cache expiration is configurable—with different durations for weekdays and weekends.
- **Threshold Classification:** Classifies stock price changes using configurable thresholds into categories such as `critdown`, `down`, `up`, and `wayup`.
- **JSON Output:** Outputs JSON that includes the ticker, last price, percentage change, and cache age (with the effective maximum allowed cache age) in the tooltip.

## Requirements

- [Rust](https://www.rust-lang.org/tools/install)
- Cargo (Rust's package manager)

## Installation

1. **Clone the repository:**

   ```bash
   git clone https://github.com/its-a-unixsystem/desktop.git
   cd desktop/sway/stocker
   ```

2. **Build the project using Cargo:**

   ```bash
   cargo build --release
   ```

## Configuration

The program requires a configuration file in TOML format. Create a file named `config.toml` in the root of your project (or specify an alternate path as the first argument when running the program).

### Example `config.toml`

```toml
# config.toml

# Your Tiingo API key.
# You can obtain an API key by signing up at https://www.tiingo.com/.
api_key = "YOUR_TIINGO_API_KEY"

# List of tickers to monitor.
tickers = ["NVDA", "BRK-A", "SPY"]

# Ticker rotation interval in seconds.
rotation_seconds = 70

# Cache settings (in seconds):
# - cache_max_age: Maximum allowed age of the cache on weekdays.
# - weekend_cache_max_age: Maximum allowed age of the cache on Saturdays and Sundays.
cache_max_age = 60
weekend_cache_max_age = 120

[thresholds]
# Thresholds for classifying the percentage price change:
# - critdown: Price change below this value is considered a critical drop.
# - down:     Price change below 0 (but above critdown) is considered a drop.
# - wayup:    Price change above this value is considered a significant rise.
critdown = -10.0
down = 0.0
wayup = 5.0
```

### Configuration Fields Explained

- **api_key:**  
  Your Tiingo API key. Replace `"YOUR_TIINGO_API_KEY"` with your actual API key.

- **tickers:**  
  An array of stock tickers that the program will cycle through.

- **rotation_seconds:**  
  The interval (in seconds) after which the program rotates to the next ticker in the list.

- **cache_max_age:**  
  The maximum allowed age (in seconds) of the cached API response during weekdays.

- **weekend_cache_max_age:**  
  The maximum allowed age (in seconds) of the cached API response on Saturdays and Sundays.

- **[thresholds]:**  
  A table of thresholds used to classify the percentage change in stock price:
  - **critdown:** If the percentage change is below this value, the output is marked as `critdown`.
  - **down:** If the percentage change is below 0 (but not lower than `critdown`), the output is marked as `down`.
  - **wayup:** If the percentage change is above this value, the output is marked as `wayup`.
  - Any percentage change that does not fall into these categories is marked as `up`.

## Running the Program

To run the program with your configuration file, use the following command:

```bash
cargo run --release
```

If your configuration file is located somewhere else or you want to use a different file name, specify its path as the first argument:

```bash
cargo run --release -- path/to/your/config.toml
```

## Output

The program outputs a JSON object similar to the following:

```json
{
  "text": "NVDA $123.45 (2.34%)",
  "tooltip": "Cache Age: 45 seconds (Max allowed: 60 seconds)",
  "class": "up"
}
```

- **text:**  
  Displays the selected ticker, its latest price, and the percentage change.

- **tooltip:**  
  Shows the current cache age (in seconds) and the effective maximum allowed age (which varies on weekends and weekdays).

- **class:**  
  Indicates the classification of the price change based on the thresholds provided (`critdown`, `down`, `up`, or `wayup`).

## Dependencies

This project uses the following Rust crates:

- [reqwest](https://crates.io/crates/reqwest) for HTTP requests.
- [serde](https://crates.io/crates/serde) and [serde_json](https://crates.io/crates/serde_json) for JSON serialization and deserialization.
- [toml](https://crates.io/crates/toml) for reading the configuration file.
- [chrono](https://crates.io/crates/chrono) for date and time handling.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.