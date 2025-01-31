# PumpFun Bot

PumpFun Bot is an all-in-one cryptocurrency monitoring and trading bot. It monitors the latest tokens migrated from the PumpFun API, performs security checks, conducts basic sentiment analysis, and integrates with a Telegram bot for alerts and (optional) trading commands.

## Features

- **Token Monitoring**: Real-time monitoring of tokens migrated from the PumpFun API.
- **Security Checks**: Perform blacklist checks and other security verifications.
- **Sentiment Analysis**: Basic sentiment analysis using TextBlob.
- **Telegram Integration**: Send alerts and receive trading commands via a Telegram bot.
- **Database Storage**: Store token and transaction data using SQLite.
- **Trading Analysis**: Identify suspicious transaction patterns using DBSCAN.

## Installation

Ensure you have Python 3.6 or higher installed. Then, install the required Python packages:

```
pip install requests web3 python-telegram-bot==20.* pandas sqlalchemy textblob scikit-learn hdbscan
```

## Usage

1. Create a `config.ini` file, using the following template as a guide:

    ```ini
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
    ```

2. Run the bot:

    ```bash
    python pumpfun_trading_robot.py
    ```

## Contributing

Contributions are welcome! Please fork this repository and submit a pull request.

## License

This project is licensed under the MIT License.

## Contact

For any questions or suggestions, please reach out via [GitHub Issues](https://github.com/EyescreenerFun/DeepSeek-Trade-Robot/issues).
```

