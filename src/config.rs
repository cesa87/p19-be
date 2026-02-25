use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub database_url: String,
    pub jwt_secret: String,
    #[serde(default = "default_jwt_expiry")]
    pub jwt_expiry_hours: i64,
    // OANDA
    #[serde(default)]
    pub oanda_api_token: Option<String>,
    #[serde(default)]
    pub oanda_account_id: Option<String>,
    #[serde(default = "default_oanda_practice")]
    pub oanda_practice: bool,
    // Telegram
    #[serde(default)]
    pub telegram_bot_token: Option<String>,
    #[serde(default)]
    pub telegram_api_id: Option<i32>,
    #[serde(default)]
    pub telegram_api_hash: Option<String>,
    // OpenAI
    #[serde(default)]
    pub openai_api_key: Option<String>,
    // Finnhub (for economic calendar and news)
    #[serde(default)]
    pub finnhub_api_key: Option<String>,
    // Alpha Vantage (news sentiment + market data)
    #[serde(default)]
    pub alpha_vantage_api_key: Option<String>,
    // NewsAPI (primary news source with regime detection)
    #[serde(default)]
    pub newsapi_key: Option<String>,
    // Intelligence Engine
    #[serde(default)]
    pub whale_alert_api_key: Option<String>,
    // MRATE settings
    #[serde(default = "default_require_fred_for_gold")]
    pub require_fred_for_gold: bool,
}

fn default_require_fred_for_gold() -> bool {
    true // Real yield is critical for gold trading decisions
}

fn default_oanda_practice() -> bool {
    true
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    8080
}

fn default_jwt_expiry() -> i64 {
    24
}

impl Config {
    pub fn from_env() -> Result<Self, envy::Error> {
        envy::from_env::<Config>()
    }
}
