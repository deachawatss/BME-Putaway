# BME Putaway Transaction Patterns

> Lesson from SQL trace analysis of legacy BME system

## Core Pattern

### Available Quantity Formula
```
Available = QtyOnHand - QtyCommitSales
```

### Transfer Flow (3-Step)
1. **Validate**: Calculate commitment from LotTransaction + QCLotTransaction
2. **Reserve**: UPDATE LotMaster.QtyCommitSales (temporary)
3. **Audit**: INSERT two LotTransaction records (Type 9 OUT, Type 8 IN)

### Batch Job Processing
- Transactions inserted with `Processed='N'`
- Background job updates actual QtyOnHand
- Commitment released after processing
- **This causes UI freezing!**

## Two Transfer Patterns

| Aspect | Simple | Complex |
|--------|--------|---------|
| Container tracking | No | Yes (ContainerMaster) |
| UOM conversion | No | Yes (INQTYCNV) |
| Validation rounds | 1x | 3x |
| Connection reset | No | Yes |
| Speed | Fast | Slow |

## Performance Anti-Patterns

1. **Triple validation**: Same queries executed 3 times
2. **Connection churn**: `sp_reset_connection` mid-transaction
3. **Bloated switches**: 20+ CustomLogicSwitch queries
4. **Synchronous batch**: UI waits for background job

## New System Recommendations

```sql
-- Direct update pattern (no batch job)
BEGIN TRANSACTION;
UPDATE LotMaster SET QtyOnHand -= @qty WHERE ...;  -- Source
UPDATE LotMaster SET QtyOnHand += @qty WHERE ...;  -- Destination
INSERT INTO LotTransaction ... ;  -- Audit only, Processed='Y'
COMMIT;
```

## Validation Checklist

- [ ] Query commitment once, cache result
- [ ] Skip ContainerMaster if item has no containers
- [ ] Skip UOM conversion if source == destination
- [ ] Use row-level locking (ROWLOCK, UPDLOCK)
- [ ] Implement optimistic concurrency

## Related Files

- `Docs/Putaway-Bme-Transaction.md` - Complete analysis
- `~/.claude/skills/nwfth-sql/SKILL.md` - Database patterns

---

*Logged via rrr - 2026-02-16*
