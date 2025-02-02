package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"io/ioutil"
	"log"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/ethereum/go-ethereum/common"
	tgbotapi "github.com/go-telegram-bot-api/telegram-bot-api/v5"
	"gopkg.in/ini.v1"
	_ "github.com/mattn/go-sqlite3"
)

// Config 结构体存储配置项
type Config struct {
	PumpfunKey           string
	InfuraKey            string
	EtherscanKey         string
	PollInterval         int
	MinLiquidity         float64
	MaxCreatorFee        float64
	MinHolders           int
	BlockNewCoinsMinutes int
	MaxCoinsPerCreator   int
	CoinAddresses        []string
	DevAddresses         []string
	TelegramBotToken     string
	TelegramChannelID    int64
}

// loadConfig 读取配置文件，如果不存在则写入一个示例并报错退出
func loadConfig(filename string) (*Config, error) {
	if _, err := os.Stat(filename); os.IsNotExist(err) {
		// 写入示例配置文件
		example := `[API]
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
`
		err := ioutil.WriteFile(filename, []byte(example), 0644)
		if err != nil {
			return nil, err
		}
		return nil, fmt.Errorf("没有找到 config.ini，已在 %s 处创建示例配置文件", filename)
	}
	cfg, err := ini.Load(filename)
	if err != nil {
		return nil, err
	}
	config := &Config{}
	config.PumpfunKey = cfg.Section("API").Key("PUMPFUN_KEY").String()
	config.InfuraKey = cfg.Section("API").Key("INFURA_KEY").String()
	config.EtherscanKey = cfg.Section("API").Key("ETHERSCAN_KEY").String()
	config.PollInterval, _ = cfg.Section("API").Key("POLL_INTERVAL").Int()

	config.MinLiquidity, _ = cfg.Section("FILTERS").Key("MIN_LIQUIDITY").Float64()
	config.MaxCreatorFee, _ = cfg.Section("FILTERS").Key("MAX_CREATOR_FEE").Float64()
	config.MinHolders, _ = cfg.Section("FILTERS").Key("MIN_HOLDERS").Int()
	config.BlockNewCoinsMinutes, _ = cfg.Section("FILTERS").Key("BLOCK_NEW_COINS_MINUTES").Int()
	config.MaxCoinsPerCreator, _ = cfg.Section("FILTERS").Key("MAX_COINS_PER_CREATOR").Int()

	coinAddresses := cfg.Section("BLACKLISTS").Key("COIN_ADDRESSES").String()
	config.CoinAddresses = strings.Split(coinAddresses, ",")
	devAddresses := cfg.Section("BLACKLISTS").Key("DEV_ADDRESSES").String()
	config.DevAddresses = strings.Split(devAddresses, ",")

	config.TelegramBotToken = cfg.Section("TELEGRAM").Key("BOT_TOKEN").String()
	config.TelegramChannelID, _ = cfg.Section("TELEGRAM").Key("CHANNEL_ID").Int64()

	return config, nil
}

// PumpFunBot 结构体保存程序运行所需的对象
type PumpFunBot struct {
	config          *Config
	db              *sql.DB
	telegramBot     *tgbotapi.BotAPI
	currentContract string
	apiBase         string
}

// CoinData 定义硬币数据结构
type CoinData struct {
	ContractAddress  string  `json:"contractAddress"`
	Name             string  `json:"name"`
	Symbol           string  `json:"symbol"`
	CreatorWallet    string  `json:"creator_wallet"`
	MigrationTime    time.Time
	InitialLiquidity float64 `json:"initial_liquidity"`
	CreatorFee       float64 `json:"creator_fee"`
	Holders          int     `json:"holders"`
}

// NewPumpFunBot 初始化 PumpFunBot 对象
func NewPumpFunBot(config *Config) (*PumpFunBot, error) {
	db, err := sql.Open("sqlite3", "./pumpfun.db")
	if err != nil {
		return nil, err
	}
	bot := &PumpFunBot{
		config:  config,
		db:      db,
		apiBase: "https://api.pump.fun",
	}
	err = bot.createTables()
	if err != nil {
		return nil, err
	}
	if config.TelegramBotToken != "" {
		telegramBot, err := tgbotapi.NewBotAPI(config.TelegramBotToken)
		if err != nil {
			log.Printf("Telegram bot 初始化失败: %v", err)
		} else {
			bot.telegramBot = telegramBot
		}
	}
	return bot, nil
}

// createTables 在 SQLite 中创建数据表
func (p *PumpFunBot) createTables() error {
	coinTable := `CREATE TABLE IF NOT EXISTS coins (
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
	);`
	_, err := p.db.Exec(coinTable)
	if err != nil {
		return err
	}
	// 其他数据表可按需创建……
	return nil
}

// fetchMigratedCoins 从 PumpFun API 获取最新迁移的币信息
func (p *PumpFunBot) fetchMigratedCoins(limit int) ([]map[string]interface{}, error) {
	url := fmt.Sprintf("%s/migrations?limit=%d&sort=desc", p.apiBase, limit)
	req, err := http.NewRequest("GET", url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Add("Authorization", "Bearer "+p.config.PumpfunKey)
	req.Header.Add("Content-Type", "application/json")
	client := &http.Client{Timeout: 10 * time.Second}
	resp, err := client.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()
	body, err := ioutil.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}
	var result map[string]interface{}
	err = json.Unmarshal(body, &result)
	if err != nil {
		return nil, err
	}
	data, ok := result["data"].([]interface{})
	if !ok {
		return nil, fmt.Errorf("返回数据格式错误")
	}
	coins := []map[string]interface{}{}
	for _, item := range data {
		if coin, ok := item.(map[string]interface{}); ok {
			coins = append(coins, coin)
		}
	}
	return coins, nil
}

// parseCoinData 将原始数据解析为 CoinData 结构
func (p *PumpFunBot) parseCoinData(raw map[string]interface{}) (*CoinData, error) {
	contractAddr, ok := raw["contractAddress"].(string)
	if !ok {
		return nil, fmt.Errorf("缺少 contractAddress 字段")
	}
	// 使用 go-ethereum 库将地址转换为 checksum 格式（此处为简单调用）
	contractAddr = common.HexToAddress(contractAddr).Hex()

	// token 字段解析
	token, ok := raw["token"].(map[string]interface{})
	if !ok {
		return nil, fmt.Errorf("缺少 token 字段")
	}
	name := token["name"].(string)
	symbol := token["symbol"].(string)
	creator := raw["creator"].(string)
	creatorWallet := common.HexToAddress(creator).Hex()

	migrationTimeStr, ok := raw["migrationTime"].(string)
	var migrationTime time.Time
	if ok {
		migrationTime, _ = time.Parse(time.RFC3339, migrationTimeStr)
	} else {
		migrationTime = time.Now()
	}
	initialLiquidity, _ := strconv.ParseFloat(fmt.Sprintf("%v", raw["initialLiquidity"]), 64)
	creatorFee, _ := strconv.ParseFloat(fmt.Sprintf("%v", raw["feePercentage"]), 64)
	holdersFloat, _ := strconv.ParseFloat(fmt.Sprintf("%v", raw["holderCount"]), 64)
	holders := int(holdersFloat)
	return &CoinData{
		ContractAddress:  contractAddr,
		Name:             name,
		Symbol:           symbol,
		CreatorWallet:    creatorWallet,
		MigrationTime:    migrationTime,
		InitialLiquidity: initialLiquidity,
		CreatorFee:       creatorFee,
		Holders:          holders,
	}, nil
}

// checkContractVerification 可选：检查合约是否在 Etherscan 上已验证（此处为 stub）
func (p *PumpFunBot) checkContractVerification(address string) bool {
	// 你可调用 Etherscan API 进行检查
	return false
}

// enhancedParseCoinData 扩展解析（添加合约验证信息等）
func (p *PumpFunBot) enhancedParseCoinData(raw map[string]interface{}) (*CoinData, error) {
	coin, err := p.parseCoinData(raw)
	if err != nil {
		return nil, err
	}
	coinVerified := p.checkContractVerification(coin.ContractAddress)
	_ = coinVerified // 如需要，可将验证信息加入结构
	return coin, nil
}

// isBlacklisted 判断硬币或创作者是否在黑名单中
func (p *PumpFunBot) isBlacklisted(coin *CoinData) bool {
	for _, addr := range p.config.CoinAddresses {
		if strings.TrimSpace(addr) == coin.ContractAddress {
			log.Printf("[SECURITY] 硬币 %s 被列入黑名单。", coin.ContractAddress)
			return true
		}
	}
	for _, addr := range p.config.DevAddresses {
		if strings.TrimSpace(addr) == coin.CreatorWallet {
			log.Printf("[SECURITY] 开发者 %s 被列入黑名单。", coin.CreatorWallet)
			return true
		}
	}
	// 如需判断单个创作者创建的币数量，可在此扩展查询数据库逻辑
	return false
}

// applyFilters 对币数据应用过滤规则
func (p *PumpFunBot) applyFilters(coin *CoinData) bool {
	if coin.InitialLiquidity < p.config.MinLiquidity {
		return false
	}
	if coin.CreatorFee > p.config.MaxCreatorFee {
		return false
	}
	if coin.Holders < p.config.MinHolders {
		return false
	}
	ageThreshold := time.Now().Add(-time.Duration(p.config.BlockNewCoinsMinutes) * time.Minute)
	if coin.MigrationTime.After(ageThreshold) {
		return false
	}
	return true
}

// performSecurityChecks 执行安全检查（例如调用外部 RugCheck API，此处仅作示例）
func (p *PumpFunBot) performSecurityChecks(coin *CoinData) {
	log.Printf("[SECURITY] 对 %s 执行安全检查", coin.ContractAddress)
	// 在此处实现安全检查逻辑
}

// saveCoin 保存币信息到数据库
func (p *PumpFunBot) saveCoin(coin *CoinData) {
	stmt, err := p.db.Prepare(`INSERT OR IGNORE INTO coins
		(contract_address, name, symbol, creator_wallet, migration_time, initial_liquidity, creator_fee, holders)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		log.Println(err)
		return
	}
	defer stmt.Close()
	_, err = stmt.Exec(coin.ContractAddress, coin.Name, coin.Symbol, coin.CreatorWallet, coin.MigrationTime, coin.InitialLiquidity, coin.CreatorFee, coin.Holders)
	if err != nil {
		log.Println(err)
	}
}

// analyzeCoin 对新币执行进一步分析（此处仅作示例）
func (p *PumpFunBot) analyzeCoin(coin *CoinData) {
	log.Printf("[ANALYSIS] 分析 %s (%s)...", coin.Symbol, coin.ContractAddress)
	// 可在此扩展对交易模式、情绪分析等的检测
}

// sendTelegramAlert 发送 Telegram 提醒消息
func (p *PumpFunBot) sendTelegramAlert(message string) {
	if p.telegramBot == nil || p.config.TelegramChannelID == 0 {
		log.Println("[TELEGRAM] Telegram 未配置。")
		return
	}
	msg := tgbotapi.NewMessage(p.config.TelegramChannelID, message)
	_, err := p.telegramBot.Send(msg)
	if err != nil {
		log.Printf("发送 Telegram 消息错误: %v", err)
	}
}

// monitorCoinsLoop 主循环：定时拉取新币、过滤、存储及发送提醒
func (p *PumpFunBot) monitorCoinsLoop() {
	ticker := time.NewTicker(time.Duration(p.config.PollInterval) * time.Second)
	for {
		select {
		case <-ticker.C:
			rawCoins, err := p.fetchMigratedCoins(10)
			if err != nil {
				log.Printf("[ERROR] %v", err)
				continue
			}
			for _, raw := range rawCoins {
				coin, err := p.enhancedParseCoinData(raw)
				if err != nil {
					log.Println(err)
					continue
				}
				if p.isBlacklisted(coin) {
					continue
				}
				p.performSecurityChecks(coin)
				if !p.applyFilters(coin) {
					continue
				}
				p.saveCoin(coin)
				p.analyzeCoin(coin)
				alertMsg := fmt.Sprintf("New coin found:\nSymbol: %s\nContract: %s\nLiquidity: %.2f\n",
					coin.Symbol, coin.ContractAddress, coin.InitialLiquidity)
				p.sendTelegramAlert(alertMsg)
				p.currentContract = coin.ContractAddress
			}
		}
	}
}

func main() {
	config, err := loadConfig("config.ini")
	if err != nil {
		log.Fatal(err)
	}
	bot, err := NewPumpFunBot(config)
	if err != nil {
		log.Fatal(err)
	}
	log.Println("PumpFunBot 正在运行……")
	bot.monitorCoinsLoop()
}
