//! Economic Calendar Client
//! 
//! Fetches upcoming high-impact economic events (NFP, CPI, FOMC, GDP)
//! Used by MRATE to adjust uncertainty scoring before major announcements

use chrono::{DateTime, Utc, Duration, Datelike, Weekday, NaiveTime, TimeZone, Timelike};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error};

/// Economic event impact level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventImpact {
    Low,
    Medium,
    High,
}

/// Classified event type for weighted uncertainty scoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventType {
    /// Federal Reserve interest rate decision — highest impact
    Fomc,
    /// Consumer Price Index — inflation data
    Cpi,
    /// Non-Farm Payrolls — employment data
    Nfp,
    /// Gross Domestic Product
    Gdp,
    /// PCE Price Index — Fed's preferred inflation gauge
    Pce,
    /// Other high-impact event
    Other,
}

impl EventType {
    /// Classify an event by its name
    pub fn from_name(name: &str) -> Self {
        let lower = name.to_lowercase();
        if lower.contains("fomc") || lower.contains("interest rate") || lower.contains("fed fund") {
            EventType::Fomc
        } else if lower.contains("cpi") || lower.contains("consumer price") {
            EventType::Cpi
        } else if lower.contains("nfp") || lower.contains("non-farm") || lower.contains("nonfarm") {
            EventType::Nfp
        } else if lower.contains("gdp") || lower.contains("gross domestic") {
            EventType::Gdp
        } else if lower.contains("pce") || lower.contains("personal consumption") {
            EventType::Pce
        } else {
            EventType::Other
        }
    }
    
    /// Uncertainty multiplier for this event type
    /// FOMC is the single most market-moving event
    pub fn uncertainty_weight(&self) -> f64 {
        match self {
            EventType::Fomc => 1.5,
            EventType::Cpi  => 1.3,
            EventType::Nfp  => 1.2,
            EventType::Pce  => 1.2,
            EventType::Gdp  => 1.1,
            EventType::Other => 1.0,
        }
    }
}

/// A scheduled economic event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EconomicEvent {
    pub name: String,
    pub timestamp: DateTime<Utc>,
    pub impact: EventImpact,
    pub currency: String,
    pub description: Option<String>,
    pub event_type: EventType,
}

impl EconomicEvent {
    /// Hours until this event occurs
    pub fn hours_until(&self) -> f64 {
        let now = Utc::now();
        if self.timestamp <= now {
            return 0.0;
        }
        let duration = self.timestamp - now;
        duration.num_minutes() as f64 / 60.0
    }
}

/// Economic calendar response from API
#[derive(Debug, Deserialize)]
struct CalendarApiResponse {
    #[serde(default)]
    events: Vec<CalendarApiEvent>,
}

#[derive(Debug, Deserialize)]
struct CalendarApiEvent {
    title: Option<String>,
    country: Option<String>,
    date: Option<String>,
    impact: Option<String>,
    forecast: Option<String>,
    previous: Option<String>,
}

/// Economic calendar client
pub struct EconomicCalendarClient {
    client: Client,
    /// Known high-impact events (static schedule as fallback)
    static_events: Vec<StaticEvent>,
}

/// Static event definition for recurring events
struct StaticEvent {
    name: &'static str,
    /// Day of month (0 = calculate dynamically)
    day_of_month: u32,
    /// Time in EST (economic data is released in EST)
    time_est: (u32, u32),
    /// Which week of month (for events like NFP - first Friday)
    week_rule: Option<WeekRule>,
    currency: &'static str,
}

#[derive(Clone, Copy)]
enum WeekRule {
    FirstFriday,      // NFP
    SecondWednesday,  // CPI (usually)
}

impl EconomicCalendarClient {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
            static_events: Self::build_static_events(),
        }
    }
    
    /// Build list of known high-impact recurring events
    fn build_static_events() -> Vec<StaticEvent> {
        vec![
            // NFP - First Friday of each month at 8:30 AM EST
            StaticEvent {
                name: "Non-Farm Payrolls (NFP)",
                day_of_month: 0,
                time_est: (8, 30),
                week_rule: Some(WeekRule::FirstFriday),
                currency: "USD",
            },
            // CPI - Usually around 13th of month at 8:30 AM EST
            StaticEvent {
                name: "Consumer Price Index (CPI)",
                day_of_month: 13,
                time_est: (8, 30),
                week_rule: None,
                currency: "USD",
            },
            // FOMC - 8 times per year, 2:00 PM EST (we'll handle this specially)
            StaticEvent {
                name: "FOMC Interest Rate Decision",
                day_of_month: 0, // Handled specially
                time_est: (14, 0),
                week_rule: None,
                currency: "USD",
            },
            // GDP - End of month at 8:30 AM EST
            StaticEvent {
                name: "GDP (Quarterly)",
                day_of_month: 28,
                time_est: (8, 30),
                week_rule: None,
                currency: "USD",
            },
            // PCE - Last Friday of month at 8:30 AM EST (Fed's preferred inflation gauge)
            StaticEvent {
                name: "PCE Price Index",
                day_of_month: 0,
                time_est: (8, 30),
                week_rule: None, // Last Friday - handled specially
                currency: "USD",
            },
        ]
    }
    
    /// Get upcoming high-impact events within the specified hours
    pub async fn get_upcoming_events(&self, hours: u32) -> Vec<EconomicEvent> {
        // Try to fetch from API first
        if let Ok(events) = self.fetch_from_api(hours).await {
            if !events.is_empty() {
                info!("Fetched {} events from calendar API", events.len());
                return events;
            }
        }
        
        // Fall back to static schedule
        info!("Using static calendar fallback");
        self.get_static_events(hours)
    }
    
    /// Fetch events from external calendar API
    async fn fetch_from_api(&self, hours: u32) -> Result<Vec<EconomicEvent>, String> {
        // Using a public economic calendar API
        // Note: In production, consider using a paid API for reliability
        let url = "https://nfs.faireconomy.media/ff_calendar_thisweek.json";
        
        let response = self.client
            .get(url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch calendar: {}", e))?;
        
        if !response.status().is_success() {
            return Err(format!("Calendar API returned {}", response.status()));
        }
        
        let api_events: Vec<CalendarApiEvent> = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse calendar: {}", e))?;
        
        let now = Utc::now();
        let cutoff = now + Duration::hours(hours as i64);
        
        let events: Vec<EconomicEvent> = api_events
            .into_iter()
            .filter_map(|e| self.parse_api_event(e))
            .filter(|e| e.timestamp > now && e.timestamp <= cutoff)
            .filter(|e| e.impact == EventImpact::High)
            .collect();
        
        Ok(events)
    }
    
    /// Parse an API event into our format
    fn parse_api_event(&self, event: CalendarApiEvent) -> Option<EconomicEvent> {
        let name = event.title?;
        let date_str = event.date?;
        
        // Parse date (format varies by API, this handles common formats)
        let timestamp = self.parse_event_date(&date_str)?;
        
        let impact = match event.impact.as_deref() {
            Some("High") | Some("high") | Some("3") => EventImpact::High,
            Some("Medium") | Some("medium") | Some("2") => EventImpact::Medium,
            _ => EventImpact::Low,
        };
        
        let currency = event.country.unwrap_or_else(|| "USD".to_string());
        
        let event_type = EventType::from_name(&name);
        Some(EconomicEvent {
            name,
            timestamp,
            impact,
            currency,
            description: event.forecast.map(|f| format!("Forecast: {}", f)),
            event_type,
        })
    }
    
    /// Parse various date formats from calendar APIs
    fn parse_event_date(&self, date_str: &str) -> Option<DateTime<Utc>> {
        // Try ISO 8601 format first
        if let Ok(dt) = DateTime::parse_from_rfc3339(date_str) {
            return Some(dt.with_timezone(&Utc));
        }
        
        // Try "YYYY-MM-DD HH:MM:SS" format
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%d %H:%M:%S") {
            return Some(Utc.from_utc_datetime(&dt));
        }
        
        // Try "YYYY-MM-DDTHH:MM:SS" format
        if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(date_str, "%Y-%m-%dT%H:%M:%S") {
            return Some(Utc.from_utc_datetime(&dt));
        }
        
        None
    }
    
    /// Get events from static schedule (fallback)
    fn get_static_events(&self, hours: u32) -> Vec<EconomicEvent> {
        let now = Utc::now();
        let cutoff = now + Duration::hours(hours as i64);
        let mut events = Vec::new();
        
        // Check FOMC dates (2025-2026 schedule)
        let fomc_dates = self.get_fomc_dates();
        for fomc_date in fomc_dates {
            if fomc_date > now && fomc_date <= cutoff {
                events.push(EconomicEvent {
                    name: "FOMC Interest Rate Decision".to_string(),
                    timestamp: fomc_date,
                    impact: EventImpact::High,
                    currency: "USD".to_string(),
                    description: Some("Federal Reserve monetary policy decision".to_string()),
                    event_type: EventType::Fomc,
                });
            }
        }
        
        // Check for NFP (first Friday of month)
        if let Some(nfp_date) = self.next_first_friday(now, (8, 30)) {
            if nfp_date > now && nfp_date <= cutoff {
                events.push(EconomicEvent {
                    name: "Non-Farm Payrolls (NFP)".to_string(),
                    timestamp: nfp_date,
                    impact: EventImpact::High,
                    currency: "USD".to_string(),
                    description: Some("US employment report".to_string()),
                    event_type: EventType::Nfp,
                });
            }
        }
        
        // Check for CPI (around 13th of month)
        if let Some(cpi_date) = self.next_cpi_date(now) {
            if cpi_date > now && cpi_date <= cutoff {
                events.push(EconomicEvent {
                    name: "Consumer Price Index (CPI)".to_string(),
                    timestamp: cpi_date,
                    impact: EventImpact::High,
                    currency: "USD".to_string(),
                    description: Some("US inflation data".to_string()),
                    event_type: EventType::Cpi,
                });
            }
        }
        
        events.sort_by_key(|e| e.timestamp);
        events
    }
    
    /// Get FOMC meeting dates for 2025-2026
    fn get_fomc_dates(&self) -> Vec<DateTime<Utc>> {
        // FOMC 2025-2026 schedule (announcement at 2:00 PM EST = 19:00 UTC)
        let dates = [
            // 2025
            "2025-01-29", "2025-03-19", "2025-05-07", "2025-06-18",
            "2025-07-30", "2025-09-17", "2025-11-05", "2025-12-17",
            // 2026
            "2026-01-28", "2026-03-18", "2026-05-06", "2026-06-17",
            "2026-07-29", "2026-09-16", "2026-11-04", "2026-12-16",
        ];
        
        dates.iter()
            .filter_map(|d| {
                chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()
            })
            .filter_map(|date| {
                let time = NaiveTime::from_hms_opt(19, 0, 0)?; // 2:00 PM EST = 19:00 UTC
                let dt = date.and_time(time);
                Some(Utc.from_utc_datetime(&dt))
            })
            .collect()
    }
    
    /// Calculate next first Friday of month
    fn next_first_friday(&self, from: DateTime<Utc>, time_est: (u32, u32)) -> Option<DateTime<Utc>> {
        let mut date = from.date_naive();
        
        // If we're past the first Friday of this month, go to next month
        let first_friday = self.first_friday_of_month(date.year(), date.month())?;
        if date.day() > first_friday.day() {
            // Move to next month
            if date.month() == 12 {
                date = chrono::NaiveDate::from_ymd_opt(date.year() + 1, 1, 1)?;
            } else {
                date = chrono::NaiveDate::from_ymd_opt(date.year(), date.month() + 1, 1)?;
            }
        }
        
        let first_friday = self.first_friday_of_month(date.year(), date.month())?;
        let time = NaiveTime::from_hms_opt(time_est.0 + 5, time_est.1, 0)?; // EST to UTC (+5)
        let dt = first_friday.and_time(time);
        Some(Utc.from_utc_datetime(&dt))
    }
    
    /// Get first Friday of a given month
    fn first_friday_of_month(&self, year: i32, month: u32) -> Option<chrono::NaiveDate> {
        let first_day = chrono::NaiveDate::from_ymd_opt(year, month, 1)?;
        let days_until_friday = (Weekday::Fri.num_days_from_monday() as i64 
            - first_day.weekday().num_days_from_monday() as i64 + 7) % 7;
        first_day.checked_add_signed(Duration::days(days_until_friday))
    }
    
    /// Get next CPI release date (approximately 13th of month, 8:30 AM EST)
    fn next_cpi_date(&self, from: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut date = from.date_naive();
        
        // CPI is typically released around the 13th
        let cpi_day = 13;
        
        if date.day() > cpi_day {
            // Move to next month
            if date.month() == 12 {
                date = chrono::NaiveDate::from_ymd_opt(date.year() + 1, 1, cpi_day)?;
            } else {
                date = chrono::NaiveDate::from_ymd_opt(date.year(), date.month() + 1, cpi_day)?;
            }
        } else {
            date = chrono::NaiveDate::from_ymd_opt(date.year(), date.month(), cpi_day)?;
        }
        
        // 8:30 AM EST = 13:30 UTC
        let time = NaiveTime::from_hms_opt(13, 30, 0)?;
        let dt = date.and_time(time);
        Some(Utc.from_utc_datetime(&dt))
    }
    
    /// Check if there's a high-impact event within the specified hours
    pub async fn has_high_impact_event(&self, hours: u32) -> bool {
        let events = self.get_upcoming_events(hours).await;
        events.iter().any(|e| e.impact == EventImpact::High)
    }
    
    /// Get hours until the next high-impact event (if any within 48 hours)
    pub async fn hours_to_next_high_impact(&self) -> Option<f64> {
        let events = self.get_upcoming_events(48).await;
        events.iter()
            .filter(|e| e.impact == EventImpact::High)
            .min_by_key(|e| e.timestamp)
            .map(|e| e.hours_until())
    }
}

impl Default for EconomicCalendarClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_event_hours_until() {
        let future_event = EconomicEvent {
            name: "Test".to_string(),
            timestamp: Utc::now() + Duration::hours(6),
            impact: EventImpact::High,
            currency: "USD".to_string(),
            description: None,
            event_type: EventType::Other,
        };
        
        let hours = future_event.hours_until();
        assert!(hours > 5.9 && hours < 6.1);
    }
    
    #[test]
    fn test_first_friday_calculation() {
        let client = EconomicCalendarClient::new();
        
        // March 2025 - first Friday should be March 7th
        let first_friday = client.first_friday_of_month(2025, 3).unwrap();
        assert_eq!(first_friday.day(), 7);
        assert_eq!(first_friday.weekday(), Weekday::Fri);
    }
    
    #[test]
    fn test_fomc_dates() {
        let client = EconomicCalendarClient::new();
        let dates = client.get_fomc_dates();
        
        // Should have 16 dates (8 per year for 2025-2026)
        assert_eq!(dates.len(), 16);
        
        // All should be at 19:00 UTC
        for date in dates {
            assert_eq!(date.hour(), 19);
        }
    }
}
