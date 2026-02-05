# Chase Transaction Sync Implementation Plan

## Overview

Add the ability to synchronize transaction history from Chase bank accounts using browser automation to download QFX files, then parse them into the keepbook transaction format.

## Approach

**Strategy:** Automated QFX download via browser automation

Rather than scraping HTML or using unofficial APIs, we automate the browser to:
1. Log in to chase.com (user handles 2FA)
2. Navigate to account download pages
3. Trigger QFX file downloads
4. Parse the structured QFX/OFX format

**Why this approach:**
- QFX/OFX is a stable, standardized format (won't break with UI changes)
- Parsing is trivial and reliable
- Up to 7 years of transaction history available
- Follows existing Schwab synchronizer pattern for browser auth
- Avoids Chase's anti-scraping measures (we're just downloading files)

## Research Summary

### Chase Download Options
- Chase offers QFX (Quicken) and CSV download formats
- QFX is OFX-compatible (SGML-based financial data format)
- Download available via: Account > Activity > Download account activity
- Date range selection available
- Chase discontinued Direct Connect/OFX in October 2022 (no automated OFX fetch)

### QFX/OFX Format
- SGML-based (OFX 1.x) or XML-based (OFX 2.x)
- Contains `FITID` (Financial Institution Transaction ID) for deduplication
- Transaction data includes: date, amount, description, type, status
- Rust crate `ofxy` available for parsing (supports OFX 1.6)

### Alternative Options Considered
1. **Plaid** - Official API, costs money for production
2. **Screen scraping** - Chase actively blocks this
3. **Unofficial APIs (coba)** - Python 2.7, likely broken
4. **Manual CSV import** - Works but not automated

## Implementation Phases

### Phase 1: Proof of Concept (Current)

**Status:** In progress

**Goal:** Validate that browser automation + file download capture works

**Files created:**
- `examples/chase_browser_poc.rs` - Browser automation test

**What it tests:**
- Chrome launches with anti-detection flags
- User can complete login + 2FA
- Download directory capture works
- Network interception for QFX content (optional)

**To run:**
```bash
cargo run --example chase_browser_poc
```

**Manual steps during POC:**
1. Log in to Chase
2. Navigate to an account
3. Click "Download account activity"
4. Select date range and QFX format
5. Download

### Phase 2: QFX Parser

**Goal:** Parse QFX files into keepbook Transaction structs

**Tasks:**
- [ ] Evaluate `ofxy` crate vs writing custom parser
- [ ] Create `src/sync/chase/qfx.rs` with parsing logic
- [ ] Map OFX transaction fields to keepbook Transaction:
  - `FITID` → `synchronizer_data.fitid` (for deduplication)
  - `DTPOSTED` → `timestamp`
  - `TRNAMT` → `amount`
  - `NAME`/`MEMO` → `description`
  - `TRNTYPE` → transaction metadata
- [ ] Handle pending vs posted transactions
- [ ] Write tests with sample QFX files

**Data mapping:**
```rust
// OFX Transaction → Keepbook Transaction
Transaction {
    id: Id::from_external(&fitid),  // Stable ID from FITID
    timestamp: parse_ofx_date(dtposted),
    amount: trnamt.to_string(),     // Negative for debits
    asset: Asset::currency("USD"),
    description: name_or_memo,
    status: TransactionStatus::Posted,
    synchronizer_data: json!({
        "fitid": fitid,
        "trntype": trntype,
        "memo": memo,
    }),
}
```

### Phase 3: Browser Login (Like Schwab)

**Goal:** Automate login with session capture

**Tasks:**
- [ ] Create `src/sync/synchronizers/chase.rs`
- [ ] Implement `InteractiveAuth` trait for Chase
- [ ] Launch browser, navigate to chase.com
- [ ] Wait for user to complete login + 2FA
- [ ] Capture session cookies
- [ ] Store session in `SessionCache` (like Schwab)
- [ ] Detect successful login (URL change or element presence)

**Session storage:**
```rust
SessionData {
    token: None,  // Chase doesn't expose bearer tokens like Schwab
    cookies: HashMap<String, String>,  // Session cookies
    captured_at: timestamp,
    data: {
        "accounts": [...],  // Discovered account info
    },
}
```

### Phase 4: Automated Download

**Goal:** Navigate to each account and trigger QFX downloads

**Tasks:**
- [ ] Enumerate accounts from Chase dashboard
- [ ] For each account:
  - [ ] Navigate to account activity page
  - [ ] Click download button
  - [ ] Select date range (last sync to now, or full history)
  - [ ] Select QFX format
  - [ ] Trigger download
  - [ ] Capture downloaded file
- [ ] Parse QFX and build SyncResult
- [ ] Handle multiple account types (checking, savings, credit card)

**Download capture methods:**
1. **CDP SetDownloadBehavior** - Direct download to known path
2. **File watching** - Monitor download directory for new files
3. **Network interception** - Capture response body directly

### Phase 5: Full Synchronizer Integration

**Goal:** Complete Chase synchronizer with CLI integration

**Tasks:**
- [ ] Implement `Synchronizer` trait
- [ ] Register in `src/sync/synchronizers/mod.rs`
- [ ] Add connection config support:
  ```toml
  name = "Chase Bank"
  synchronizer = "chase"
  ```
- [ ] Support `login` command for interactive auth
- [ ] Support `sync` command for data fetch
- [ ] Handle incremental sync (only fetch new transactions)
- [ ] Store last sync cursor in `synchronizer_data`

## File Structure

```
src/sync/
├── synchronizers/
│   ├── mod.rs          # Add chase to factory
│   ├── chase.rs        # Main synchronizer
│   └── ...
└── chase/
    ├── mod.rs
    ├── qfx.rs          # QFX/OFX parser
    └── browser.rs      # Browser automation helpers

examples/
└── chase_browser_poc.rs  # POC (created)
```

## Risks and Mitigations

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Chase detects automation | Medium | Anti-detection flags, user does manual login |
| Download UI changes | Low | QFX format is stable; only navigation breaks |
| 2FA blocks automation | N/A | User completes 2FA manually |
| Session expires quickly | Medium | Re-login when needed, cache session |
| Rate limiting | Low | Sync infrequently, batch downloads |

## Testing Strategy

1. **Unit tests**: QFX parser with sample files
2. **Integration tests**: Mock browser interactions
3. **Manual testing**: Full flow with real Chase account
4. **Sample files**: Collect QFX samples from different account types

## Dependencies

Existing:
- `chromiumoxide` - Browser automation (already used for Schwab)
- `tokio` - Async runtime
- `serde` - Serialization

New (maybe):
- `ofxy` - OFX parsing (evaluate vs custom parser)

## Open Questions

1. Does `ofxy` crate handle Chase's QFX files correctly?
2. What's the best way to enumerate accounts from the Chase dashboard?
3. How long do Chase sessions last before requiring re-login?
4. Should we support CSV as a fallback if QFX fails?

## Next Steps

1. Run the POC (`cargo run --example chase_browser_poc`)
2. Verify download capture works
3. Get a sample QFX file to test parsing
4. Implement QFX parser
5. Automate the download navigation
