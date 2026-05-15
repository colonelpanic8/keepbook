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
