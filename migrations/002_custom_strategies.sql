-- Add custom_rules column for custom strategy builder
ALTER TABLE strategies ADD COLUMN IF NOT EXISTS custom_rules JSONB;

-- Add comment for documentation
COMMENT ON COLUMN strategies.custom_rules IS 'JSON rules for custom strategies: entry/exit conditions with indicators';
