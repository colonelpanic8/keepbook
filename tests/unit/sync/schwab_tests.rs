use super::*;
use crate::credentials::SessionData;
use crate::models::Id;
use serde_json::json;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[test]
fn parse_exported_session_strips_bearer_prefix() {
    let json = r#"{"token":"Bearer test-token","cookies":{}}"#;
    let session = parse_exported_session(json).expect("parse session");
    assert_eq!(session.token.as_deref(), Some("test-token"));
}

#[test]
fn parse_exported_transactions_json_parses_rows_and_generates_deterministic_ids() {
    let json = r#"
[
  {
    "Date": "10/11/2024",
    "Action": "Exchange or Exercise",
    "Symbol": "XPOA",
    "Description": "XPOA CBOE PUT NOV 24 7.5",
    "Quantity": "2",
    "Price": "0",
    "Fees & Comm": "0",
    "Amount": "0"
  },
  {
    "Date": "05/20/2024 as of 05/17/2024",
    "Action": "Dividend",
    "Symbol": "VTI",
    "Description": "VANGUARD TOTAL STOCK MARKET ETF",
    "Amount": "$1.23"
  }
]
"#;

    let account_id = Id::from_string("acct-1");
    let first = parse_exported_transactions_json(&account_id, json).expect("parse");
    assert_eq!(first.skipped, 0);
    assert_eq!(first.transactions.len(), 2);

    let second = parse_exported_transactions_json(&account_id, json).expect("parse");
    assert_eq!(first.transactions[0].id, second.transactions[0].id);
    assert_eq!(first.transactions[1].id, second.transactions[1].id);
    assert_eq!(first.transactions[1].amount, "1.23");
}

#[test]
fn parse_brokerage_transactions_rows_parses_and_generates_deterministic_ids() {
    let rows = vec![
        BrokerageTransactionRow {
            transaction_date: "02/10/2026".to_string(),
            action: Some("Buy".to_string()),
            symbol: Some("ADBE".to_string()),
            description: Some("ADOBE INC".to_string()),
            share_quantity: Some("75".to_string()),
            execution_price: Some("$265.1199".to_string()),
            fees_and_commission: Some(String::new()),
            amount: Some("-$19,883.99".to_string()),
            source_code: Some(String::new()),
            effective_date: Some("02/10/2026".to_string()),
            deposit_sequence_id: Some("0".to_string()),
            check_date: Some("02/11/2026".to_string()),
            item_issue_id: Some("1670511108".to_string()),
            schwab_order_id: Some("719598531600".to_string()),
        },
        BrokerageTransactionRow {
            transaction_date: "01/13/2026 as of 12/31/2025".to_string(),
            action: Some("Cash In Lieu".to_string()),
            symbol: Some("FG".to_string()),
            description: Some("F&G ANNUITIES & LIFE INC".to_string()),
            share_quantity: Some(String::new()),
            execution_price: Some(String::new()),
            fees_and_commission: Some(String::new()),
            amount: Some("$9.05".to_string()),
            source_code: Some("CIL".to_string()),
            effective_date: Some("12/31/2025".to_string()),
            deposit_sequence_id: Some("1".to_string()),
            check_date: Some("01/13/2026".to_string()),
            item_issue_id: Some("84212712".to_string()),
            schwab_order_id: Some("0".to_string()),
        },
    ];

    let account_id = Id::from_string("acct-1");
    let first = parse_brokerage_transactions_rows(&account_id, &rows).expect("parse");
    assert_eq!(first.skipped, 0);
    assert_eq!(first.transactions.len(), 2);

    let second = parse_brokerage_transactions_rows(&account_id, &rows).expect("parse");
    assert_eq!(first.transactions[0].id, second.transactions[0].id);
    assert_eq!(first.transactions[1].id, second.transactions[1].id);
}

#[test]
fn parse_banking_transactions_rows_parses_and_generates_deterministic_ids() {
    let rows = vec![
        BankingTransactionRow {
            posting_date: "02/18/2026".to_string(),
            description: Some("BALLAST-CZB-6708 WEB PMTS".to_string()),
            type_label: Some("ACH".to_string()),
            withdrawal_amount: Some("$1,850.00".to_string()),
            deposit_amount: Some(String::new()),
            running_balance: Some("$9,923.96".to_string()),
            check_sequence_number: Some("0".to_string()),
            check_number: None,
            deposit_check_id: None,
        },
        BankingTransactionRow {
            posting_date: "02/14/2026".to_string(),
            description: Some("PAYROLL".to_string()),
            type_label: Some("DIRECT DEPOSIT".to_string()),
            withdrawal_amount: Some(String::new()),
            deposit_amount: Some("$4,000.00".to_string()),
            running_balance: Some("$11,773.96".to_string()),
            check_sequence_number: Some("0".to_string()),
            check_number: None,
            deposit_check_id: None,
        },
    ];

    let account_id = Id::from_string("acct-2");
    let first = parse_banking_transactions_rows(&account_id, &rows).expect("parse");
    assert_eq!(first.skipped, 0);
    assert_eq!(first.transactions.len(), 2);
    assert_eq!(first.transactions[0].amount, "-1850.00");
    assert_eq!(first.transactions[1].amount, "4000.00");

    let second = parse_banking_transactions_rows(&account_id, &rows).expect("parse");
    assert_eq!(first.transactions[0].id, second.transactions[0].id);
    assert_eq!(first.transactions[1].id, second.transactions[1].id);
}

#[tokio::test]
async fn get_brokerage_transactions_paginates_with_bookmark() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
            .and(path(
                "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions",
            ))
            .and(body_partial_json(json!({
                "timeFrame": "All",
                "bookmark": null
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "bookmark": {
                    "fromKey": { "primarySortCode": null, "primarySortValue": "" },
                    "fromExecutionDate": "2022-12-21T00:00:00",
                    "fromPublTimeStamp": "2022-12-21 13:27:00.423163",
                    "fromSecondarySortCode": "4",
                    "fromSecondarySortValue": "FG",
                    "fromTertiarySortValue": "0.00000"
                },
                "brokerageTransactions": [
                    {
                        "transactionDate": "02/10/2026",
                        "action": "Buy",
                        "symbol": "ADBE",
                        "description": "ADOBE INC",
                        "shareQuantity": "75",
                        "executionPrice": "$265.1199",
                        "feesAndCommission": "",
                        "amount": "-$19,883.99",
                        "sourceCode": "",
                        "effectiveDate": "02/10/2026",
                        "depositSequenceId": "0",
                        "checkDate": "02/11/2026",
                        "itemIssueId": "1670511108",
                        "schwabOrderId": "719598531600"
                    }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

    Mock::given(method("POST"))
            .and(path(
                "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/brokerage/transactions",
            ))
            .and(body_partial_json(json!({
                "bookmark": {
                    "fromExecutionDate": "2022-12-21T00:00:00"
                }
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "bookmark": null,
                "brokerageTransactions": [
                    {
                        "transactionDate": "01/13/2026 as of 12/31/2025",
                        "action": "Cash In Lieu",
                        "symbol": "FG",
                        "description": "F&G ANNUITIES & LIFE INC",
                        "shareQuantity": "",
                        "executionPrice": "",
                        "feesAndCommission": "",
                        "amount": "$9.05",
                        "sourceCode": "CIL",
                        "effectiveDate": "12/31/2025",
                        "depositSequenceId": "1",
                        "checkDate": "01/13/2026",
                        "itemIssueId": "84212712",
                        "schwabOrderId": "0"
                    }
                ]
            })))
            .expect(1)
            .mount(&server)
            .await;

    let mut session = SessionData::new().with_token("test-token");
    session.data.insert("api_base".to_string(), server.uri());

    let client = SchwabClient::new(session)?;
    let rows = client
        .get_brokerage_transactions("81636739", "Individual", TransactionHistoryTimeFrame::All)
        .await?;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].action.as_deref(), Some("Buy"));
    assert_eq!(rows[1].action.as_deref(), Some("Cash In Lieu"));
    Ok(())
}

#[tokio::test]
async fn get_banking_transactions_paginates_with_page_number() -> Result<()> {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
            .and(path(
                "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/banking/non-pledged-asset-line/transactions",
            ))
            .and(body_partial_json(json!({
                "timeFrame": "Last6Months",
                "pageNumber": "0",
                "selectedAccountId": "440033623420",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "postedTransactions": [
                    {
                        "postingDate": "02/18/2026",
                        "description": "BALLAST-CZB-6708 WEB PMTS",
                        "type": "ACH",
                        "withdrawalAmount": "$1,850.00",
                        "depositAmount": "",
                        "runningBalance": "$9,923.96",
                        "checkSequenceNumber": "0"
                    }
                ],
                "pendingTransactions": [],
                "pagingInformation": {
                    "number": 0,
                    "moreRecords": true
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

    Mock::given(method("POST"))
            .and(path(
                "/api/is.TransactionHistoryWeb/TransactionHistoryInterface/TransactionHistory/banking/non-pledged-asset-line/transactions",
            ))
            .and(body_partial_json(json!({
                "pageNumber": "1",
                "selectedAccountId": "440033623420",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "postedTransactions": [
                    {
                        "postingDate": "02/14/2026",
                        "description": "PAYROLL",
                        "type": "DIRECT DEPOSIT",
                        "withdrawalAmount": "",
                        "depositAmount": "$4,000.00",
                        "runningBalance": "$11,773.96",
                        "checkSequenceNumber": "0"
                    }
                ],
                "pendingTransactions": [],
                "pagingInformation": {
                    "number": 1,
                    "moreRecords": false
                }
            })))
            .expect(1)
            .mount(&server)
            .await;

    let mut session = SessionData::new().with_token("test-token");
    session.data.insert("api_base".to_string(), server.uri());

    let client = SchwabClient::new(session)?;
    let rows = client
        .get_banking_transactions(
            "440033623420",
            "Checking",
            TransactionHistoryTimeFrame::Last6Months,
        )
        .await?;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].type_label.as_deref(), Some("ACH"));
    assert_eq!(rows[1].type_label.as_deref(), Some("DIRECT DEPOSIT"));
    Ok(())
}
