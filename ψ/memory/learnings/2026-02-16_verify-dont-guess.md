# Lesson: Verify, Don't Guess

**Date**: 2026-02-16
**Context**: BME-Putaway environment configuration
**Pattern**: Database-driven configuration

## The Mistake

When setting up location codes in `.env`, I guessed:
```
# Available locations: TFC1, TFC2, TFC3, MFC1, MFC2  ← WRONG
```

These locations don't exist in the database. I invented them based on naming patterns.

## The Correction

Wind pushed back: *"wtf อย่ากรอก location มั่วๆ ใช้ nwfth-sql to check"*

Query revealed actual locations:
```sql
SELECT DISTINCT Location FROM INLOC ORDER BY Location
-- Result: CCN, CPN, NPD, QCR, SNT, TEST, TFC1, TGJ1, TIP8, TST, UPC1
```

Then narrowed to Putaway-supported: TFC1, TIP8

## The Rule

**When configuration data exists in a database, query it. Never guess.**

This applies to:
- Location codes
- Warehouse bins
- User departments
- Item categories
- Any reference data

## Why It Matters

1. **Wrong config breaks the app** - Fake locations cause API failures
2. **Wasted time on corrections** - Fixing is slower than doing it right
3. **Loses trust** - Shows I didn't verify before coding

## How to Prevent

```bash
# Before writing any reference data config:
nwfth-sql "SELECT DISTINCT column FROM table"

# Verify against requirements
# Then write to .env
```

## Related Concepts

- Source of Truth
- Database-Driven Development
- Configuration Validation
