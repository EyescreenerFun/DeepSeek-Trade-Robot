#!/usr/bin/env python3
"""
PumpFun Bot - All-In-One 
--------------------------------
Monitors recently migrated coins from PumpFun API, performs security checks,
handles basic sentiment analysis, and integrates a Telegram bot for alerts
and (optional) trading commands.

Requirements:
    pip install requests web3 python-telegram-bot==20.* pandas sqlalchemy textblob scikit-learn hdbscan

Usage:
    1. Create a config.ini (see example below).
    2. python pumpfun_bot.py

"""

import os
import time
import json
import asyncio
import configparser
from datetime import datetime, timedelta

import requests
import numpy as np
import pandas as pd
from sqlalchemy import create_engine
from textblob import TextBlob

# Blockchain
from web3 import Web3

# Telegram Bot (async version)
from telegram import Bot, Update
from telegram.ext import (
    ApplicationBuilder,
    CommandHandler,
    MessageHandler,
    ContextTypes,
    filters
)

# Machine Learning
from sklearn.cluster import DBSCAN
from sklearn.ensemble import IsolationForest

######################################################################
# 1. CONFIGURATION
######################################################################

CONFIG_FILE = "config.ini"

EXAMPLE_CONFIG = """
[API]
PUMPFUN_KEY = your_pumpfun_api_key_here
INFURA_KEY = your_infura_key_here
ETHERSCAN_KEY = your_etherscan_key_here
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
COIN_BLACKLIST_URL = https://your.service/coin_blacklist
DEV_BLACKLIST_URL = https://your.service/dev_blacklist

[TWITTER]
# If you have a custom Twitter API or a service like TweetScout
API_KEY = your_twitter_api_key
RATE_LIMIT = 30

[TELEGRAM]
BOT_TOKEN = your_telegram_bot_token
CHANNEL_ID = your_telegram_channel_id

[TRADING]
DEFAULT_AMOUNT = 0.1
SLIPPAGE = 1.5
MAX_POSITION = 5.0
STOP_LOSS = -0.15
TAKE_PROFIT = 0.3

[SECURITY]
RUGCHECK_API = your_rugcheck_api_key
BUNDLED_THRESHOLD = 0.65
"""


class PumpFunBot:
    def __init__(self, config_path: str = CONFIG_FILE):
        """Initialize everything: config, DB, Web3, etc."""
        self.config = self.load_config(config_path)

        # Database
        self.db_path = self.config.get("DATABASE", "DB_PATH", fallback="pumpfun.db")
        self.db_engine = create_engine(f"sqlite:///{self.db_path}")

        self.create_tables()

        # Web3
        infura_key = self.config["API"].get("INFURA_KEY", "")
        self.w3 = Web3(
            Web3.HTTPProvider(f"https://mainnet.infura.io/v3/{infura_key}")
        )

        # PumpFun API settings
        self.api_base = "https://api.pump.fun"  # Example endpoint
        self.pumpfun_key = self.config["API"].get("PUMPFUN_KEY", "")
        self.headers = {
            'Authorization': f'Bearer {self.pumpfun_key}',
            'Content-Type': 'application/json'
        }

        # Filters
        self.filters = {
            'min_liquidity': self.config["FILTERS"].getfloat("MIN_LIQUIDITY", 5.0),
            'max_creator_fee': self.config["FILTERS"].getfloat("MAX_CREATOR_FEE", 10.0),
            'min_holders': self.config["FILTERS"].getint("MIN_HOLDERS", 25),
            'block_new_coins_minutes': self.config["FILTERS"].getint("BLOCK_NEW_COINS_MINUTES", 10),
            'max_coins_per_creator': self.config["FILTERS"].getint("MAX_COINS_PER_CREATOR", 3),
        }

        # Blacklist sets
        self.blacklisted_coins = self.load_blacklist("COIN_ADDRESSES")
        self.blacklisted_devs = self.load_blacklist("DEV_ADDRESSES")

        # Telegram
        self.telegram_token = self.config["TELEGRAM"].get("BOT_TOKEN", "")
        self.telegram_channel_id = self.config["TELEGRAM"].get("CHANNEL_ID", "")
        self.application = None  # Will initialize if Telegram token is present

        # For demonstration, we keep track of the "currently analyzed" contract
        self.currently_analyzed_contract = None

    ######################################################################
    # 2. CONFIG & DATABASE
    ######################################################################

    @staticmethod
    def load_config(path: str) -> configparser.ConfigParser:
        """Load config.ini or create an example if missing."""
        if not os.path.exists(path):
            with open(path, "w") as f:
                f.write(EXAMPLE_CONFIG)
            raise FileNotFoundError(
                f"No config.ini found. An example config has been created at {path}."
            )

        config = configparser.ConfigParser()
        config.read(path)
        return config

    def load_blacklist(self, key: str):
        """Load blacklisted addresses from config."""
        addresses_str = self.config["BLACKLISTS"].get(key, "")
        addresses = [addr.strip() for addr in addresses_str.split(",") if addr.strip()]
        # Convert to checksummed addresses if valid
        checksummed = set()
        for addr in addresses:
            try:
                checksummed.add(Web3.to_checksum_address(addr))
            except ValueError:
                # If it doesn't parse as an address, skip
                pass
        return checksummed

    def create_tables(self):
        """Initialize database schema if not exists."""
        with self.db_engine.connect() as conn:
            # Coins table
            conn.execute("""
                CREATE TABLE IF NOT EXISTS coins (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    contract_address TEXT UNIQUE,
                    name TEXT,
                    symbol TEXT,
                    creator_wallet TEXT,
                    migration_time DATETIME,
                    initial_liquidity REAL,
                    creator_fee FLOAT,
                    holders INTEGER,
                    social_score FLOAT,
                    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
                )
            """)

            # Transactions table
            conn.execute("""
                CREATE TABLE IF NOT EXISTS transactions (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    contract_address TEXT,
                    tx_hash TEXT UNIQUE,
                    direction TEXT,
                    amount_eth REAL,
                    gas_price REAL,
                    block_number INTEGER,
                    timestamp DATETIME
                )
            """)

            # Twitter metrics table
            conn.execute("""
                CREATE TABLE IF NOT EXISTS twitter_metrics (
                    coin_address TEXT PRIMARY KEY,
                    twitter_handle TEXT,
                    follower_count INTEGER,
                    following_count INTEGER,
                    sentiment_score REAL,
                    post_frequency REAL,
                    verified BOOLEAN,
                    account_age_days INTEGER
                )
            """)

            # Twitter posts table
            conn.execute("""
                CREATE TABLE IF NOT EXISTS twitter_posts (
                    post_id TEXT PRIMARY KEY,
                    coin_address TEXT,
                    content TEXT,
                    likes INTEGER,
                    retweets INTEGER,
                    timestamp DATETIME,
                    sentiment REAL,
                    hashtags TEXT,
                    links TEXT
                )
            """)

            # Security checks table
            conn.execute("""
                CREATE TABLE IF NOT EXISTS security_checks (
                    contract_address TEXT PRIMARY KEY,
                    rugcheck_score REAL,
                    rugcheck_verdict TEXT,
                    top_holder_percent REAL,
                    is_bundled BOOLEAN,
                    check_time DATETIME
                )
            """)

            # Trades table (if using trading features)
            conn.execute("""
                CREATE TABLE IF NOT EXISTS trades (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
                    direction TEXT,
                    contract_address TEXT,
                    amount REAL,
                    tx_hash TEXT,
                    profit REAL
                )
            """)

    ######################################################################
    # 3. DATA FETCHING & PARSING
    ######################################################################

    def fetch_migrated_coins(self, limit=100):
        """Retrieve recently migrated coins from PumpFun API."""
        try:
            params = {
                'limit': limit,
                'sort': 'desc'
            }
            url = f"{self.api_base}/migrations"
            resp = requests.get(url, headers=self.headers, params=params, timeout=10)
            resp.raise_for_status()
            data = resp.json()
            return data.get("data", [])
        except Exception as e:
            print(f"fetch_migrated_coins error: {e}")
            return []

    def parse_coin_data(self, raw_data):
        """Convert raw PumpFun API data into a consistent dictionary."""
        try:
            # Some data might not exist in raw_data; handle gracefully
            contract_address = raw_data.get("contractAddress", "")
            # Convert to checksum
            contract_address = Web3.to_checksum_address(contract_address)

            name = raw_data["token"].get("name", "Unknown")
            symbol = raw_data["token"].get("symbol", "UNK")
            creator = raw_data.get("creator", "0x0000000000000000000000000000000000000000")
            creator_wallet = Web3.to_checksum_address(creator)

            migration_time_str = raw_data.get("migrationTime", datetime.now().isoformat())
            migration_time = datetime.fromisoformat(migration_time_str)

            initial_liquidity = float(raw_data.get("initialLiquidity", 0))
            creator_fee = float(raw_data.get("feePercentage", 0))
            holders = int(raw_data.get("holderCount", 0))

            parsed = {
                "contract_address": contract_address,
                "name": name,
                "symbol": symbol,
                "creator_wallet": creator_wallet,
                "migration_time": migration_time,
                "initial_liquidity": initial_liquidity,
                "creator_fee": creator_fee,
                "holders": holders,
            }
            return parsed

        except Exception as e:
            print(f"parse_coin_data error: {e}")
            return {}

    def enhanced_parse_coin_data(self, raw_data):
        """
        Extended parse to add more fields (like contract verification).
        Adjust or remove if you do not have Etherscan or advanced checks.
        """
        parsed = self.parse_coin_data(raw_data)
        parsed["is_verified_contract"] = self.check_contract_verification(
            parsed["contract_address"]
        )
        return parsed

    def check_contract_verification(self, address: str) -> bool:
        """Check if contract is verified on Etherscan (optional)."""
        etherscan_key = self.config["API"].get("ETHERSCAN_KEY", "")
        if not etherscan_key or not address:
            return False
        try:
            url = (
                f"https://api.etherscan.io/api?"
                f"module=contract&action=getabi&address={address}&apikey={etherscan_key}"
            )
            resp = requests.get(url, timeout=5)
            data = resp.json()
            # If 'result' is a string error, contract might not be verified
            return bool(data.get("status") == "1")
        except Exception:
            return False

    ######################################################################
    # 4. SECURITY & FILTERS
    ######################################################################

    def is_blacklisted(self, coin_data) -> bool:
        """Check if coin or creator wallet is in our blacklists."""
        contract_address = coin_data.get("contract_address", "")
        creator_wallet = coin_data.get("creator_wallet", "")
        if not contract_address or not creator_wallet:
            return True

        if contract_address in self.blacklisted_coins:
            print(f"[SECURITY] Coin {contract_address} is blacklisted.")
            return True
        if creator_wallet in self.blacklisted_devs:
            print(f"[SECURITY] Dev {creator_wallet} is blacklisted.")
            return True

        # Could also implement "suspicious creator" checks:
        if self.is_suspicious_creator(creator_wallet):
            print(f"[SECURITY] Creator {creator_wallet} made too many coins.")
            return True

        return False

    def is_suspicious_creator(self, creator_wallet: str) -> bool:
        """Check how many coins a single creator has made."""
        query = f"""
            SELECT COUNT(*) as created_coins 
            FROM coins 
            WHERE creator_wallet = '{creator_wallet}'
        """
        df = pd.read_sql(query, self.db_engine)
        return df["created_coins"].iloc[0] > self.filters["max_coins_per_creator"]

    def apply_filters(self, coin_data):
        """Apply standard filters to exclude suspicious/low-quality coins."""
        if not coin_data:
            return False

        # Check liquidity, fee, holders, and age
        if coin_data["initial_liquidity"] < self.filters["min_liquidity"]:
            return False
        if coin_data["creator_fee"] > self.filters["max_creator_fee"]:
            return False
        if coin_data["holders"] < self.filters["min_holders"]:
            return False

        age_threshold = datetime.now() - timedelta(
            minutes=self.filters["block_new_coins_minutes"]
        )
        if coin_data["migration_time"] > age_threshold:
            # It's too new
            return False

        return True

    def perform_security_checks(self, coin_data: dict):
        """
        Placeholder for external security checks (e.g. RugCheck).
        If you don't have an external service, omit or stub this.
        """
        # Example: Mark 'top_holder_percent' or 'is_bundled' as False
        # because we don't have a standard way to get top holders
        security_data = {
            "contract_address": coin_data["contract_address"],
            "rugcheck_score": 100.0,  # Pretend it's safe
            "rugcheck_verdict": "Good",
            "top_holder_percent": 0.0,
            "is_bundled": False,
            "check_time": datetime.now()
        }

        # In a real scenario, you'd call the external API and parse:
        # rug_data = self.check_rugcheck_verdict(coin_data["contract_address"])
        # is_bundled = self.analyze_token_distribution(coin_data["contract_address"])

        # Then store in DB
        df = pd.DataFrame([security_data])
        df.to_sql("security_checks", self.db_engine, if_exists="replace", index=False)
        return security_data

    ######################################################################
    # 5. STORING & ANALYZING DATA
    ######################################################################

    def save_coins(self, parsed_coin: dict):
        """Insert or update coin data in the 'coins' table."""
        if not parsed_coin:
            return
        df = pd.DataFrame([parsed_coin])
        df.to_sql("coins", self.db_engine, if_exists="append", index=False)

    def analyze_transaction_patterns(self):
        """Example of identifying suspicious transactions using DBSCAN."""
        query = """
            SELECT contract_address, 
                   COUNT(*) as tx_count,
                   SUM(amount_eth) as total_volume,
                   AVG(gas_price) as avg_gas
            FROM transactions
            GROUP BY contract_address
        """
        df = pd.read_sql(query, self.db_engine)
        if df.empty:
            return pd.DataFrame()

        X = df[["tx_count", "total_volume", "avg_gas"]].fillna(0)
        clustering = DBSCAN(eps=0.5, min_samples=3).fit(X)
        df["cluster"] = clustering.labels_
        # Outliers have label == -1
        return df[df["cluster"] == -1]

    def sentiment_analysis_example(self, text: str) -> float:
        """Basic sentiment polarity with TextBlob."""
        if not text:
            return 0.0
        analysis = TextBlob(text)
        return analysis.sentiment.polarity

    def analyze_coin(self, coin_data: dict):
        """
        Placeholder for additional analysis steps on a new coin:
        e.g. analyzing sentiment, transaction patterns, liquidity, etc.
        """
        print(f"[ANALYSIS] Analyzing {coin_data['symbol']} ({coin_data['contract_address']})...")
        # ...
        outliers = self.analyze_transaction_patterns()
        # If outliers exist, you might do some alerting or extra checks
        if not outliers.empty:
            print(f"[WARNING] Transaction outliers detected: {outliers.to_dict('records')}")

    ######################################################################
    # 6. TELEGRAM BOT (Optional)
    ######################################################################

    def setup_telegram_bot(self):
        """Initialize Telegram application if a bot token is configured."""
        if not self.telegram_token:
            print("[TELEGRAM] No bot token provided. Telegram bot is disabled.")
            return

        self.application = ApplicationBuilder().token(self.telegram_token).build()

        self.application.add_handler(CommandHandler("start", self.cmd_start))
        self.application.add_handler(CommandHandler("buy", self.cmd_buy))
        self.application.add_handler(CommandHandler("sell", self.cmd_sell))
        self.application.add_handler(MessageHandler(filters.TEXT, self.cmd_handle_text))

    async def cmd_start(self, update: Update, context: ContextTypes.DEFAULT_TYPE):
        """Handle /start command."""
        await update.message.reply_text(
            "Welcome to PumpFun Bot!\n"
            "Commands:\n"
            "/buy [amount] - Execute a mock buy.\n"
            "/sell [amount] - Execute a mock sell.\n"
        )

    async def cmd_buy(self, update: Update, context: ContextTypes.DEFAULT_TYPE):
        """Mock buy command."""
        try:
            args = context.args
            amount = float(args[0]) if args else 0.1
            message = f"Buying {amount} of {self.currently_analyzed_contract} (mock)..."
            await update.message.reply_text(message)
            # Here you'd call your actual trading code
        except Exception as e:
            await update.message.reply_text(f"Error: {str(e)}")

    async def cmd_sell(self, update: Update, context: ContextTypes.DEFAULT_TYPE):
        """Mock sell command."""
        try:
            args = context.args
            amount = float(args[0]) if args else 0.1
            message = f"Selling {amount} of {self.currently_analyzed_contract} (mock)..."
            await update.message.reply_text(message)
            # Here you'd call your actual trading code
        except Exception as e:
            await update.message.reply_text(f"Error: {str(e)}")

    async def cmd_handle_text(self, update: Update, context: ContextTypes.DEFAULT_TYPE):
        """Handle any text message not matching a command."""
        await update.message.reply_text("Use /buy or /sell commands, or /start for help.")

    async def send_telegram_alert(self, message: str):
        """Send a message to the configured Telegram channel."""
        if not self.telegram_channel_id:
            print("[TELEGRAM] No CHANNEL_ID configured.")
            return
        if not self.application:
            print("[TELEGRAM] Telegram bot is not initialized.")
            return
        bot: Bot = self.application.bot
        await bot.send_message(chat_id=self.telegram_channel_id, text=message)

    ######################################################################
    # 7. MAIN LOOP
    ######################################################################

    async def monitor_coins_loop(self):
        """Asynchronous loop that fetches, filters, and analyzes new coins."""
        poll_interval = self.config["API"].getint("POLL_INTERVAL", 60)
        while True:
            try:
                raw_coins = self.fetch_migrated_coins(limit=10)
                for raw_coin in raw_coins:
                    parsed = self.enhanced_parse_coin_data(raw_coin)

                    # Security checks
                    if self.is_blacklisted(parsed):
                        continue
                    self.perform_security_checks(parsed)

                    # Filter checks
                    if not self.apply_filters(parsed):
                        continue

                    # Save to DB
                    self.save_coins(parsed)

                    # Perform further analysis
                    self.analyze_coin(parsed)

                    # Example: send a Telegram alert if desired
                    if self.application:
                        msg = (f"New coin found:\n"
                               f"Symbol: {parsed['symbol']}\n"
                               f"Contract: {parsed['contract_address']}\n"
                               f"Liquidity: {parsed['initial_liquidity']}\n")
                        await self.send_telegram_alert(msg)

                    # Keep track of the "current" coin for /buy /sell
                    self.currently_analyzed_contract = parsed["contract_address"]

                await asyncio.sleep(poll_interval)

            except Exception as e:
                print(f"[ERROR] {e}")
                await asyncio.sleep(60)

    def run(self):
        """Entry point to run the bot. If Telegram is configured, run async."""
        self.setup_telegram_bot()

        if self.application:
            # If Telegram is available, run the async loop
            loop = asyncio.get_event_loop()
            loop.create_task(self.monitor_coins_loop())
            loop.create_task(self.application.initialize())
            loop.create_task(self.application.start_polling())
            try:
                loop.run_forever()
            except KeyboardInterrupt:
                print("[SHUTDOWN] Stopping Telegram bot...")
                loop.run_until_complete(self.application.shutdown())
                loop.run_until_complete(self.application.stop())
                loop.close()
        else:
            # Run a simple synchronous loop if Telegram not configured
            print("[INFO] Telegram not configured; running simple loop.")
            while True:
                try:
                    raw_coins = self.fetch_migrated_coins(limit=10)
                    for raw_coin in raw_coins:
                        parsed = self.enhanced_parse_coin_data(raw_coin)
                        if self.is_blacklisted(parsed):
                            continue
                        self.perform_security_checks(parsed)
                        if not self.apply_filters(parsed):
                            continue
                        self.save_coins(parsed)
                        self.analyze_coin(parsed)
                    time.sleep(self.config["API"].getint("POLL_INTERVAL", 60))
                except Exception as e:
                    print(f"[ERROR] {e}")
                    time.sleep(60)


######################################################################
# 8. MAIN ENTRY POINT
######################################################################

if __name__ == "__main__":
    bot = PumpFunBot()
    bot.run()