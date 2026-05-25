# FinanceManager Local Data Schema (No Database)

## 1. Design principles
- Local-only persistence.
- No database engines.
- File-based storage by user and period.
- Append-safe audit logs for manual overrides.
- Deterministic re-build of analytics from source files.

## 2. Root folder layout

```text
FinanceManager/
  data/
    users/
      <userId>/
        profile.json
        settings.json
        mappings/
          merchant-canonical-map.json
          category-rules.json
        source/
          uploads/
            YYYY/
              MM/
                <importId>/
                  file-meta.json
                  original/
                    *.pdf
                  parsed/
                    raw-transactions.json
                    normalized-transactions.csv
                    parse-errors.json
        working/
          flags/
            YYYY-MM.flags.json
          adjustments/
            YYYY-MM.adjustments.json
        domain/
          investments.json
          loans.json
        analytics/
          monthly/
            YYYY-MM.summary.json
            YYYY-MM.categories.csv
            YYYY-MM.trends.json
            YYYY-MM.health-score.json
          history/
            historical-summary.json
        exports/
          YYYY/
            MM/
              FinanceManager-YYYY-MM.xlsx
        audit/
          events-YYYY-MM.jsonl
```

## 3. File contract details

### 3.1 profile.json
Stores user identity metadata and isolation key.

```json
{
  "userId": "user-001",
  "displayName": "Asha",
  "createdAt": "2026-05-25T10:00:00Z",
  "timezone": "Asia/Kolkata",
  "currency": "INR"
}
```

### 3.2 settings.json
User-level preferences and policy toggles.

```json
{
  "internetLookupEnabled": false,
  "requireManualApprovalForLookup": true,
  "encryptionEnabled": true,
  "dashboardActiveWindowMonths": 2,
  "allowFlagBypassWithReason": true
}
```

### 3.3 merchant-canonical-map.json
Known merchant aliases normalized to canonical names and default category.

```json
{
  "AMZN MKTPLACE": {
    "canonicalName": "Amazon",
    "defaultCategory": "Shopping",
    "confidence": 0.98,
    "source": "manual"
  },
  "SWIGGY INSTAMART": {
    "canonicalName": "Swiggy",
    "defaultCategory": "Food",
    "confidence": 0.93,
    "source": "inference"
  }
}
```

### 3.4 category-rules.json
Rule-driven categorization pipeline.

```json
{
  "version": 1,
  "rules": [
    {
      "ruleId": "r-001",
      "matchType": "contains",
      "matchValue": "UPI/RENT",
      "category": "Housing",
      "priority": 100,
      "active": true
    },
    {
      "ruleId": "r-002",
      "matchType": "regex",
      "matchValue": "(?i)salary|payroll",
      "category": "Income",
      "priority": 120,
      "active": true
    }
  ]
}
```

### 3.5 file-meta.json
Upload traceability for each import batch.

```json
{
  "importId": "imp-2026-05-25-01",
  "uploadedAt": "2026-05-25T11:20:00Z",
  "files": [
    {
      "name": "hdfc-cc-apr.pdf",
      "sha256": "<hash>",
      "statementType": "credit-card",
      "institution": "HDFC",
      "period": {
        "from": "2026-04-01",
        "to": "2026-04-30"
      }
    }
  ]
}
```

### 3.6 raw-transactions.json
Parser output with minimal transformation. Every row carries full source provenance.

```json
[
  {
    "importId": "imp-2026-05-25-01",
    "sourceFile": "hdfc-cc-apr.pdf",
    "sourceSha256": "<hash>",
    "sourcePage": 2,
    "rowNumber": 38,
    "parserVersion": "hdfc-cc@1.0.0",
    "parserBackend": "pdfium",
    "txnDate": "2026-04-18",
    "description": "SWIGGY INSTAMART",
    "debit": 850.0,
    "credit": 0.0,
    "balance": null
  }
]
```

Source provenance fields:

| Field | Purpose |
|---|---|
| `importId` | Links the row back to its batch in [`file-meta.json`](#35-file-metajson). The same filename can recur across imports; `importId` disambiguates. |
| `sourceFile` | Original PDF filename inside the import. |
| `sourceSha256` | Content hash of the source PDF. Survives renames; primary key for duplicate-import detection. |
| `sourcePage` | 1-based page number within the PDF. Speeds debugging when `rowNumber` alone is ambiguous. |
| `rowNumber` | Row position within the extracted text stream (or 1-based row index within `sourcePage`, adapter's choice — declared by the adapter). |
| `parserVersion` | SemVer of the bank adapter that produced this row (`<adapterId>@<version>`). When a parser changes, we know which rows are eligible for re-extraction. |
| `parserBackend` | `pdfium` \| `pdf-extract` \| `ocr-tesseract`. When backends disagree, we know which one this row came from. |

### 3.7 normalized-transactions.csv
Canonical transaction rows used by analytics. Source-provenance columns are appended at the end so the analytics-relevant fields read first.

```csv
transactionId,userId,txnDate,postDate,amount,currency,direction,kind,merchantRaw,merchantCanonical,category,subCategory,accountType,accountMask,isTransfer,isReimbursement,linkedTransactionId,confidence,flagged,flagReason,importId,sourceFile,sourceSha256,sourcePage,sourceRow,parserVersion,parserBackend
trx-001,user-001,2026-04-18,2026-04-18,850,INR,debit,expense,SWIGGY INSTAMART,Swiggy,Food,Groceries,credit-card,XXXX1234,false,false,,0.93,false,,imp-2026-05-25-01,hdfc-cc-apr.pdf,<sha256>,2,38,hdfc-cc@1.0.0,pdfium
```

Source-provenance columns mirror the fields documented in §3.6.

### 3.8 parse-errors.json
Rows that could not be parsed reliably.

```json
[
  {
    "sourceFile": "sbi-apr.pdf",
    "rowHint": "line 45",
    "errorCode": "UNSUPPORTED_ROW_FORMAT",
    "message": "Unable to detect amount columns",
    "severity": "warning"
  }
]
```

### 3.9 YYYY-MM.flags.json
Manual review queue for uncertain records.

```json
{
  "month": "2026-04",
  "openFlags": [
    {
      "flagId": "f-001",
      "transactionId": "trx-017",
      "type": "CATEGORY_UNCERTAIN",
      "reason": "No category confidence above threshold",
      "status": "open",
      "createdAt": "2026-05-25T11:30:00Z"
    }
  ],
  "resolvedFlags": []
}
```

### 3.10 YYYY-MM.adjustments.json
Reimbursements, split expenses, and user corrections.

```json
{
  "month": "2026-04",
  "adjustments": [
    {
      "adjustmentId": "a-001",
      "transactionId": "trx-102",
      "type": "partial-reimbursement",
      "amount": 500.0,
      "effectiveExpenseAfterAdjustment": 350.0,
      "note": "Friend paid back dinner split",
      "createdBy": "user-001",
      "createdAt": "2026-05-25T11:40:00Z"
    }
  ]
}
```

### 3.11 investments.json
Manual investment portfolio records.

```json
{
  "assets": [
    {
      "assetId": "inv-001",
      "assetType": "MutualFund",
      "assetName": "Nifty 50 Index Fund",
      "investedAmount": 300000,
      "currentValue": 356000,
      "asOfDate": "2026-05-20",
      "notes": "Monthly SIP"
    }
  ]
}
```

### 3.12 loans.json
Loan tracking master file.

```json
{
  "loans": [
    {
      "loanId": "loan-001",
      "loanType": "HomeLoan",
      "lender": "SBI",
      "principalOutstanding": 2850000,
      "interestRate": 8.6,
      "rateType": "floating",
      "remainingTenureMonths": 196,
      "emi": 28650,
      "prepaymentPenalty": "none",
      "taxBenefitEligible": true,
      "nextDueDate": "2026-06-05",
      "classification": "good",
      "classificationReason": "Tax benefit + low effective rate"
    }
  ]
}
```

### 3.13 Monthly analytics files
Generated files only. Never manually edited.

- YYYY-MM.summary.json: Totals and key KPIs.
- YYYY-MM.categories.csv: Category-level spend pivot.
- YYYY-MM.trends.json: Time-series points.
- YYYY-MM.health-score.json: Score and weighted factors.

Example summary:

```json
{
  "month": "2026-04",
  "income": 185000,
  "expensesGross": 121500,
  "reimbursements": 6200,
  "expensesNet": 115300,
  "investments": 25000,
  "netSavings": 44700,
  "bleedAmount": 14250
}
```

### 3.14 audit events-YYYY-MM.jsonl
Append-only per action event log.

```json
{"ts":"2026-05-25T11:45:00Z","userId":"user-001","action":"flag-resolved","entityId":"f-001","details":{"transactionId":"trx-017","newCategory":"Utilities"}}
{"ts":"2026-05-25T11:47:00Z","userId":"user-001","action":"adjustment-added","entityId":"a-001","details":{"transactionId":"trx-102","type":"partial-reimbursement","amount":500}}
```

## 4. Processing lifecycle
1. Upload PDFs to source uploads folder.
2. Parse into raw transactions.
3. Normalize and classify into canonical CSV.
4. Generate flags for uncertain rows.
5. Require manual resolution or approved defer.
6. Apply adjustments for reimbursements/splits.
7. Build monthly analytics artifacts.
8. Enable export when open flags count equals zero.

## 5. Multi-user isolation rules
- Every user has a separate root at data/users/<userId>/.
- No cross-user read/write operations.
- Export paths and audit logs remain user-scoped.
- Session context always includes active userId.

## 6. Validation rules
- transactionId must be globally unique per user.
- amount must be positive, direction defines sign semantics.
- month keys must follow YYYY-MM.
- parse errors and unresolved flags must be retained for traceability.
- analytics cannot run if required source artifacts are missing.
- every transaction row (raw and normalized) must carry the seven source-provenance fields documented in §3.6; rows missing any of them are rejected at import.
- `(importId, sourceFile, sourcePage, rowNumber)` is the natural key for re-locating a transaction in its original PDF; collisions within a single import are a parser bug.
- `parserVersion` bumps trigger re-extraction eligibility: when an adapter's version advances, the affected `importId` batches can be re-parsed without re-uploading the PDF.

## 7. Recommended implementation notes
- Prefer JSON for nested entities, CSV for large tabular transactions.
- Use atomic write pattern: write temp file then rename.
- Keep schemaVersion in every generated JSON.
- Add checksum files for export reproducibility.
