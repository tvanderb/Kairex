mod connection;
mod error;
mod market_data;
mod migrations;
mod models;
mod retention;
mod system_output;

pub use connection::Database;
pub use error::{Result, StorageError};
pub use models::{
    ActiveSetup, Candle, FiredAlert, FundingRate, IndexValue, OpenInterest, SystemOutput,
};
pub use retention::RetentionConfig;
pub use system_output::extract_setups;

// -- Market data convenience methods --

impl Database {
    pub fn insert_candle(&self, candle: &Candle) -> Result<()> {
        self.with_writer(|conn| market_data::insert_candle(conn, candle))
    }

    pub fn insert_candles(&self, candles: &[Candle]) -> Result<()> {
        self.with_writer(|conn| market_data::insert_candles(conn, candles))
    }

    pub fn query_candles(
        &self,
        symbol: &str,
        timeframe: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<Candle>> {
        self.with_reader(|conn| market_data::query_candles(conn, symbol, timeframe, start, end))
    }

    pub fn query_latest_candle(&self, symbol: &str, timeframe: &str) -> Result<Option<Candle>> {
        self.with_reader(|conn| market_data::query_latest_candle(conn, symbol, timeframe))
    }

    pub fn insert_funding_rate(&self, rate: &FundingRate) -> Result<()> {
        self.with_writer(|conn| market_data::insert_funding_rate(conn, rate))
    }

    pub fn query_funding_rates(
        &self,
        symbol: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<FundingRate>> {
        self.with_reader(|conn| market_data::query_funding_rates(conn, symbol, start, end))
    }

    pub fn query_latest_funding_rate(&self, symbol: &str) -> Result<Option<FundingRate>> {
        self.with_reader(|conn| market_data::query_latest_funding_rate(conn, symbol))
    }

    pub fn insert_open_interest(&self, oi: &OpenInterest) -> Result<()> {
        self.with_writer(|conn| market_data::insert_open_interest(conn, oi))
    }

    pub fn query_open_interest(
        &self,
        symbol: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<OpenInterest>> {
        self.with_reader(|conn| market_data::query_open_interest(conn, symbol, start, end))
    }

    pub fn query_latest_open_interest(&self, symbol: &str) -> Result<Option<OpenInterest>> {
        self.with_reader(|conn| market_data::query_latest_open_interest(conn, symbol))
    }

    pub fn insert_index_value(&self, idx: &IndexValue) -> Result<()> {
        self.with_writer(|conn| market_data::insert_index_value(conn, idx))
    }

    pub fn query_index_values(
        &self,
        index_type: &str,
        start: i64,
        end: i64,
    ) -> Result<Vec<IndexValue>> {
        self.with_reader(|conn| market_data::query_index_values(conn, index_type, start, end))
    }

    pub fn query_latest_index_value(&self, index_type: &str) -> Result<Option<IndexValue>> {
        self.with_reader(|conn| market_data::query_latest_index_value(conn, index_type))
    }

    // -- System output convenience methods --

    pub fn store_report(&self, output: &SystemOutput, setups: &[ActiveSetup]) -> Result<i64> {
        self.with_writer(|conn| system_output::store_report(conn, output, setups))
    }

    pub fn query_outputs_by_type(
        &self,
        report_type: &str,
        limit: i64,
    ) -> Result<Vec<SystemOutput>> {
        self.with_reader(|conn| system_output::query_outputs_by_type(conn, report_type, limit))
    }

    pub fn query_outputs_by_date_range(&self, start: i64, end: i64) -> Result<Vec<SystemOutput>> {
        self.with_reader(|conn| system_output::query_outputs_by_date_range(conn, start, end))
    }

    pub fn query_latest_output_by_type(&self, report_type: &str) -> Result<Option<SystemOutput>> {
        self.with_reader(|conn| system_output::query_latest_output_by_type(conn, report_type))
    }

    pub fn resolve_setup(
        &self,
        setup_id: i64,
        status: &str,
        resolved_at: i64,
        resolved_price: f64,
    ) -> Result<()> {
        self.with_writer(|conn| {
            system_output::resolve_setup(conn, setup_id, status, resolved_at, resolved_price)
        })
    }

    pub fn update_delivery_status(
        &self,
        output_id: i64,
        status: &str,
        delivered_at: i64,
    ) -> Result<()> {
        self.with_writer(|conn| {
            system_output::update_delivery_status(conn, output_id, status, delivered_at)
        })
    }

    pub fn expire_stale_setups(&self, before_timestamp: i64) -> Result<u64> {
        self.with_writer(|conn| system_output::expire_stale_setups(conn, before_timestamp))
    }

    pub fn query_active_setups(&self) -> Result<Vec<ActiveSetup>> {
        self.with_reader(system_output::query_active_setups)
    }

    pub fn query_active_setups_by_asset(&self, asset: &str) -> Result<Vec<ActiveSetup>> {
        self.with_reader(|conn| system_output::query_active_setups_by_asset(conn, asset))
    }

    pub fn insert_fired_alert(&self, alert: &FiredAlert) -> Result<i64> {
        self.with_writer(|conn| system_output::insert_fired_alert(conn, alert))
    }

    pub fn is_alert_on_cooldown(&self, alert_type: &str, now: i64) -> Result<bool> {
        self.with_reader(|conn| system_output::is_alert_on_cooldown(conn, alert_type, now))
    }

    // -- Retention convenience method --

    pub fn run_retention(&self, config: &RetentionConfig, now: i64) -> Result<()> {
        self.with_writer(|conn| retention::run_retention(conn, config, now))
    }
}
