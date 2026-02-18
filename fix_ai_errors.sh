#!/bin/bash
# Script to apply all fixes for AI/ML compilation errors

echo "Fixing AI/ML compilation errors..."

# Add BigDecimal import to trailing_stop.rs
grep -q "use bigdecimal" src/analytics/trailing_stop.rs || \
  sed -i '' '7i\
use bigdecimal::BigDecimal;
' src/analytics/trailing_stop.rs

echo "✓ Added BigDecimal import"
echo "Done! Now manually apply code fixes..."
