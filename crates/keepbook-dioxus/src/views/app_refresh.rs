use super::*;

/// The app-wide refresh control in the shell's title row.
///
/// The main action brings stale balances and then stale prices up to date;
/// the chevron holds the less common refreshes. Failures are reported through
/// `operation`, whose owner shows the `ErrorNotice`.
#[component]
pub(super) fn AppRefreshButton(operation: ActionFeedback, onrefresh: EventHandler<()>) -> Element {
    let refresh_stale = move || {
        operation.start("stale", "Refreshing…");
        spawn(async move {
            let (summary, failed) = refresh_stale_data().await;
            if failed {
                operation.fail("stale", "Some failed", summary);
            } else {
                operation.succeed("stale", "Refreshed", Some(summary));
            }
            onrefresh.call(());
        });
    };
    let refresh_all_prices = move || {
        operation.start("prices", "Refreshing…");
        let input = SyncPricesInput {
            scope: "all".to_string(),
            target: None,
            force: true,
            quote_staleness_seconds: None,
        };
        spawn(async move {
            match sync_prices(input).await {
                Ok(result) => {
                    let summary = price_sync_result_summary(&result);
                    if price_sync_result_has_failures(&result) {
                        operation.fail("prices", "Some prices failed", summary);
                    } else {
                        operation.succeed("prices", "Prices refreshed", Some(summary));
                    }
                    onrefresh.call(());
                }
                Err(error) => operation.fail(
                    "prices",
                    "Refresh failed",
                    format!("Price refresh failed: {error}"),
                ),
            }
        });
    };
    let resync_data = move || {
        operation.start("resync", "Resyncing…");
        spawn(async move {
            match reload_data().await {
                Ok(_) => {
                    operation.succeed("resync", "Data resynced", None);
                    onrefresh.call(());
                }
                Err(error) => {
                    operation.fail("resync", "Resync failed", format!("Resync failed: {error}"))
                }
            }
        });
    };
    let git_sync = move || {
        operation.start("git", "Syncing…");
        spawn(async move {
            let result = async {
                let settings = fetch_git_settings().await?;
                let input = GitSyncInput {
                    data_dir: normalize_git_data_dir_for_client(settings.data_dir),
                    host: settings.git.host,
                    repo: settings.git.repo,
                    branch: settings.git.branch,
                    ssh_user: settings.git.ssh_user,
                    private_key_pem: String::new(),
                    save_settings: false,
                };
                sync_git_repo_cancelable(input, new_git_sync_cancel_handle()).await
            }
            .await;
            match result {
                Ok(result) => {
                    operation.succeed(
                        "git",
                        "Git synced",
                        Some(format!("Synced {} {}.", result.remote_url, result.branch)),
                    );
                    onrefresh.call(());
                }
                Err(error) => {
                    operation.fail("git", "Sync failed", format!("Git sync failed: {error}"))
                }
            }
        });
    };

    rsx! {
        SplitButton {
            icon: ButtonIcon::Refresh,
            title: "Refresh stale balances and prices",
            menu_label: "More refresh options",
            actions: vec![
                MenuAction {
                    value: "all-prices",
                    label: "Refresh all prices",
                    detail: "Refetch every price, ignoring staleness",
                },
                MenuAction {
                    value: "resync",
                    label: "Resync data",
                    detail: "Reload keepbook data from disk",
                },
                MenuAction {
                    value: "git",
                    label: "Git sync",
                    detail: "Pull and push the data repository",
                },
            ],
            feedback: operation.current(),
            onclick: move |_| refresh_stale(),
            onselect: move |value| match value {
                "all-prices" => refresh_all_prices(),
                "resync" => resync_data(),
                _ => git_sync(),
            },
        }
    }
}

/// Syncs stale connections, then stale prices, even when the first step
/// fails. Returns the combined summary and whether anything failed.
async fn refresh_stale_data() -> (String, bool) {
    let balances = sync_connections(SyncConnectionsInput {
        target: None,
        if_stale: true,
        full_transactions: false,
    })
    .await;
    let prices = sync_prices(SyncPricesInput {
        scope: "all".to_string(),
        target: None,
        force: false,
        quote_staleness_seconds: None,
    })
    .await;

    let (balance_summary, balances_failed) = match balances {
        Ok(result) => (
            sync_result_summary(&result),
            balance_sync_result_has_failures(&result),
        ),
        Err(error) => (format!("Balance refresh failed: {error}"), true),
    };
    let (price_summary, prices_failed) = match prices {
        Ok(result) => (
            price_sync_result_summary(&result),
            price_sync_result_has_failures(&result),
        ),
        Err(error) => (format!("Price refresh failed: {error}"), true),
    };
    (
        format!("{balance_summary} {price_summary}"),
        balances_failed || prices_failed,
    )
}
