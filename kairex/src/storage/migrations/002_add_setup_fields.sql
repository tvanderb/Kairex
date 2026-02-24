-- Add trigger_field and confidence columns to active_setups.
-- trigger_field: indicator name for indicator triggers (null for price triggers)
-- confidence: LLM confidence rating (0.0-1.0) for calibration tracking

ALTER TABLE active_setups ADD COLUMN trigger_field TEXT;
ALTER TABLE active_setups ADD COLUMN confidence REAL;
