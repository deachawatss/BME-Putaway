# BME Putaway Bin Transfer - Transaction Flow Analysis

> Documented from SQL trace of legacy BME Putaway system
> Date: 2026-02-16
> Test Case: Transfer 500 units from K0802-4B to WHKON1

## Overview

This document captures the complete database transaction flow for a bin transfer operation in the legacy BME Putaway system. Understanding this flow is critical for replicating the behavior in the new Putaway Bin Transfer System.

## Pre-Transfer State (LotMaster)

### Source Bin (K0802-4B)
| Field | Value |
|-------|-------|
| LotNo | 2600107-1 |
| ItemKey | INBC1403 |
| LocationKey | TFC1 |
| QtyOnHand | 975 |
| Qtycommitsales | 50 |
| QtyReserved | 0 |
| VendorKey | NZSUS |
| DateExpiry | 2027-05-07 |

### Destination Bin (WHKON1)
| Field | Value |
|-------|-------|
| LotNo | 2600107-1 |
| ItemKey | INBC1403 |
| LocationKey | TFC1 |
| QtyOnHand | 3350 |
| Qtycommitsales | 0 |

---

## Phase 1: Validation Queries

### 1.1 Available Bins Lookup
```sql
select BinNo, Location, Description
from (
    select distinct b.BinNo, b.Locationkey Location, Description
    from BinMaster a
    right outer join lotmaster b on a.binNo=b.binNo and a.location=b.locationkey
    Where b.binNo <> 'K0802-4B' and b.Locationkey = 'TFC1'
        and rtrim(ltrim(b.binNo)) <> ''
    Union
    select BinNo, Location, Description
    from binmaster
    where location='TFC1' and Binno<>'K0802-4B'
) c
order by c.BinNo
```
**Purpose**: Get list of available destination bins (excluding source bin)

### 1.2 Lot Details Query
```sql
select a.DateExpiry, a.DateReceived, a.QtyOnHand,
       b.Seriallotflg, b.stockuomCode StockUOM
from Lotmaster a
INNER JOIN inmast b on a.itemkey=b.itemKey
where b.seriallotFlg <> 'N'
    and B.MultipleBinsreq = 'Y'
    and a.binNo='K0802-4B'
    and a.locationKey = 'TFC1'
    and a.itemKey ='INBC1403'
    and a.lotNo ='2600107-1'
```
**Purpose**: Validate lot can be transferred (serial lot, multiple bins enabled)

### 1.3 Commitment Calculation (CRITICAL)
```sql
Select SUM(QtyIssued) as Commitment
from (
    Select QtyIssued From LotTransaction
    where Processed IN ('N','P')
        and TransactionType In (2,3,5,7,9,10,12,16,17,20,21)
        and Itemkey = 'INBC1403'
        AND LocationKey = 'TFC1'
        AND LotNo = '2600107-1'
        AND BinNo = 'K0802-4B'
    Union all
    Select QtyIssued From QCLotTransaction
    where Processed IN ('N','P')
        and TransactionType In (2,3,5,7,9,10,12,16,17,20,21)
        and Itemkey = 'INBC1403'
        AND LocationKey = 'TFC1'
        AND LotNo = '2600107-1'
        AND BinNo = 'K0802-4B'
) AS X
```
**Purpose**: Calculate reserved/committed quantity from pending transactions

**TransactionType Values**:
| Type | Description |
|------|-------------|
| 2 | Purchase Return |
| 3 | Sales Issue |
| 5 | Mfg. Issue |
| 7 | Inventory Transfer |
| 9 | Inventory Adj. Negative |
| 10 | Damaged |
| 12 | Warehouse Move Out |
| 16 | Transfer Out |
| 17 | Move |
| 20 | Transfer Out |
| 21 | Sales Provisional |

### 1.4 Pending Transactions Query
```sql
Select LotNo, BinNo, IssueDocNo, IssueDocLineNo, QtyIssued, LotTranNo,
    (Case TransactionType
        When 1 Then 'Purchase Receipt'
        When 2 Then 'Purchase Return'
        When 3 Then 'Sales Issue'
        When 4 Then 'Sales Return'
        When 5 Then 'Mfg. Issue'
        When 6 Then 'Mfg. Return'
        When 7 Then 'Inventory Transfer'
        When 8 Then 'Inventory Adj. Positive'
        When 9 Then 'Inventory Adj. Negative'
        When 10 Then 'Damaged'
        When 11 Then 'Warehouse Move In'
        When 12 Then 'Warehouse Move Out'
        When 14 Then 'Physical Count'
        When 15 Then 'Transfer In'
        When 16 Then 'Transfer Out'
        When 17 Then 'Move'
        When 18 Then 'Mfg. Receipt'
        When 21 Then 'Sales Provisional'
    end) as TranTyp
from LotTransaction
Where Processed IN ('N','P')
    and TransactionType In (2,3,5,7,9,10,12,16,17,20,21)
    and Itemkey = 'INBC1403'
    AND LocationKey = 'TFC1'
    AND LotNo = '2600107-1'
    AND BinNo = 'K0802-4B'
UNION ALL
Select LotNo, BinNo, IssueDocNo, IssueDocLineNo, QtyIssued, LotTranNo, ...
from QCLotTransaction
Where Processed IN ('N','P')
    and TransactionType In (2,3,5,7,9,10,12,16,17,20,21)
    and Itemkey = 'INBC1403'
    AND LocationKey = 'TFC1'
    AND LotNo = '2600107-1'
    AND BinNo = 'K0802-4B'
```
**Purpose**: List all pending transactions affecting this lot/bin

---

## Phase 2: System Configuration Checks

Multiple system parameter queries are executed:

```sql
-- Distribution parameters
select * from distributionparameter where Default_key = 'MobileFormHeigthandWidth' and Module_code='WHM'
select * from distributionparameter where Default_key = 'SHOW_CAPTIONS' and Module_code='WHM'

-- Default settings
Select * from mgsdflt
Select * From roundingdflt

-- Manufacturing connection switches
Select ScreenOption, SubScreenOption, Switch, SkipNextStep
from Customlogicswitch
where ScreenOption = 'clsMfgConnection'

-- Time zone handling
if not exists (select * from mgsdflt where Default_Key='TIME_ZONEDIFF')
begin
    INSERT INTO mgsdflt (Default_Key, Default_Value, Description, Module_Code)
    VALUES ('TIME_ZONEDIFF', 0, 'time zone difference', 'VER')
End

-- Inventory freeze check
SELECT Default_Value from DistributionParameter Where Default_Key = 'Freeze_Inventory'

-- Physical count in progress check
select Physinprogress from INLOC where ItemKey = 'INBC1403' AND Location = 'TFC1'
```

---

## Phase 3: Document Number Generation

```sql
-- Open cursor on SeqNum table
exec sp_cursoropen @p1 output,
    N'select * from SeqNum where SeqName=''BT''',
    @p3 output, @p4 output, @p5 output

-- Fetch sequence
exec sp_cursorfetch 180150003, 16, 1, 64

-- Update sequence (increment)
exec sp_cursor 180150003, 33, 1, N'SeqNum', @SeqNum=26112174

-- Close cursor
exec sp_cursorclose 180150003
```

**Result**: New Document Number = **BT-26112174**

---

## Phase 4: Transfer Commit (ATOMIC OPERATION)

### Step 4.1: Retrieve Vendor Information
```sql
SELECT VendorKey, VendorLotNo FROM LotMaster
WHERE ItemKey = 'INBC1403'
    AND LocationKey = 'TFC1'
    AND LotNo = '2600107-1'
    AND BinNo = 'K0802-4B'
```

### Step 4.2: Get Date Expiry
```sql
SELECT DateExpiry FROM LotMaster
WHERE ItemKey = 'INBC1403'
    AND LocationKey = 'TFC1'
    AND LotNo = '2600107-1'
    AND BinNo = 'K0802-4B'
```

### Step 4.3: UPDATE Source Bin - Reserve Quantity (CRITICAL)
```sql
UPDATE LotMaster
SET Qtycommitsales = Qtycommitsales + 500
WHERE ItemKey='INBC1403'
    AND LocationKey='TFC1'
    AND LotNo='2600107-1'
    AND BinNo = 'K0802-4B'
```
**Effect**: Increases commitment on source bin by transfer quantity (500)

**New Source Bin State**:
- QtyOnHand: 975 (unchanged)
- Qtycommitsales: 50 + 500 = 550
- Available: 975 - 550 = 425

### Step 4.4: Check Destination Bin Exists
```sql
SELECT count(*) FROM LotMaster
WHERE ItemKey = 'INBC1403'
    AND LocationKey = 'TFC1'
    AND LotNo = '2600107-1'
    AND BinNo = 'WHKON1'
```
**Result**: Returns 1 (destination lot exists)

---

## Phase 5: Transaction Recording (AUDIT TRAIL)

### Step 5.1: INSERT Source Transaction (OUT)
```sql
INSERT INTO "BME882024".."LotTransaction" (
    "LotNo",
    "ItemKey",
    "LocationKey",
    "DateReceived",
    "DateExpiry",
    "TransactionType",
    "VendorlotNo",
    "IssueDocNo",
    "IssueDocLineNo",
    "IssueDate",
    "QtyIssued",
    "RecUserid",
    "RecDate",
    "Processed",
    "BinNo"
) VALUES (
    '2600107-1',        -- LotNo
    'INBC1403',         -- ItemKey
    'TFC1',             -- LocationKey
    '2025-08-07 08:36:02',  -- DateReceived
    '2027-05-07 00:00:00',  -- DateExpiry
    9,                  -- TransactionType = Inventory Adj. Negative
    '07-05-25',         -- VendorlotNo
    'BT-26112174',      -- IssueDocNo
    1,                  -- IssueDocLineNo
    '2026-02-16 00:00:00',  -- IssueDate
    500,                -- QtyIssued
    'DECHAWAT',         -- RecUserid
    '2026-02-16 00:00:00',  -- RecDate
    'N',                -- Processed = Not processed
    N'K0802-4B'         -- BinNo (Source)
)
```

**Transaction Type**: 9 = Inventory Adj. Negative (issued from source)

### Step 5.2: INSERT Destination Transaction (IN)
```sql
INSERT INTO "BME882024".."LotTransaction" (
    "LotNo",
    "ItemKey",
    "LocationKey",
    "DateReceived",
    "DateExpiry",
    "TransactionType",
    "ReceiptDocNo",
    "ReceiptDocLineNo",
    "QtyReceived",
    "Vendorkey",
    "VendorlotNo",
    "CustomerKey",
    "RecUserid",
    "RecDate",
    "Processed",
    "BinNo",
    "DateQuarantine"
) VALUES (
    '2600107-1',        -- LotNo
    'INBC1403',         -- ItemKey
    'TFC1',             -- LocationKey
    '2025-08-07 08:36:02',  -- DateReceived
    '2027-05-07 00:00:00',  -- DateExpiry
    8,                  -- TransactionType = Inventory Adj. Positive
    'BT-26112174',      -- ReceiptDocNo
    1,                  -- ReceiptDocLineNo
    500,                -- QtyReceived
    'NZSUS',            -- Vendorkey
    '07-05-25',         -- VendorlotNo
    '',                 -- CustomerKey
    'DECHAWAT',         -- RecUserid
    '2026-02-16 00:00:00',  -- RecDate
    'N',                -- Processed = Not processed
    N'WHKON1',          -- BinNo (Destination)
    NULL                -- DateQuarantine
)
```

**Transaction Type**: 8 = Inventory Adj. Positive (received at destination)

---

## Summary: Key Business Logic

### Available Quantity Calculation
```
Available Qty = QtyOnHand - Qtycommitsales
```

### Transfer Validation
1. Source lot must exist with sufficient available quantity
2. Commitment = Sum of QtyIssued from pending LotTransaction/QCLotTransaction
3. System must not be in freeze mode
4. No physical count in progress for the item/location

### Transfer Commit Pattern
1. **Reserve**: UPDATE LotMaster.Qtycommitsales += transfer_qty (source bin)
2. **Audit Out**: INSERT LotTransaction with TransactionType=9 (negative adjustment)
3. **Audit In**: INSERT LotTransaction with TransactionType=8 (positive adjustment)
4. Both transactions share the same IssueDocNo/ReceiptDocNo (BT-XXXXXXX)

### Important Notes
- **Processed='N'**: Transactions are created as "Not Processed"
- **TransactionType 9**: Inventory Adjustment Negative (source)
- **TransactionType 8**: Inventory Adjustment Positive (destination)
- **Qtycommitsales**: Acts as a reservation/commitment mechanism
- **LotTransaction**: Primary audit table for all inventory movements
- **QCLotTransaction**: Quality control transaction table (also checked for commitments)

---

## Data Flow Diagram

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Source Bin     │     │   Transfer Op    │     │ Destination Bin │
│  K0802-4B       │────▶│   BT-26112174    │────▶│  WHKON1         │
│                 │     │                  │     │                 │
│ QtyOnHand: 975  │     │ 1. Reserve 500   │     │ QtyOnHand: 3350 │
│ QtyCommit: 50   │     │    (Update LM)   │     │ QtyCommit: 0    │
│                 │     │                  │     │                 │
│                 │     │ 2. INSERT LT-Out │     │                 │
│                 │     │    (Type 9)      │     │                 │
│                 │     │                  │     │                 │
│                 │     │ 3. INSERT LT-In  │     │                 │
│                 │     │    (Type 8)      │     │                 │
│                 │     │                  │     │                 │
│ QtyCommit: 550  │◄────│                  │────▶│ (Processed later│
│ (50+500)        │     │                  │     │  by batch job)  │
└─────────────────┘     └──────────────────┘     └─────────────────┘
                               │
                               ▼
                    ┌──────────────────────┐
                    │  LotTransaction      │
                    │  Doc: BT-26112174    │
                    │  Two records:        │
                    │  - Type 9 (Out)      │
                    │  - Type 8 (In)       │
                    │  Processed: 'N'      │
                    └──────────────────────┘
```

---

## Implementation Notes for New System

1. **Must replicate** the commitment calculation logic exactly
2. **Must use** same TransactionType values (8 and 9)
3. **Must generate** BT- prefixed document numbers from SeqNum table
4. **Must set** Processed='N' for new transactions
5. **Must update** Qtycommitsales on source bin immediately
6. **Destination bin QtyOnHand** is NOT updated immediately — only via the LotTransaction record

---

*Documented by Gale Oracle for Putaway Bin Transfer System*
*Source: SQL trace from legacy BME Putaway system (2026-02-16)*
