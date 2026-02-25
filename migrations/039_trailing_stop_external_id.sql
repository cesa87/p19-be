-- Add external_trade_id to trailing_stop_state so we can modify trades on OANDA
ALTER TABLE trailing_stop_state ADD COLUMN IF NOT EXISTS external_trade_id TEXT;

-- Index for looking up by external trade ID
CREATE INDEX IF NOT EXISTS idx_trailing_stop_ext_trade ON trailing_stop_state(external_trade_id);
