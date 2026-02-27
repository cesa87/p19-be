//! Markets — fetches live prediction market data from Polymarket Gamma API
//! Endpoint: GET /api/intelligence/markets?category=&sort=volume&limit=50

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ─── Public types (returned to frontend) ─────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketOutcome {
    pub name: String,
    /// Probability 0.0–1.0
    pub price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketCard {
    /// Polymarket hex condition_id (0x...) — used by CLOB API for trading
    pub condition_id: String,
    pub id: String,
    pub title: String,
    pub category: String,
    pub outcomes: Vec<MarketOutcome>,
    pub volume_usd: f64,
    pub liquidity_usd: f64,
    pub end_date: Option<String>,
    pub source: String,
    pub url: Option<String>,
    pub image_url: Option<String>,
}

// ─── Raw Polymarket Gamma API types ──────────────────────────────────────────

// Minimal event reference embedded in each market response
#[derive(Debug, Deserialize)]
struct RawEventRef {
    slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawMarket {
    id: Option<serde_json::Value>,
    question: Option<String>,
    #[serde(rename = "groupItemTitle")]
    group_item_title: Option<String>,
    category: Option<String>,
    outcomes: Option<serde_json::Value>,
    #[serde(rename = "outcomePrices")]
    outcome_prices: Option<serde_json::Value>,
    volume: Option<serde_json::Value>,
    liquidity: Option<serde_json::Value>,
    #[serde(rename = "endDate")]
    end_date: Option<String>,
    #[serde(rename = "conditionId")]
    condition_id: Option<String>,
    slug: Option<String>,
    image: Option<String>,
    active: Option<bool>,
    closed: Option<bool>,
    /// Parent event(s) — we use events[0].slug for the Polymarket URL
    events: Option<Vec<RawEventRef>>,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a JSON value that could be a float, string float, or null.
fn parse_f64(v: &serde_json::Value) -> f64 {
    match v {
        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
        serde_json::Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

/// Parse outcomes/prices — Polymarket returns these as JSON-encoded strings
/// e.g. "[\"Yes\",\"No\"]" or already as an array.
fn parse_string_array(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::Array(arr) => arr
            .iter()
            .map(|x| match x {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string().trim_matches('"').to_string(),
            })
            .collect(),
        serde_json::Value::String(s) => {
            // Try to parse as JSON array
            serde_json::from_str::<Vec<String>>(s).unwrap_or_default()
        }
        _ => vec![],
    }
}

fn build_polymarket_url(market_id: &str, slug: Option<&str>) -> String {
    match slug.filter(|s| !s.is_empty()) {
        Some(s) => format!("https://polymarket.com/event/{}", s),
        None    => format!("https://polymarket.com/event/{}", market_id),
    }
}

impl RawMarket {
    fn into_card(self, event_slug: Option<&str>) -> Option<MarketCard> {
        // Must have a title
        let title = self.question
            .or(self.group_item_title)
            .filter(|s| !s.is_empty())?;

        let id = match &self.id {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Number(n)) => n.to_string(),
            _ => self.condition_id.clone().unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        };
        // Capture the hex conditionId for CLOB trading (distinct from the numeric market id)
        let condition_id = self.condition_id.clone().unwrap_or_default();

        let category = self.category
            .unwrap_or_else(|| "General".to_string());

        // Parse outcomes and prices
        let names  = self.outcomes
            .as_ref()
            .map(parse_string_array)
            .unwrap_or_default();
        let prices = self.outcome_prices
            .as_ref()
            .map(parse_string_array)
            .unwrap_or_default();

        let outcomes: Vec<MarketOutcome> = names.into_iter().enumerate().map(|(i, name)| {
            let price_str = prices.get(i).cloned().unwrap_or_default();
            let price = price_str.parse::<f64>().unwrap_or(0.0);
            MarketOutcome { name, price }
        }).collect();

        let volume_usd = self.volume
            .as_ref().map(parse_f64).unwrap_or(0.0);
        let liquidity_usd = self.liquidity
            .as_ref().map(parse_f64).unwrap_or(0.0);

        let url = Some(build_polymarket_url(&id, event_slug.or(self.slug.as_deref())));

        Some(MarketCard {
            id,
            condition_id,
            title,
            category,
            outcomes,
            volume_usd,
            liquidity_usd,
            end_date: self.end_date,
            source: "polymarket".to_string(),
            url,
            image_url: self.image,
        })
    }
}


// ─── Raw event wrapper (events endpoint) ─────────────────────────────────────
//
// The Gamma /events endpoint returns events, each containing a list of
// individual markets. We use the EVENT slug for the URL because market-level
// slugs often have suffixes (e.g. -btts, date variants) that 404 on the
// Polymarket frontend.

#[derive(Debug, Deserialize)]
struct RawEvent {
    slug:    Option<String>,
    active:  Option<bool>,
    closed:  Option<bool>,
    markets: Option<Vec<RawMarket>>,
}

impl RawEvent {
    fn into_cards(self) -> Vec<MarketCard> {
        let event_slug = self.slug.clone();
        self.markets
            .unwrap_or_default()
            .into_iter()
            .filter(|m| m.active.unwrap_or(true) && !m.closed.unwrap_or(false))
            .filter_map(|m| m.into_card(event_slug.as_deref()))
            .filter(|c| !c.outcomes.is_empty())
            .collect()
    }
}

// ─── Fetcher ─────────────────────────────────────────────────────────────────

pub struct MarketsFetcher {
    client: reqwest::Client,
}

impl MarketsFetcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("Mozilla/5.0 Aureum Intelligence Terminal")
            .build()
            .expect("Failed to build reqwest client");
        Self { client }
    }

    pub async fn fetch(
        &self,
        category: Option<&str>,
        sort: &str,
        limit: usize,
    ) -> Result<Vec<MarketCard>, String> {
        // Build Gamma API URL
        // sort options: "volume" | "liquidity" | "start_date_min"
        let order = match sort {
            "liquidity" => "liquidity",
            "newest"    => "start_date_min",
            _           => "volume",
        };

        let mut url = format!(
            "https://gamma-api.polymarket.com/markets?active=true&closed=false&limit={}&order={}&ascending=false",
            limit.min(200),
            order,
        );

        if let Some(cat) = category {
            if !cat.is_empty() && cat != "all" {
                let encoded = urlencoding::encode(cat);
                url.push_str(&format!("&category={}", encoded));
            }
        }

        info!("Fetching Polymarket markets: {}", url);

        let resp = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Polymarket request failed: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            warn!("Polymarket API returned {}", status);
            return Err(format!("Polymarket API error: {}", status));
        }

        let raw: Vec<RawMarket> = resp.json().await
            .map_err(|e| format!("Failed to parse Polymarket response: {}", e))?;

        info!("Polymarket returned {} raw markets", raw.len());

        let cards: Vec<MarketCard> = raw
            .into_iter()
            .filter(|m| m.active.unwrap_or(true) && !m.closed.unwrap_or(false))
            .filter_map(|m| {
                // Extract event slug as owned String before moving m
                let event_slug: Option<String> = m.events.as_ref()
                    .and_then(|evts| evts.first())
                    .and_then(|e| e.slug.clone());
                m.into_card(event_slug.as_deref())
            })
            .filter(|c| !c.outcomes.is_empty())
            .collect();

        info!("Parsed {} market cards", cards.len());
        Ok(cards)
    }
}
