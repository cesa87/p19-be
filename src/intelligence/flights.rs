//! Flight Tracker — Military / Government aircraft via OpenSky Network + ADS-B Exchange
//!
//! Primary source: OpenSky Network (free, no key).
//! Upgrade path: ADS-B Exchange RapidAPI (set ADSBEXCHANGE_RAPIDAPI_KEY in .env).
//!
//! Classifies aircraft by callsign prefix into:
//!   ISR / VIP / BOMBER / FIGHTER / TANKER / TRANSPORT / UNKNOWN

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::config::Config;

// ─── Domain types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TrackedFlight {
    pub id: Uuid,
    pub icao24: String,
    pub callsign: Option<String>,
    pub aircraft_type: String,
    pub origin_country: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude_ft: Option<f64>,
    pub speed_kts: Option<f64>,
    pub heading: Option<f64>,
    pub squawk: Option<String>,
    pub on_ground: bool,
    pub source: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
}

// ─── Callsign classifier ───────────────────────────────────────────────────────

/// Classify an aircraft into a type based on its callsign.
/// Returns one of: ISR / VIP / BOMBER / FIGHTER / TANKER / TRANSPORT / UNKNOWN
pub fn classify_callsign(callsign: &str) -> &'static str {
    let cs = callsign.trim().to_uppercase();

    // VIP / Executive transport (highest priority)
    for prefix in &["SAM", "PAT", "AF1", "AF2", "EXEC", "VENUS", "MARINE", "FLOTUS"] {
        if cs.starts_with(prefix) { return "VIP"; }
    }

    // ISR — Intelligence, Surveillance, Reconnaissance
    for prefix in &[
        "JAKE",   // E-8 JSTARS callsign series
        "IRON",   // RC-135 Rivet Joint
        "FURY",   // U-2 / RQ-4
        "RAIDR",  // B-21 / ISR
        "COBRA",  // EP-3 / ISR
        "GHOST",  // ISR platform
        "DEMON",  // ISR
        "VIPR",   // EA-18G Growler series
        "SWIFT",  // E-3 Sentry / ISR
        "SCOPE",  // RC-135
        "DRAGON", // RQ-4 Global Hawk
        "BACN",   // EQ-4 relay
        "RIOT",   // RC-135
        "FORGE",  // ISR
        "TOPAZ",  // RQ-4
        "JEDI",   // ISR composite
        "SIGINT", // generic SIGINT
        "STING",  // ISR
    ] {
        if cs.starts_with(prefix) { return "ISR"; }
    }

    // Bombers
    for prefix in &[
        "BONE",   // B-1B Lancer
        "SPIRIT", // B-2 Spirit
        "BUFF",   // B-52 Stratofortress
        "HAVOC",  // B-52 callsign
    ] {
        if cs.starts_with(prefix) { return "BOMBER"; }
    }

    // Tankers
    for prefix in &[
        "TIGER",  // KC-135 training
        "ROOK",   // KC-135
        "QUID",   // RAF Voyager tanker
        "TARTAN", // RAF VC10
        "REACH",  // AMC tanker/transport (some tanker missions)
        "NIGHT",  // KC-130 USMC
        "ARCO",   // KC-135 call
    ] {
        if cs.starts_with(prefix) { return "TANKER"; }
    }

    // Transport / Airlift
    for prefix in &[
        "RCH",    // Air Mobility Command (primary AMC code)
        "MATS",   // Military Air Transport Service
        "BARAK",  // Israeli military transport
        "CNV",    // USN Logistics
        "SPAR",   // USAF transport (VIP-adjacent but classified as transport)
        "ATLAS",  // Atlas Air military charter
        "BOXER",  // C-17 callsign
        "GLOBEMASTER", // C-17
        "HERKY",  // C-130 Hercules
        "YANKY",  // C-130 callsign
    ] {
        if cs.starts_with(prefix) { return "TRANSPORT"; }
    }

    // Fighters
    for prefix in &[
        "VIPER",  // F-16
        "EAGLE",  // F-15
        "RAPTOR", // F-22
        "HORNET", // F/A-18
        "FALCON", // F-16 generic
        "SNAKE",  // F-16 callsign
        "KNIFE",  // fighter callsign
        "BLADE",  // fighter callsign
    ] {
        if cs.starts_with(prefix) { return "FIGHTER"; }
    }

    "UNKNOWN"
}

// ─── OpenSky Network fetcher ───────────────────────────────────────────────────

/// OpenSky `/states/all` response
#[derive(Deserialize)]
struct OpenSkyResponse {
    states: Option<Vec<serde_json::Value>>,
}

/// Parse a single OpenSky state vector into a RawFlight.
/// State vector: [icao24, callsign, origin_country, time_position, last_contact,
///                longitude, latitude, baro_altitude, on_ground, velocity,
///                true_track, vertical_rate, sensors, geo_altitude, squawk, spi, pos_src]
#[derive(Debug)]
struct RawFlight {
    icao24: String,
    callsign: Option<String>,
    origin_country: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    altitude_ft: Option<f64>,
    speed_kts: Option<f64>,
    heading: Option<f64>,
    squawk: Option<String>,
    on_ground: bool,
    aircraft_type: &'static str,
    source: &'static str,
}

fn parse_opensky_state(state: &serde_json::Value) -> Option<RawFlight> {
    let arr = state.as_array()?;

    let icao24 = arr.get(0)?.as_str()?.trim().to_string();
    if icao24.is_empty() { return None; }

    let callsign = arr.get(1)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let origin_country = arr.get(2)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let longitude = arr.get(5).and_then(|v| v.as_f64());
    let latitude  = arr.get(6).and_then(|v| v.as_f64());

    // baro_altitude in metres → feet
    let altitude_ft = arr.get(7)
        .and_then(|v| v.as_f64())
        .map(|m| m * 3.28084);

    let on_ground = arr.get(8).and_then(|v| v.as_bool()).unwrap_or(false);

    // velocity in m/s → knots
    let speed_kts = arr.get(9)
        .and_then(|v| v.as_f64())
        .map(|ms| ms * 1.94384);

    let heading = arr.get(10).and_then(|v| v.as_f64());

    let squawk = arr.get(14)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty() && s != "null");

    // Classify
    let aircraft_type = callsign
        .as_deref()
        .map(classify_callsign)
        .unwrap_or("UNKNOWN");

    Some(RawFlight {
        icao24,
        callsign,
        origin_country,
        latitude,
        longitude,
        altitude_ft,
        speed_kts,
        heading,
        squawk,
        on_ground,
        aircraft_type,
        source: "opensky",
    })
}

/// Fetch military-interest aircraft from OpenSky Network.
/// We query a broad North-Atlantic / Europe / Middle-East bounding box
/// and filter by known military callsign patterns.
async fn fetch_opensky(http: &Client) -> Vec<RawFlight> {
    // Broad box: covers NA, Atlantic, Europe, Middle East
    let url = "https://opensky-network.org/api/states/all?lamin=20&lomin=-130&lamax=75&lomax=60";

    let resp = match http
        .get(url)
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => { warn!("OpenSky request failed: {}", e); return vec![]; }
    };

    if !resp.status().is_success() {
        warn!("OpenSky returned status {}", resp.status());
        return vec![];
    }

    let body: OpenSkyResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => { warn!("OpenSky parse error: {}", e); return vec![]; }
    };

    let states = match body.states {
        Some(s) => s,
        None => return vec![],
    };

    states
        .iter()
        .filter_map(parse_opensky_state)
        .filter(|f| {
            // Keep only classified (non-UNKNOWN) aircraft or military squawk codes
            f.aircraft_type != "UNKNOWN"
                || f.squawk.as_deref().map(|s| is_military_squawk(s)).unwrap_or(false)
        })
        .collect()
}

/// Some squawk codes are used exclusively by military operations
fn is_military_squawk(squawk: &str) -> bool {
    matches!(squawk, "7400" | "7500" | "7600" | "7700"  // emergency
        | "0000" | "0010" | "0020" | "0030"  // military
    )
}

// ─── ADS-B Exchange (RapidAPI) fetcher — upgrade path ─────────────────────────

#[derive(Deserialize)]
struct AdsbExchangeResponse {
    ac: Option<Vec<AdsbAircraft>>,
}

#[derive(Deserialize)]
struct AdsbAircraft {
    #[serde(default)]
    hex: String,
    #[serde(default)]
    flight: String,
    #[serde(default)]
    r: String,
    #[serde(default)]
    t: String,
    lat: Option<f64>,
    lon: Option<f64>,
    alt_baro: Option<serde_json::Value>,
    gs: Option<f64>,
    track: Option<f64>,
    squawk: Option<String>,
    #[serde(default)]
    gnd: bool,
}

async fn fetch_adsbexchange(http: &Client, api_key: &str) -> Vec<RawFlight> {
    let resp = match http
        .get("https://adsbexchange-com1.p.rapidapi.com/v2/mil/")
        .header("x-rapidapi-host", "adsbexchange-com1.p.rapidapi.com")
        .header("x-rapidapi-key", api_key)
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => { warn!("ADS-B Exchange request failed: {}", e); return vec![]; }
    };

    if !resp.status().is_success() {
        warn!("ADS-B Exchange returned status {}", resp.status());
        return vec![];
    }

    let body: AdsbExchangeResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => { warn!("ADS-B Exchange parse error: {}", e); return vec![]; }
    };

    body.ac.unwrap_or_default().into_iter().filter_map(|ac| {
        if ac.hex.is_empty() { return None; }
        let callsign = if ac.flight.trim().is_empty() { None } else { Some(ac.flight.trim().to_string()) };
        let aircraft_type = callsign.as_deref().map(classify_callsign).unwrap_or("UNKNOWN");
        let altitude_ft = ac.alt_baro.as_ref().and_then(|v| {
            v.as_f64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        });
        Some(RawFlight {
            icao24: ac.hex.clone(),
            callsign,
            origin_country: None,
            latitude: ac.lat,
            longitude: ac.lon,
            altitude_ft,
            speed_kts: ac.gs,
            heading: ac.track,
            squawk: ac.squawk,
            on_ground: ac.gnd,
            aircraft_type,
            source: "adsbexchange",
        })
    }).collect()
}

// ─── FlightTracker ────────────────────────────────────────────────────────────

pub struct FlightTracker {
    pool: PgPool,
    http: Client,
    adsbexchange_key: Option<String>,
}

impl FlightTracker {
    pub fn new(pool: PgPool, config: &Config) -> Self {
        Self {
            pool,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(25))
                .user_agent("Mozilla/5.0 (compatible; AureumFlightTracker/1.0)")
                .build()
                .unwrap_or_default(),
            adsbexchange_key: config.adsbexchange_rapidapi_key.clone(),
        }
    }

    pub async fn poll(&self) {
        let flights = if let Some(key) = &self.adsbexchange_key {
            info!("✈️  Flight poll via ADS-B Exchange");
            fetch_adsbexchange(&self.http, key).await
        } else {
            fetch_opensky(&self.http).await
        };

        if flights.is_empty() {
            return;
        }

        // Evict stale entries older than 2 hours
        let _ = sqlx::query(
            "DELETE FROM tracked_flights WHERE last_seen_at < NOW() - INTERVAL '2 hours'"
        )
        .execute(&self.pool)
        .await;

        info!("✈️  {} military/interest flights tracked", flights.len());

        for f in &flights {
            let _ = sqlx::query(r#"
                INSERT INTO tracked_flights
                    (icao24, callsign, aircraft_type, origin_country,
                     latitude, longitude, altitude_ft, speed_kts, heading,
                     squawk, on_ground, source, last_seen_at)
                VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,NOW())
                ON CONFLICT (icao24) DO UPDATE SET
                    callsign       = EXCLUDED.callsign,
                    aircraft_type  = EXCLUDED.aircraft_type,
                    origin_country = COALESCE(EXCLUDED.origin_country, tracked_flights.origin_country),
                    latitude       = EXCLUDED.latitude,
                    longitude      = EXCLUDED.longitude,
                    altitude_ft    = EXCLUDED.altitude_ft,
                    speed_kts      = EXCLUDED.speed_kts,
                    heading        = EXCLUDED.heading,
                    squawk         = EXCLUDED.squawk,
                    on_ground      = EXCLUDED.on_ground,
                    source         = EXCLUDED.source,
                    last_seen_at   = NOW()
            "#)
            .bind(&f.icao24)
            .bind(&f.callsign)
            .bind(f.aircraft_type)
            .bind(&f.origin_country)
            .bind(f.latitude)
            .bind(f.longitude)
            .bind(f.altitude_ft)
            .bind(f.speed_kts)
            .bind(f.heading)
            .bind(&f.squawk)
            .bind(f.on_ground)
            .bind(f.source)
            .execute(&self.pool)
            .await;
        }
    }
}

// ─── DB helpers for API ───────────────────────────────────────────────────────

pub async fn get_tracked_flights(
    pool: &PgPool,
    aircraft_type_filter: Option<&str>,
    limit: i64,
) -> Result<Vec<TrackedFlight>, sqlx::Error> {
    sqlx::query_as::<_, TrackedFlight>(r#"
        SELECT * FROM tracked_flights
        WHERE ($1::text IS NULL OR aircraft_type = $1)
        ORDER BY last_seen_at DESC
        LIMIT $2
    "#)
    .bind(aircraft_type_filter)
    .bind(limit)
    .fetch_all(pool)
    .await
}
