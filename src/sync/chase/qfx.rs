//! Minimal QFX/OFX (OFX 1.x SGML) parser for Chase downloads.
//!
//! We intentionally parse only the subset we need:
//! - account id/type/currency
//! - ledger balance (optional)
//! - STMTTRN list with FITID/DTPOSTED/TRNAMT/NAME/MEMO and a few extras

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    Bank,
    CreditCard,
}

#[derive(Debug, Clone)]
pub struct QfxStatement {
    pub kind: StatementKind,
    pub currency: Option<String>,
    pub account_id: String,
    pub account_type: Option<String>,
    pub ledger_balance: Option<Decimal>,
    pub ledger_balance_as_of: Option<DateTime<Utc>>,
    pub transactions: Vec<QfxTransaction>,
}

#[derive(Debug, Clone)]
pub struct QfxTransaction {
    pub fitid: String,
    pub posted_at: DateTime<Utc>,
    pub amount: Decimal,
    pub trn_type: Option<String>,
    pub name: Option<String>,
    pub memo: Option<String>,
    pub check_num: Option<String>,
    pub ref_num: Option<String>,
}

impl QfxStatement {
    pub fn parse(content: &str) -> Result<Self> {
        let ofx = extract_ofx_body(content);
        let ofx_u = ofx.to_ascii_uppercase();

        // Identify statement kind + the right "account from" container.
        let (kind, acct_container_tag) = if ofx_u.contains("<CCSTMTTRNRS>")
            || ofx_u.contains("<CREDITCARDMSGSRSV1>")
            || ofx_u.contains("<CCSTMTRS>")
        {
            (StatementKind::CreditCard, "CCACCTFROM")
        } else {
            (StatementKind::Bank, "BANKACCTFROM")
        };

        let currency = extract_tag_value(ofx, "CURDEF").map(|s| s.trim().to_string());

        let acct_container = extract_block(ofx, acct_container_tag)
            .with_context(|| format!("Missing <{acct_container_tag}> block"))?;
        let account_id = extract_tag_value(acct_container, "ACCTID")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .with_context(|| format!("Missing <ACCTID> within <{acct_container_tag}>"))?;
        let account_type =
            extract_tag_value(acct_container, "ACCTTYPE").map(|s| s.trim().to_string());

        // Ledger balance is optional.
        let ledger_block = extract_block(ofx, "LEDGERBAL");
        let (ledger_balance, ledger_balance_as_of) = match ledger_block {
            None => (None, None),
            Some(b) => {
                let bal = extract_tag_value(b, "BALAMT")
                    .map(|s| Decimal::from_str(s.trim()))
                    .transpose()
                    .context("Invalid <BALAMT> decimal")?;
                let as_of = extract_tag_value(b, "DTASOF")
                    .map(|s| parse_ofx_datetime(s.trim()))
                    .transpose()
                    .context("Invalid <DTASOF>")?;
                (bal, as_of)
            }
        };

        // Transactions.
        let mut transactions = Vec::new();
        for trn_block in extract_blocks(ofx, "STMTTRN") {
            let fitid = extract_tag_value(trn_block, "FITID")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .with_context(|| "Missing <FITID> in <STMTTRN>".to_string())?;
            let posted_raw = extract_tag_value(trn_block, "DTPOSTED")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .with_context(|| format!("Missing <DTPOSTED> for FITID={fitid}"))?;
            let posted_at = parse_ofx_datetime(&posted_raw)
                .with_context(|| format!("Invalid <DTPOSTED> for FITID={fitid}: {posted_raw}"))?;

            let amt_raw = extract_tag_value(trn_block, "TRNAMT")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .with_context(|| format!("Missing <TRNAMT> for FITID={fitid}"))?;
            let amount = Decimal::from_str(&amt_raw)
                .with_context(|| format!("Invalid <TRNAMT> for FITID={fitid}: {amt_raw}"))?;

            let trn_type = extract_tag_value(trn_block, "TRNTYPE").map(|s| s.trim().to_string());
            let name = extract_tag_value(trn_block, "NAME").map(|s| s.trim().to_string());
            let memo = extract_tag_value(trn_block, "MEMO").map(|s| s.trim().to_string());
            let check_num = extract_tag_value(trn_block, "CHECKNUM").map(|s| s.trim().to_string());
            let ref_num = extract_tag_value(trn_block, "REFNUM").map(|s| s.trim().to_string());

            transactions.push(QfxTransaction {
                fitid,
                posted_at,
                amount,
                trn_type,
                name,
                memo,
                check_num,
                ref_num,
            });
        }

        Ok(Self {
            kind,
            currency,
            account_id,
            account_type,
            ledger_balance,
            ledger_balance_as_of,
            transactions,
        })
    }
}

fn extract_ofx_body(content: &str) -> &str {
    // QFX/OFX has a header block then the SGML body. We only care about the body.
    let u = content.to_ascii_uppercase();
    if let Some(idx) = u.find("<OFX>") {
        &content[idx..]
    } else {
        content
    }
}

fn extract_blocks<'a>(content: &'a str, tag: &str) -> Vec<&'a str> {
    let tag_u = tag.to_ascii_uppercase();
    let u = content.to_ascii_uppercase();
    let open = format!("<{tag_u}>");
    let close = format!("</{tag_u}>");

    let mut blocks = Vec::new();
    let mut search_from = 0usize;
    while let Some(start_rel) = u[search_from..].find(&open) {
        let start = search_from + start_rel + open.len();
        let after_open_u = &u[start..];
        let end = if let Some(end_rel) = after_open_u.find(&close) {
            start + end_rel
        } else {
            // Fall back: if closing tag is absent, stop at next opening tag or end.
            if let Some(next_rel) = after_open_u.find(&open) {
                start + next_rel
            } else {
                content.len()
            }
        };

        blocks.push(&content[start..end]);
        search_from = end.min(content.len());
    }

    blocks
}

fn extract_block<'a>(content: &'a str, tag: &str) -> Option<&'a str> {
    extract_blocks(content, tag).into_iter().next()
}

fn extract_tag_value<'a>(content: &'a str, tag: &str) -> Option<&'a str> {
    let tag_u = tag.to_ascii_uppercase();
    let u = content.to_ascii_uppercase();
    let needle = format!("<{tag_u}>");
    let start = u.find(&needle)? + needle.len();
    let rest = &content[start..];
    let end = rest.find('<').unwrap_or(rest.len());
    let v = rest[..end].trim();
    if v.is_empty() {
        None
    } else {
        Some(v)
    }
}

fn parse_ofx_datetime(raw: &str) -> Result<DateTime<Utc>> {
    // OFX 1.x datetime often looks like:
    //   YYYYMMDD
    //   YYYYMMDDHHMMSS
    //   YYYYMMDDHHMMSS.XXX
    //   YYYYMMDDHHMMSS[-5:EST]
    // We'll parse the leading digits, then apply the bracketed hour offset if present.
    let mut digits = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
        } else {
            break;
        }
    }
    if digits.len() < 8 {
        anyhow::bail!("OFX datetime has fewer than 8 leading digits: {raw}");
    }

    let year: i32 = digits[0..4].parse()?;
    let month: u32 = digits[4..6].parse()?;
    let day: u32 = digits[6..8].parse()?;
    let (hour, minute, second) = if digits.len() >= 14 {
        (
            digits[8..10].parse()?,
            digits[10..12].parse()?,
            digits[12..14].parse()?,
        )
    } else {
        (0u32, 0u32, 0u32)
    };

    let local = Utc
        .with_ymd_and_hms(year, month, day, hour, minute, second)
        .single()
        .context("Invalid date/time components")?;

    // Optional timezone offset, e.g. "[-5:EST]". Apply UTC = local - offset.
    if let Some(bi) = raw.find('[') {
        if let Some(end) = raw[bi + 1..].find(']') {
            let inner = &raw[bi + 1..bi + 1 + end];
            // Offset is the first chunk before ':' (or the whole string if no ':')
            let off_part = inner.split(':').next().unwrap_or(inner).trim();
            if !off_part.is_empty() {
                // Support -5, +3, -0500, +0530.
                let sign = if off_part.starts_with('-') {
                    -1i32
                } else if off_part.starts_with('+') {
                    1i32
                } else {
                    1i32
                };
                let off_digits: String = off_part.chars().filter(|c| c.is_ascii_digit()).collect();
                if !off_digits.is_empty() {
                    let (hh, mm) = if off_digits.len() <= 2 {
                        (off_digits.parse::<i32>()?, 0i32)
                    } else if off_digits.len() == 4 {
                        (
                            off_digits[0..2].parse::<i32>()?,
                            off_digits[2..4].parse::<i32>()?,
                        )
                    } else {
                        // Unexpected; ignore.
                        return Ok(local);
                    };
                    let offset_secs = sign * (hh * 3600 + mm * 60);
                    let utc = local - chrono::Duration::seconds(offset_secs.into());
                    return Ok(utc);
                }
            }
        }
    }

    Ok(local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parse_qfx_bank_minimal() {
        let qfx = r#"OFXHEADER:100
DATA:OFXSGML
VERSION:102
SECURITY:NONE
ENCODING:USASCII
CHARSET:1252
COMPRESSION:NONE
OLDFILEUID:NONE
NEWFILEUID:NONE

<OFX>
<BANKMSGSRSV1>
<STMTTRNRS>
<TRNUID>1
<STATUS>
<CODE>0
<SEVERITY>INFO
</STATUS>
<STMTRS>
<CURDEF>USD
<BANKACCTFROM>
<BANKID>021000021
<ACCTID>123456789
<ACCTTYPE>CHECKING
</BANKACCTFROM>
<BANKTRANLIST>
<DTSTART>20260101000000[-5:EST]
<DTEND>20260201000000[-5:EST]
<STMTTRN>
<TRNTYPE>DEBIT
<DTPOSTED>20260205120000[-5:EST]
<TRNAMT>-12.3400
<FITID>202602050001
<NAME>COFFEE SHOP
<MEMO>LATTE
</STMTTRN>
</BANKTRANLIST>
<LEDGERBAL>
<BALAMT>1000.00
<DTASOF>20260206120000[-5:EST]
</LEDGERBAL>
</STMTRS>
</STMTTRNRS>
</BANKMSGSRSV1>
</OFX>"#;

        let stmt = QfxStatement::parse(qfx).unwrap();
        assert_eq!(stmt.kind, StatementKind::Bank);
        assert_eq!(stmt.currency.as_deref(), Some("USD"));
        assert_eq!(stmt.account_id, "123456789");
        assert_eq!(stmt.account_type.as_deref(), Some("CHECKING"));
        assert_eq!(stmt.ledger_balance.unwrap().normalize().to_string(), "1000");
        assert_eq!(
            stmt.ledger_balance_as_of.unwrap(),
            Utc.with_ymd_and_hms(2026, 2, 6, 17, 0, 0).unwrap()
        );
        assert_eq!(stmt.transactions.len(), 1);
        let t = &stmt.transactions[0];
        assert_eq!(t.fitid, "202602050001");
        assert_eq!(t.amount.normalize().to_string(), "-12.34");
        assert_eq!(
            t.posted_at,
            Utc.with_ymd_and_hms(2026, 2, 5, 17, 0, 0).unwrap()
        );
        assert_eq!(t.name.as_deref(), Some("COFFEE SHOP"));
        assert_eq!(t.memo.as_deref(), Some("LATTE"));
        assert_eq!(t.trn_type.as_deref(), Some("DEBIT"));
    }

    #[test]
    fn parse_ofx_datetime_date_only() {
        let dt = parse_ofx_datetime("20260205").unwrap();
        assert_eq!(dt, Utc.with_ymd_and_hms(2026, 2, 5, 0, 0, 0).unwrap());
    }
}
