use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::time::sleep;

#[derive(Debug, Clone)]
struct Config {
    pumpfun_key: String,
    infura_key: String,
    etherscan_key: String,
    poll_interval: u64,
    min_liquidity: f64,
    max_creator_fee: f64,
    min_holders: i64,
    block_new_coins_minutes: u64,
    max_coins_per_creator: i64,
    coin_addresses: Vec<String>,
    dev_addresses: Vec<String>,
    telegram_bot_token: String,
    telegram_channel_id: i64,
}

impl Config {
    fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        if !Path::new(path).exists() {
            let example = r#"[API]
PUMPFUN_KEY = your_pumpfun_api_key_here
INFURA_KEY = your_infura_key_here
ETHERSCAN_KEY = your_etherscan_api_key_here
POLL_INTERVAL = 60

[FILTERS]
MIN_LIQUIDITY = 5.0
MAX_CREATOR_FEE = 10.0
MIN_HOLDERS = 25
BLOCK_NEW_COINS_MINUTES = 10
MAX_COINS_PER_CREATOR = 3

[BLACKLISTS]
COIN_ADDRESSES = 0x0000000000000000000000000000000000000000
DEV_ADDRESSES = 0x0000000000000000000000000000000000000000

[TELEGRAM]
BOT_TOKEN = your_telegram_bot_token
CHANNEL_ID = 123456789
"#;
            fs::write(path, example)?;
            return Err(format!("Config file not found. Example created at {}", path).into());
        }

        let contents = fs::read_to_string(path)?;
        let ini = ini::Ini::load_from_str(&contents)?;
        let api_section = ini.section(Some("API")).unwrap();
        let filters_section = ini.section(Some("FILTERS")).unwrap();
        let blacklists_section = ini.section(Some("BLACKLISTS")).unwrap();
        let telegram_section = ini.section(Some("TELEGRAM")).unwrap();

        let config = Config {
            pumpfun_key: api_section.get("PUMPFUN_KEY").unwrap_or("").to_string(),
            infura_key: api_section.get("INFURA_KEY").unwrap_or("").to_string(),
            etherscan_key: api_section.get("ETHERSCAN_KEY").unwrap_or("").to_string(),
            poll_interval: api_section.get("POLL_INTERVAL").unwrap_or("60").parse()?,
            min_liquidity: filters_section.get("MIN_LIQUIDITY").unwrap_or("5.0").parse()?,
            max_creator_fee: filters_section.get("MAX_CREATOR_FEE").unwrap_or("10.0").parse()?,
            min_holders: filters_section.get("MIN_HOLDERS").unwrap_or("25").parse()?,
            block_new_coins_minutes: filters_section.get("BLOCK_NEW_COINS_MINUTES").unwrap_or("10").parse()?,
            max_coins_per_creator: filters_section.get("MAX_COINS_PER_CREATOR").unwrap_or("3").parse()?,
            coin_addresses: blacklists_section.get("COIN_ADDRESSES").unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            dev_addresses: blacklists_section.get("DEV_ADDRESSES").unwrap_or("")
                .split(',')
                .map(|s| s.trim().to_string())
                .collect(),
            telegram_bot_token: telegram_section.get("BOT_TOKEN").unwrap_or("").to_string(),
            telegram_channel_id: telegram_section.get("CHANNEL_ID").unwrap_or("0").parse()?,
        };
        Ok(config)
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct CoinToken {
    name: Option<String>,
    symbol: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RawCoinData {
    #[serde(rename = "contractAddress")]
    contract_address: Option<String>,
    token: Option<CoinToken>,
    creator: Option<String>,
    #[serde(rename = "migrationTime")]
    migration_time: Option<String>,
    #[serde(rename = "initialLiquidity")]
    initial_liquidity: Option<f64>,
    #[serde(rename = "feePercentage")]
    creator_fee: Option<f64>,
    #[serde(rename = "holderCount")]
    holder_count: Option<i64>,
}

#[derive(Debug)]
struct CoinData {
    contract_address: String,
    name: String,
    symbol: String,
    creator_wallet: String,
    migration_time: SystemTime,
    initial_liquidity: f64,
    creator_fee: f64,
    holders: i64,
}

struct PumpFunBot {
    config: Config,
    db: Connection,
    current_contract: String,
    api_base: String,
}

impl PumpFunBot {
    fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let db = Connection::open("pumpfun.db")?;
        let bot = PumpFunBot {
            config,
            db,
            current_contract: String::new(),
            api_base: "https://api.pump.fun".to_string(),
        };
        bot.create_tables()?;
        Ok(bot)
    }

    fn create_tables(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.db.execute(
            "CREATE TABLE IF NOT EXISTS coins (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                contract_address TEXT UNIQUE,
                name TEXT,
                symbol TEXT,
                creator_wallet TEXT,
                migration_time DATETIME,
                initial_liquidity REAL,
                creator_fee REAL,
                holders INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;
        Ok(())
    }

    async fn fetch_migrated_coins(&self, limit: u64) -> Result<Vec<RawCoinData>, Box<dyn std::error::Error>> {
        let url = format!("{}/migrations?limit={}&sort=desc", self.api_base, limit);
        let client = reqwest::Client::new();
        let res = client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.config.pumpfun_key))
            .header("Content-Type", "application/json")
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        let json: Value = res.json().await?;
        let data = json.get("data").ok_or("Missing data field")?;
        let coins: Vec<RawCoinData> = serde_json::from_value(data.clone())?;
        Ok(coins)
    }

    fn parse_coin_data(&self, raw: &RawCoinData) -> Option<CoinData> {
        let contract_address = raw.contract_address.as_ref()?.to_string();
        // 此处可以使用 web3 crate 将地址转换为 checksum 格式
        let token = raw.token.as_ref()?;
        let name = token.name.clone().unwrap_or("Unknown".to_string());
        let symbol = token.symbol.clone().unwrap_or("UNK".to_string());
        let creator = raw.creator.as_ref().unwrap_or(&"0x0000000000000000000000000000000000000000".to_string()).to_string();
        let migration_time_str = raw.migration_time.as_ref().unwrap_or(&"".to_string());
        let migration_time = chrono::DateTime::parse_from_rfc3339(migration_time_str)
            .map(|dt| SystemTime::from(dt))
            .unwrap_or(SystemTime::now());
        let initial_liquidity = raw.initial_liquidity.unwrap_or(0.0);
        let creator_fee = raw.creator_fee.unwrap_or(0.0);
        let holders = raw.holder_count.unwrap_or(0);
        Some(CoinData {
            contract_address,
            name,
            symbol,
            creator_wallet: creator, // 可扩展为 checksum 地址
            migration_time,
            initial_liquidity,
            creator_fee,
            holders,
        })
    }

    fn is_blacklisted(&self, coin: &CoinData) -> bool {
        for addr in &self.config.coin_addresses {
            if addr.trim() == coin.contract_address {
                println!("[SECURITY] Coin {} is blacklisted.", coin.contract_address);
                return true;
            }
        }
        for addr in &self.config.dev_addresses {
            if addr.trim() == coin.creator_wallet {
                println!("[SECURITY] Dev {} is blacklisted.", coin.creator_wallet);
                return true;
            }
        }
        false
    }

    fn apply_filters(&self, coin: &CoinData) -> bool {
        if coin.initial_liquidity < self.config.min_liquidity {
            return false;
        }
        if coin.creator_fee > self.config.max_creator_fee {
            return false;
        }
        if coin.holders < self.config.min_holders {
            return false;
        }
        if let Ok(elapsed) = coin.migration_time.elapsed() {
            if elapsed < Duration::from_secs(self.config.block_new_coins_minutes * 60) {
                return false;
            }
        }
        true
    }

    fn perform_security_checks(&self, coin: &CoinData) {
        println!("[SECURITY] Performing security checks for {}", coin.contract_address);
        // 此处添加外部安全检查逻辑
    }

    fn save_coin(&self, coin: &CoinData) {
        let _ = self.db.execute(
            "INSERT OR IGNORE INTO coins (contract_address, name, symbol, creator_wallet, migration_time, initial_liquidity, creator_fee, holders)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                coin.contract_address,
                coin.name,
                coin.symbol,
                coin.creator_wallet,
                coin.migration_time.duration_since(UNIX_EPOCH).unwrap().as_secs(),
                coin.initial_liquidity,
                coin.creator_fee,
                coin.holders
            ],
        );
    }

    fn analyze_coin(&self, coin: &CoinData) {
        println!("[ANALYSIS] Analyzing {} ({})...", coin.symbol, coin.contract_address);
        // 这里可以添加对交易模式、情绪分析等的扩展逻辑
    }

    async fn send_telegram_alert(&self, message: &str) {
        if self.config.telegram_bot_token.is_empty() || self.config.telegram_channel_id == 0 {
            println!("[TELEGRAM] Telegram not configured.");
            return;
        }
        // 简单通过 HTTP GET 调用 Telegram Bot API 发送消息（也可使用 teloxide 库）
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage?chat_id={}&text={}",
            self.config.telegram_bot_token,
            self.config.telegram_channel_id,
            urlencoding::encode(message)
        );
        let _ = reqwest::get(&url).await;
    }

    async fn monitor_coins_loop(&mut self) {
        loop {
            match self.fetch_migrated_coins(10).await {
                Ok(raw_coins) => {
                    for raw in raw_coins.iter() {
                        if let Some(coin) = self.parse_coin_data(raw) {
                            if self.is_blacklisted(&coin) {
                                continue;
                            }
                            self.perform_security_checks(&coin);
                            if !self.apply_filters(&coin) {
                                continue;
                            }
                            self.save_coin(&coin);
                            self.analyze_coin(&coin);
                            let alert = format!(
                                "New coin found:\nSymbol: {}\nContract: {}\nLiquidity: {:.2}\n",
                                coin.symbol, coin.contract_address, coin.initial_liquidity
                            );
                            self.send_telegram_alert(&alert).await;
                            self.current_contract = coin.contract_address.clone();
                        }
                    }
                }
                Err(e) => {
                    println!("[ERROR] {}", e);
                }
            }
            sleep(Duration::from_secs(self.config.poll_interval)).await;
        }
    }
}

#[tokio::main]
async fn main() {
    let config = match Config::load("config.ini") {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    let mut bot = match PumpFunBot::new(config) {
        Ok(bot) => bot,
        Err(e) => {
            eprintln!("Failed to initialize bot: {}", e);
            return;
        }
    };
    println!("PumpFunBot is running...");
    bot.monitor_coins_loop().await;
}
