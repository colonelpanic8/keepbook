use super::*;
use crate::api::{fetch_git_settings, save_git_settings, sync_git_repo};

#[component]
pub(super) fn GraphsView(
    currency: String,
    defaults: HistoryDefaults,
    accounts: Vec<Account>,
    connections: Vec<Connection>,
    filter_overrides: FilterOverrides,
) -> Element {
    let _ = accounts;
    let _ = connections;

    rsx! {
        section { class: "panel graph-panel",
            HistoryGraphPanel {
                title: "Net Worth Over Time".to_string(),
                scope_label: currency.clone(),
                empty_title: "No net worth history".to_string(),
                empty_detail: "Refresh balances to populate the chart.".to_string(),
                currency: currency.clone(),
                defaults: defaults.clone(),
                filter_overrides,
                account: None,
                show_header: true,
            }
        }
    }
}

#[component]
fn PortfolioSettingsPanel(
    filtering: FilteringSettings,
    filter_overrides: FilterOverrides,
    config_path: String,
    data_dir: String,
    onfilterchange: EventHandler<FilterOverrides>,
) -> Element {
    let latent_tax = filtering.latent_capital_gains_tax;
    let override_active = filter_overrides.include_latent_capital_gains_tax.is_some();
    let source = if override_active {
        "Dioxus override"
    } else {
        "TOML default"
    };
    let configured_state = enabled_label(latent_tax.configured_enabled);
    let effective_state = enabled_label(latent_tax.effective_enabled);
    let rate_state = if latent_tax.rate_configured {
        "Configured"
    } else {
        "Missing"
    };

    rsx! {
        section { class: "panel settings-panel",
            div { class: "panel-header",
                h2 { "Portfolio" }
                span { "{source}" }
            }
            div { class: "settings-list",
                article { class: "setting-row",
                    div { class: "setting-copy",
                        strong { "Latent capital gains tax" }
                        small { "Include {latent_tax.account_name} in net worth and history" }
                    }
                    label { class: "switch-control",
                        input {
                            r#type: "checkbox",
                            checked: latent_tax.effective_enabled,
                            onchange: move |event| {
                                let mut next = filter_overrides;
                                next.include_latent_capital_gains_tax = Some(event.checked());
                                onfilterchange.call(next);
                            }
                        }
                        span { class: "switch-track",
                            span { class: "switch-thumb" }
                        }
                    }
                }
            }
            div { class: "settings-meta settings-meta-grid",
                span { "Default {configured_state}" }
                span { "Current {effective_state}" }
                span { "Tax rate {rate_state}" }
            }
            div { class: "settings-actions",
                button {
                    class: "control-button",
                    disabled: !override_active,
                    onclick: move |_| {
                        let mut next = filter_overrides;
                        next.include_latent_capital_gains_tax = None;
                        onfilterchange.call(next);
                    },
                    "Reset"
                }
            }
            div { class: "settings-source",
                small { "{config_path}" }
                small { "{data_dir}" }
            }
        }
    }
}

#[component]
pub(super) fn SettingsView(
    filtering: FilteringSettings,
    filter_overrides: FilterOverrides,
    config_path: String,
    data_dir: String,
    onfilterchange: EventHandler<FilterOverrides>,
    onrefresh: EventHandler<()>,
) -> Element {
    let mut settings = use_resource(fetch_git_settings);
    let mut loaded_key = use_signal(String::new);
    let mut git_data_dir = use_signal(String::new);
    let mut host = use_signal(|| "github.com".to_string());
    let mut repo = use_signal(|| "colonelpanic8/keepbook-data".to_string());
    let mut branch = use_signal(|| "master".to_string());
    let mut ssh_user = use_signal(|| "git".to_string());
    let mut ssh_key_path = use_signal(|| None::<String>);
    let mut private_key = use_signal(String::new);
    let mut private_key_name = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut add_location_open = use_signal(|| false);
    let mut location_remote_input = use_signal(String::new);
    let mut location_path_input = use_signal(String::new);
    let mut location_branch_input = use_signal(|| "master".to_string());
    let mut location_error = use_signal(String::new);
    let mut clone_dialog_open = use_signal(|| false);
    let mut clone_dialog_title = use_signal(String::new);
    let mut clone_dialog_message = use_signal(String::new);

    if let Some(Ok(current)) = settings.cloned() {
        let key = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            current.data_dir,
            current.git.host,
            current.git.repo,
            current.git.branch,
            current.git.ssh_user,
            current.git.ssh_key_path.as_deref().unwrap_or_default()
        );
        if loaded_key() != key {
            git_data_dir.set(normalize_git_data_dir_for_client(current.data_dir));
            host.set(current.git.host);
            repo.set(current.git.repo);
            branch.set(current.git.branch);
            ssh_user.set(current.git.ssh_user);
            ssh_key_path.set(current.git.ssh_key_path);
            loaded_key.set(key);
        }
    }

    let current_settings = settings.cloned();
    let is_busy = busy();
    let status_text = status();

    rsx! {
        PortfolioSettingsPanel {
            filtering,
            filter_overrides,
            config_path,
            data_dir,
            onfilterchange,
        }
        section { class: "panel settings-panel",
            div { class: "panel-header",
                div { class: "panel-title",
                    h2 { "Git Sync" }
                    span { "Repository only" }
                }
                div { class: "settings-actions inline-actions",
                    button {
                        class: "icon-button add-location-button",
                        title: "Add location",
                        disabled: is_busy,
                        onclick: move |_| {
                            location_remote_input.set(remote_input_from_settings(&host(), &repo(), &ssh_user()));
                            location_path_input.set(git_data_dir());
                            location_branch_input.set(branch());
                            location_error.set(String::new());
                            add_location_open.set(true);
                        },
                        "+"
                    }
                }
            }
            match current_settings {
                None => rsx! { BackendActivity { message: "Loading Git settings" } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(current)) => rsx! {
                    div { class: "settings-meta",
                        span { "Config {current.config_path}" }
                    }
                    if !status_text.is_empty() {
                        p { class: "settings-status", "{status_text}" }
                    }
                    GitLocationList {
                        current: current.clone(),
                        staged_data_dir: git_data_dir(),
                        staged_remote: remote_input_from_settings(&host(), &repo(), &ssh_user()),
                        staged_branch: branch(),
                        disabled: is_busy
                            || (private_key().trim().is_empty()
                                && ssh_key_path().as_deref().unwrap_or_default().trim().is_empty()),
                        onclone: move |_| {
                            let repo_cloned = current.repo_state.cloned;
                            let input = GitSyncInput {
                                data_dir: git_data_dir(),
                                host: host(),
                                repo: repo(),
                                branch: branch(),
                                ssh_user: ssh_user(),
                                private_key_pem: private_key(),
                                save_settings: true,
                            };
                            let action = if repo_cloned { "Git sync" } else { "Clone" };
                            let action_progress = if repo_cloned { "Syncing" } else { "Cloning" };
                            let key_source = if input.private_key_pem.trim().is_empty() {
                                "saved SSH key"
                            } else {
                                "selected SSH key"
                            };
                            busy.set(true);
                            clone_dialog_open.set(true);
                            clone_dialog_title.set(format!("{action} repository"));
                            clone_dialog_message.set(format!(
                                "{} {} at {} using {}",
                                action_progress,
                                remote_input_from_settings(&input.host, &input.repo, &input.ssh_user),
                                input.data_dir,
                                key_source
                            ));
                            status.set(format!("{action_progress} repository..."));
                            spawn(async move {
                                match sync_git_repo(input).await {
                                    Ok(result) => {
                                        clone_dialog_title.set("Repository ready".to_string());
                                        clone_dialog_message.set(format!(
                                            "Git synced {} from {} {}",
                                            result.data_dir, result.remote_url, result.branch
                                        ));
                                        status.set(format!("Git synced {} from {} {}", result.data_dir, result.remote_url, result.branch));
                                        settings.restart();
                                        onrefresh.call(());
                                    }
                                    Err(error) => {
                                        clone_dialog_title.set("Git operation failed".to_string());
                                        clone_dialog_message.set(error.clone());
                                        status.set(format!("Git sync failed: {error}"));
                                    }
                                }
                                busy.set(false);
                            });
                        },
                    }
                    div { class: "control-field secret-field",
                        span { "SSH private key" }
                        div { class: "key-file-picker",
                            label { class: "file-select-wrapper",
                                input {
                                    id: "ssh-private-key-file-input",
                                    class: "file-select-input",
                                    r#type: "file",
                                    disabled: is_busy,
                                }
                                span { class: "file-select-button", "Select key file" }
                            }
                            input {
                                id: "ssh-private-key-file-payload",
                                class: "file-payload-input",
                                r#type: "text",
                                oninput: move |event| {
                                    match serde_json::from_str::<serde_json::Value>(&event.value()) {
                                        Ok(payload) => {
                                            if let Some(message) = payload.get("status").and_then(|value| value.as_str()) {
                                                status.set(message.to_string());
                                                return;
                                            }
                                            if let Some(error) = payload.get("error").and_then(|value| value.as_str()) {
                                                status.set(error.to_string());
                                                return;
                                            }
                                            let name = payload
                                                .get("name")
                                                .and_then(|value| value.as_str())
                                                .unwrap_or("selected key")
                                                .to_string();
                                            let contents = payload
                                                .get("contents")
                                                .and_then(|value| value.as_str())
                                                .unwrap_or_default()
                                                .to_string();
                                            if contents.trim().is_empty() {
                                                status.set("Selected SSH key file is empty.".to_string());
                                            } else {
                                                private_key.set(contents);
                                                private_key_name.set(name.clone());
                                                status.set(format!("Selected SSH key file: {name}."));
                                            }
                                        }
                                        Err(error) => status.set(format!("Key file read failed: {error}")),
                                    }
                                }
                            }
                            small { class: "key-file-status",
                                if private_key().trim().is_empty() {
                                    if let Some(saved_key_path) = ssh_key_path() {
                                        "Saved key: {saved_key_path}"
                                    } else {
                                        "No private key selected"
                                    }
                                } else if private_key_name().is_empty() {
                                    "Private key loaded"
                                } else {
                                    "{private_key_name()} loaded"
                                }
                            }
                            if !private_key().trim().is_empty() {
                                button {
                                    class: "control-button",
                                    disabled: is_busy,
                                    onclick: move |_| {
                                        private_key.set(String::new());
                                        private_key_name.set(String::new());
                                        status.set("SSH key cleared.".to_string());
                                    },
                                    "Clear key"
                                }
                            }
                        }
                    }
                },
            }
        }
        if add_location_open() {
            div { class: "modal-backdrop",
                div { class: "modal-dialog",
                    div { class: "modal-header",
                        h3 { "Add location" }
                        button {
                            class: "icon-button",
                            disabled: is_busy,
                            onclick: move |_| add_location_open.set(false),
                            "x"
                        }
                    }
                    label { class: "control-field",
                        span { "Remote" }
                        input {
                            class: "control-input",
                            r#type: "text",
                            value: "{location_remote_input()}",
                            placeholder: "git@github.com:owner/keepbook-data.git",
                            autofocus: true,
                            oninput: move |event| {
                                location_remote_input.set(event.value());
                                location_error.set(String::new());
                            }
                        }
                    }
                    TextInput {
                        label: "Location",
                        value: location_path_input(),
                        placeholder: "/path/to/keepbook-data",
                        oninput: move |value| location_path_input.set(value)
                    }
                    TextInput {
                        label: "Branch",
                        value: location_branch_input(),
                        placeholder: "master",
                        oninput: move |value| location_branch_input.set(value)
                    }
                    if let Some(path) = recommended_data_dir() {
                        div { class: "settings-actions inline-actions",
                            button {
                                class: "control-button",
                                disabled: is_busy,
                                onclick: move |_| location_path_input.set(path.clone()),
                                "Use app data folder"
                            }
                        }
                    }
                    if !location_error().is_empty() {
                        p { class: "validation", "{location_error()}" }
                    }
                    div { class: "modal-actions",
                        button {
                            class: "control-button",
                            disabled: is_busy,
                            onclick: move |_| add_location_open.set(false),
                            "Cancel"
                        }
                        button {
                            class: "control-button selected",
                            disabled: is_busy,
                            onclick: move |_| {
                                match git_settings_from_remote(&location_remote_input()) {
                                    Ok((next_host, next_repo, next_ssh_user)) => {
                                        let next_data_dir = location_path_input();
                                        if next_data_dir.trim().is_empty() {
                                            location_error.set("Enter a local location.".to_string());
                                            return;
                                        }
                                        let next_branch = non_empty_client(&location_branch_input(), "master");
                                        let input = GitSettingsInput {
                                            data_dir: next_data_dir.clone(),
                                            host: next_host.clone(),
                                            repo: next_repo.clone(),
                                            branch: next_branch.clone(),
                                            ssh_user: next_ssh_user.clone(),
                                            ssh_key_path: ssh_key_path(),
                                        };
                                        busy.set(true);
                                        status.set("Saving Git location...".to_string());
                                        spawn(async move {
                                            match save_git_settings(input).await {
                                                Ok(saved) => {
                                                    git_data_dir.set(normalize_git_data_dir_for_client(saved.data_dir));
                                                    host.set(saved.git.host);
                                                    repo.set(saved.git.repo);
                                                    branch.set(saved.git.branch);
                                                    ssh_user.set(saved.git.ssh_user);
                                                    ssh_key_path.set(saved.git.ssh_key_path);
                                                    location_error.set(String::new());
                                                    add_location_open.set(false);
                                                    status.set("Git location added.".to_string());
                                                    settings.restart();
                                                    onrefresh.call(());
                                                }
                                                Err(error) => {
                                                    location_error.set(error.clone());
                                                    status.set(format!("Save failed: {error}"));
                                                }
                                            }
                                            busy.set(false);
                                        });
                                    }
                                    Err(error) => location_error.set(error),
                                }
                            },
                            "Add"
                        }
                    }
                }
            }
        }
        if clone_dialog_open() {
            div { class: "modal-backdrop",
                div { class: "modal-dialog clone-dialog",
                    div { class: "modal-header",
                        h3 { "{clone_dialog_title()}" }
                        if !is_busy {
                            button {
                                class: "icon-button",
                                onclick: move |_| clone_dialog_open.set(false),
                                "x"
                            }
                        }
                    }
                    div { class: "clone-progress",
                        if is_busy {
                            span { class: "activity-spinner large" }
                        }
                        p { "{clone_dialog_message()}" }
                    }
                    if !is_busy {
                        div { class: "modal-actions",
                            button {
                                class: "control-button selected",
                                onclick: move |_| clone_dialog_open.set(false),
                                "Close"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn GitLocationList(
    current: GitSettingsOutput,
    staged_data_dir: String,
    staged_remote: String,
    staged_branch: String,
    disabled: bool,
    onclone: EventHandler<()>,
) -> Element {
    let state = current.repo_state;
    let remote_label = state.remote_url.clone().unwrap_or(staged_remote);
    let branch_label = state.branch.clone().unwrap_or(staged_branch);
    let commit_label = state
        .commit
        .as_deref()
        .map(short_commit)
        .unwrap_or_else(|| "Not cloned".to_string());
    let status_label = if state.cloned { "Cloned" } else { "Not cloned" };
    let action_label = if state.cloned { "Git sync" } else { "Clone" };

    rsx! {
        div { class: "git-locations",
            div { class: "git-locations-heading",
                strong { "Known locations" }
            }
            div { class: "git-location-row",
                div { class: "git-location-main",
                    div { class: "git-location-title",
                        strong { "{status_label}" }
                        small { "{branch_label}" }
                    }
                    div { class: "git-state-grid",
                        div { class: "git-state-row",
                            span { "Remote" }
                            code { "{remote_label}" }
                        }
                        div { class: "git-state-row",
                            span { "Commit" }
                            code { "{commit_label}" }
                        }
                        div { class: "git-state-row",
                            span { "Location" }
                            code { "{staged_data_dir}" }
                        }
                    }
                }
                div { class: "git-location-actions",
                    button {
                        class: "control-button selected",
                        disabled,
                        onclick: move |_| onclone.call(()),
                        "{action_label}"
                    }
                }
            }
        }
    }
}
