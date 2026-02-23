-- Market data tables

CREATE TABLE IF NOT EXISTS candles (
    symbol    TEXT    NOT NULL,
    timeframe TEXT    NOT NULL,
    open_time INTEGER NOT NULL,
    open      REAL    NOT NULL,
    high      REAL    NOT NULL,
    low       REAL    NOT NULL,
    close     REAL    NOT NULL,
    volume    REAL    NOT NULL,
    source    TEXT    NOT NULL,
    PRIMARY KEY (symbol, timeframe, open_time)
);

CREATE TABLE IF NOT EXISTS funding_rates (
    symbol    TEXT    NOT NULL,
    timestamp INTEGER NOT NULL,
    rate      REAL    NOT NULL,
    PRIMARY KEY (symbol, timestamp)
);

CREATE TABLE IF NOT EXISTS open_interest (
    symbol    TEXT    NOT NULL,
    timestamp INTEGER NOT NULL,
    value     REAL    NOT NULL,
    PRIMARY KEY (symbol, timestamp)
);

CREATE TABLE IF NOT EXISTS indices (
    index_type TEXT    NOT NULL,
    timestamp  INTEGER NOT NULL,
    value      REAL    NOT NULL,
    PRIMARY KEY (index_type, timestamp)
);

-- System output tables

CREATE TABLE IF NOT EXISTS system_outputs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    report_type     TEXT    NOT NULL,
    generated_at    INTEGER NOT NULL,
    schema_version  TEXT    NOT NULL,
    output          TEXT    NOT NULL,
    delivered_at    INTEGER,
    delivery_status TEXT    NOT NULL DEFAULT 'pending'
);

CREATE TABLE IF NOT EXISTS active_setups (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    source_output_id   INTEGER NOT NULL REFERENCES system_outputs(id),
    asset              TEXT    NOT NULL,
    direction          TEXT    NOT NULL,
    trigger_condition  TEXT    NOT NULL,
    trigger_level      REAL    NOT NULL,
    target_level       REAL,
    invalidation_level REAL,
    status             TEXT    NOT NULL DEFAULT 'active',
    created_at         INTEGER NOT NULL,
    resolved_at        INTEGER,
    resolved_price     REAL
);

CREATE TABLE IF NOT EXISTS fired_alerts (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    setup_id       INTEGER REFERENCES active_setups(id),
    alert_type     TEXT    NOT NULL,
    fired_at       INTEGER NOT NULL,
    cooldown_until INTEGER NOT NULL,
    output_id      INTEGER REFERENCES system_outputs(id)
);

-- Indexes for common query patterns

CREATE INDEX IF NOT EXISTS idx_candles_symbol_tf
    ON candles (symbol, timeframe, open_time);

CREATE INDEX IF NOT EXISTS idx_funding_rates_symbol
    ON funding_rates (symbol, timestamp);

CREATE INDEX IF NOT EXISTS idx_open_interest_symbol
    ON open_interest (symbol, timestamp);

CREATE INDEX IF NOT EXISTS idx_indices_type
    ON indices (index_type, timestamp);

CREATE INDEX IF NOT EXISTS idx_outputs_type_date
    ON system_outputs (report_type, generated_at);

CREATE INDEX IF NOT EXISTS idx_setups_status
    ON active_setups (status);

CREATE INDEX IF NOT EXISTS idx_setups_asset_status
    ON active_setups (asset, status);

CREATE INDEX IF NOT EXISTS idx_alerts_cooldown
    ON fired_alerts (alert_type, cooldown_until);
