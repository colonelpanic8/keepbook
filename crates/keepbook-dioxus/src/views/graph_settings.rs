use super::*;
use crate::api::{
    fetch_application_settings, fetch_git_settings, new_git_sync_cancel_handle,
    save_application_settings, sync_git_repo_cancelable, GitSyncCancelHandle,
};
use dioxus::core::Task;

#[component]
pub(super) fn NetWorthGraphView(
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
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
pub(super) fn NetWorthBreakdownGraphView(
    currency: String,
    defaults: HistoryDefaults,
    filter_overrides: FilterOverrides,
) -> Element {
    rsx! {
        StackedHistoryGraphPanel {
            currency,
            defaults,
            filter_overrides,
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
    let toggle_filter_overrides = filter_overrides.clone();
    let reset_filter_overrides = filter_overrides.clone();

    rsx! {
        Panel {
            class: "settings-panel",
            title: "Portfolio",
            subtitle: source.to_string(),
            div { class: "settings-list",
                SettingRow {
                    title: "Latent capital gains tax",
                    description: format!("Include {} in net worth and history", latent_tax.account_name),
                    Switch {
                        label: "Latent capital gains tax",
                        checked: latent_tax.effective_enabled,
                        onchange: move |checked| {
                            let mut next = toggle_filter_overrides.clone();
                            next.include_latent_capital_gains_tax = Some(checked);
                            onfilterchange.call(next);
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
                ControlButton {
                    disabled: !override_active,
                    onclick: move |_| {
                        let mut next = reset_filter_overrides.clone();
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
fn ApplicationSettingsPanel() -> Element {
    let app_version = env!("CARGO_PKG_VERSION");
    let app_commit = short_commit(env!("GIT_COMMIT_HASH"));
    let mut settings = use_resource(fetch_application_settings);
    let mut start_minimized = use_signal(|| false);
    let mut window_decorations = use_signal(|| "auto".to_string());
    let mut loaded_value = use_signal(|| None::<(bool, String)>);
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);

    if let Some(Ok(current)) = settings.cloned() {
        let current_value = (
            current.start_minimized_to_tray,
            current.window_decorations.clone(),
        );
        if loaded_value() != Some(current_value.clone()) {
            start_minimized.set(current.start_minimized_to_tray);
            window_decorations.set(current.window_decorations);
            loaded_value.set(Some(current_value));
        }
    }

    let current_settings = settings.cloned();
    let is_busy = busy();
    let status_text = status();

    rsx! {
        Panel {
            class: "settings-panel",
            title: "Application",
            subtitle: "Build",
            div { class: "settings-list",
                ThemePicker {}
            }
            div { class: "settings-meta settings-meta-grid app-build-meta",
                span { "Version {app_version}" }
                span {
                    "Commit "
                    code { "{app_commit}" }
                }
            }
            match current_settings {
                None => rsx! { OperationStatus { message: "Loading application settings".to_string(), busy: true } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(current)) => rsx! {
                    div { class: "settings-list",
                        SettingRow {
                            title: "Start minimized to tray",
                            description: "Launch Keepbook in the background and open it from the tray icon",
                                Switch {
                                    label: "Start minimized to tray",
                                    checked: start_minimized(),
                                    disabled: is_busy,
                                    onchange: move |next: bool| {
                                        start_minimized.set(next);
                                        busy.set(true);
                                        status.set("Saving application settings...".to_string());
                                        spawn(async move {
                                            match save_application_settings(ApplicationSettingsInput {
                                                start_minimized_to_tray: next,
                                                window_decorations: window_decorations(),
                                            }).await {
                                                Ok(saved) => {
                                                    start_minimized.set(saved.start_minimized_to_tray);
                                                    window_decorations.set(saved.window_decorations.clone());
                                                    loaded_value.set(Some((
                                                        saved.start_minimized_to_tray,
                                                        saved.window_decorations,
                                                    )));
                                                    status.set("Saved. This takes effect the next time Keepbook starts.".to_string());
                                                    settings.restart();
                                                }
                                                Err(error) => {
                                                    start_minimized.set(!next);
                                                    status.set(error);
                                                }
                                            }
                                            busy.set(false);
                                        });
                                    }
                                }
                        }
                        SettingRow {
                            stacked: true,
                            title: "Window decorations",
                            description: "Auto hides the system title bar on Hyprland; choose System or Hidden to override it",
                            Select {
                                options: vec![
                                    SelectOption::new("auto", "Auto"),
                                    SelectOption::new("system", "System"),
                                    SelectOption::new("hidden", "Hidden"),
                                ],
                                value: window_decorations(),
                                disabled: is_busy,
                                onchange: move |next: String| {
                                    let previous = window_decorations();
                                    let current_start_minimized = start_minimized();
                                    window_decorations.set(next.clone());
                                    busy.set(true);
                                    status.set("Saving application settings...".to_string());
                                    spawn(async move {
                                        match save_application_settings(ApplicationSettingsInput {
                                            start_minimized_to_tray: current_start_minimized,
                                            window_decorations: next,
                                        }).await {
                                            Ok(saved) => {
                                                start_minimized.set(saved.start_minimized_to_tray);
                                                window_decorations.set(saved.window_decorations.clone());
                                                loaded_value.set(Some((
                                                    saved.start_minimized_to_tray,
                                                    saved.window_decorations,
                                                )));
                                                status.set("Saved. This takes effect the next time Keepbook starts.".to_string());
                                                settings.restart();
                                            }
                                            Err(error) => {
                                                window_decorations.set(previous);
                                                status.set(error);
                                            }
                                        }
                                        busy.set(false);
                                    });
                                },
                            }
                        }
                    }
                    if !status_text.is_empty() {
                        FloatingStatus { message: status_text, busy: is_busy }
                    }
                    div { class: "settings-source",
                        small { "{current.config_path}" }
                    }
                },
            }
        }
    }
}

#[component]
pub(super) fn SettingsView(
    repositories: Option<Result<RepositoryRegistry, String>>,
    repository_busy: bool,
    onrepositorychange: EventHandler<String>,
    filtering: FilteringSettings,
    filter_overrides: FilterOverrides,
    config_path: String,
    data_dir: String,
    onfilterchange: EventHandler<FilterOverrides>,
    onrefresh: EventHandler<()>,
) -> Element {
    let settings = use_resource(fetch_git_settings);
    let mut loaded_key = use_signal(String::new);
    let mut ssh_key_path = use_signal(|| None::<String>);
    let mut private_key = use_signal(String::new);
    let mut private_key_name = use_signal(String::new);
    let mut status = use_signal(String::new);
    let mut busy = use_signal(|| false);
    let mut cancel_requested = use_signal(|| false);
    let mut git_sync_cancel = use_signal(|| None::<GitSyncCancelHandle>);
    let mut git_sync_task = use_signal(|| None::<Task>);
    let mut add_location_open = use_signal(|| false);
    let mut location_name_input = use_signal(String::new);
    let mut location_remote_input = use_signal(String::new);
    let mut location_path_input = use_signal(String::new);
    let mut location_branch_input = use_signal(|| "master".to_string());
    let mut location_error = use_signal(String::new);
    let mut clone_dialog_open = use_signal(|| false);
    let mut clone_dialog_title = use_signal(String::new);
    let mut clone_dialog_message = use_signal(String::new);

    if let Some(Ok(current)) = settings.cloned() {
        let key = current.git.ssh_key_path.clone().unwrap_or_default();
        if loaded_key() != key {
            ssh_key_path.set(current.git.ssh_key_path);
            loaded_key.set(key);
        }
    }

    let current_settings = settings.cloned();
    let is_busy = busy();
    let is_canceling = cancel_requested();
    let status_text = status();

    rsx! {
        PortfolioSettingsPanel {
            filtering,
            filter_overrides,
            config_path,
            data_dir,
            onfilterchange,
        }
        ApplicationSettingsPanel {}
        Panel {
            class: "settings-panel",
            title: "Repositories",
            subtitle: "App-wide",
            actions: rsx! {
                button {
                    class: "control-button add-location-button",
                    title: "Add repository",
                    disabled: is_busy || repository_busy,
                    onclick: move |_| {
                        location_name_input.set(String::new());
                        location_remote_input.set(String::new());
                        location_path_input.set(String::new());
                        location_branch_input.set("master".to_string());
                        location_error.set(String::new());
                        add_location_open.set(true);
                    },
                    "+"
                }
            },
            match repositories.clone() {
                None => rsx! { OperationStatus { message: "Loading repositories".to_string(), busy: true } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(registry)) => rsx! {
                    div { class: "settings-meta",
                        span { "Manifest {registry.config_path}" }
                        if !registry.device_config_path.is_empty() {
                            span { "Device state {registry.device_config_path}" }
                        }
                    }
                    RepositoryList {
                        registry,
                        busy: is_busy || repository_busy,
                        clone_disabled: private_key().trim().is_empty()
                            && ssh_key_path().as_deref().unwrap_or_default().trim().is_empty(),
                        onactivate: move |id| onrepositorychange.call(id),
                        onremove: move |id: String| {
                            busy.set(true);
                            status.set("Removing repository from Keepbook...".to_string());
                            spawn(async move {
                                match remove_repository(id).await {
                                    Ok(_) => {
                                        status.set("Repository removed from Keepbook. Files were not deleted.".to_string());
                                        onrefresh.call(());
                                    }
                                    Err(error) => status.set(error),
                                }
                                busy.set(false);
                            });
                        },
                        onclone: move |repository: Repository| {
                            let (next_host, next_repo, next_ssh_user) = match git_settings_from_remote(&repository.remote) {
                                Ok(settings) => settings,
                                Err(error) => {
                                    status.set(error);
                                    return;
                                }
                            };
                            let action = if repository.cloned { "Git sync" } else { "Clone" };
                            let action_progress = if repository.cloned { "Syncing" } else { "Cloning" };
                            let input = GitSyncInput {
                                data_dir: repository.path.clone(),
                                host: next_host,
                                repo: next_repo,
                                branch: repository.branch.clone(),
                                ssh_user: next_ssh_user,
                                private_key_pem: private_key(),
                                // Persist a freshly provided key so it survives app restarts;
                                // mobile has no ~/.ssh fallback.
                                save_settings: !private_key().trim().is_empty(),
                            };
                            let cancel_handle = new_git_sync_cancel_handle();
                            busy.set(true);
                            cancel_requested.set(false);
                            git_sync_cancel.set(Some(cancel_handle.clone()));
                            clone_dialog_open.set(true);
                            clone_dialog_title.set(format!("{action} repository"));
                            clone_dialog_message.set(format!(
                                "{action_progress} {} at {}",
                                repository.remote, repository.path
                            ));
                            status.set(format!("{action_progress} repository..."));
                            let task = spawn(async move {
                                match sync_git_repo_cancelable(input, cancel_handle).await {
                                    Ok(result) => {
                                        clone_dialog_title.set("Repository ready".to_string());
                                        clone_dialog_message.set(format!(
                                            "Git synced {} from {} {}",
                                            result.data_dir, result.remote_url, result.branch
                                        ));
                                        status.set(format!("Repository {} is ready.", repository.name));
                                        onrefresh.call(());
                                    }
                                    Err(error) => {
                                        if error.contains("cancelled") || error.contains("canceled") {
                                            clone_dialog_title.set("Git operation canceled".to_string());
                                            clone_dialog_message.set("Git sync was canceled before it completed.".to_string());
                                            status.set("Git sync canceled.".to_string());
                                        } else {
                                            clone_dialog_title.set("Git operation failed".to_string());
                                            clone_dialog_message.set(error.clone());
                                            status.set(format!("Git sync failed: {error}"));
                                        }
                                    }
                                }
                                git_sync_task.set(None);
                                git_sync_cancel.set(None);
                                cancel_requested.set(false);
                                busy.set(false);
                            });
                            git_sync_task.set(Some(task));
                        },
                    }
                },
            }
        }
        Panel {
            class: "settings-panel",
            title: "Git Authentication",
            subtitle: "Device-local",
            match current_settings {
                None => rsx! { OperationStatus { message: "Loading Git authentication".to_string(), busy: true } },
                Some(Err(error)) => rsx! { p { class: "validation", "{error}" } },
                Some(Ok(current)) => rsx! {
                    div { class: "settings-meta",
                        span { "Config {current.config_path}" }
                    }
                    if !status_text.is_empty() {
                        FloatingStatus { message: status_text, busy: is_busy }
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
                                span { class: "control-button", "Select key file" }
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
                                ControlButton {
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
            Modal {
                title: "Add repository",
                header_actions: rsx! {
                    IconButton {
                        label: "Close",
                        glyph: "×",
                        disabled: is_busy,
                        onclick: move |_| add_location_open.set(false),
                    }
                },
                actions: rsx! {
                    ControlButton {
                        disabled: is_busy,
                        onclick: move |_| add_location_open.set(false),
                        "Cancel"
                    }
                    ControlButton {
                        primary: true,
                        disabled: is_busy,
                        onclick: move |_| {
                            match git_settings_from_remote(&location_remote_input()) {
                                    Ok(_) => {
                                        let next_data_dir = location_path_input();
                                        if next_data_dir.trim().is_empty() {
                                            location_error.set("Enter a local location.".to_string());
                                            return;
                                        }
                                        let next_branch = non_empty_client(&location_branch_input(), "master");
                                        let remote = location_remote_input().trim().to_string();
                                        let input = AddRepositoryInput {
                                            name: location_name_input(),
                                            path: next_data_dir,
                                            remote,
                                            branch: next_branch,
                                        };
                                        busy.set(true);
                                        status.set("Adding repository...".to_string());
                                        spawn(async move {
                                            match add_repository(input).await {
                                                Ok(_) => {
                                                    location_error.set(String::new());
                                                    add_location_open.set(false);
                                                    status.set("Repository added. Clone it when you are ready.".to_string());
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
                },
                label { class: "control-field",
                    span { "Name" }
                    input {
                        class: "control-input",
                        r#type: "text",
                        value: "{location_name_input()}",
                        placeholder: "Personal",
                        autofocus: true,
                        oninput: move |event| location_name_input.set(event.value())
                    }
                }
                label { class: "control-field",
                    span { "Remote" }
                    input {
                        class: "control-input",
                        r#type: "text",
                        value: "{location_remote_input()}",
                        placeholder: "git@github.com:owner/keepbook-data.git",
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
                        ControlButton {
                            disabled: is_busy,
                            onclick: move |_| location_path_input.set(path.clone()),
                            "Use app data folder"
                        }
                    }
                }
                if !location_error().is_empty() {
                    p { class: "validation", "{location_error()}" }
                }
            }
        }
        if clone_dialog_open() {
            Modal {
                wide: true,
                title: clone_dialog_title(),
                header_actions: rsx! {
                    if !is_busy {
                        IconButton {
                            label: "Close",
                            glyph: "×",
                            onclick: move |_| clone_dialog_open.set(false),
                        }
                    }
                },
                actions: rsx! {
                    if is_busy {
                        ControlButton {
                            danger: true,
                            disabled: is_canceling,
                            onclick: move |_| {
                                if let Some(cancel_handle) = git_sync_cancel() {
                                    cancel_handle.cancel();
                                }
                                cancel_requested.set(true);
                                clone_dialog_title.set("Canceling Git operation".to_string());
                                clone_dialog_message.set("Waiting for the current Git transfer step to stop.".to_string());
                                status.set("Canceling Git sync...".to_string());
                            },
                            if is_canceling {
                                "Canceling"
                            } else {
                                "Cancel"
                            }
                        }
                    } else {
                        ControlButton {
                            primary: true,
                            onclick: move |_| clone_dialog_open.set(false),
                            "Close"
                        }
                    }
                },
                Progress { label: clone_dialog_message(), busy: is_busy }
            }
        }
    }
}

#[component]
fn RepositoryList(
    registry: RepositoryRegistry,
    busy: bool,
    clone_disabled: bool,
    onactivate: EventHandler<String>,
    onclone: EventHandler<Repository>,
    onremove: EventHandler<String>,
) -> Element {
    rsx! {
        div { class: "git-locations",
            div { class: "git-locations-heading",
                strong { "Known repositories" }
                small { "Manifest repositories are read-only; repositories added here are device-local." }
            }
            for repository in registry.repositories {
                div { class: if repository.active { "git-location-row active" } else { "git-location-row" },
                    div { class: "git-location-main",
                        div { class: "git-location-title",
                            strong { "{repository.name}" }
                            small {
                                if repository.active { "Active" } else if repository.cloned { "Ready" } else { "Not cloned" }
                                if repository.managed { " · Managed" }
                                " · {repository.branch}"
                            }
                        }
                        div { class: "git-state-grid",
                            div { class: "git-state-row",
                                span { "Remote" }
                                code { "{repository.remote}" }
                            }
                            div { class: "git-state-row",
                                span { "Location" }
                                code { "{repository.path}" }
                            }
                            if let Some(commit) = repository.commit.as_deref() {
                                div { class: "git-state-row",
                                    span { "Commit" }
                                    code { "{short_commit(commit)}" }
                                }
                            }
                        }
                    }
                    div { class: "git-location-actions",
                        if !repository.active && repository.cloned {
                            ControlButton {
                                primary: true,
                                disabled: busy,
                                onclick: {
                                    let repository_id = repository.id.clone();
                                    move |_| onactivate.call(repository_id.clone())
                                },
                                "Use"
                            }
                        }
                        ControlButton {
                            disabled: busy || clone_disabled,
                            onclick: {
                                let repository = repository.clone();
                                move |_| onclone.call(repository.clone())
                            },
                            if repository.cloned { "Git sync" } else { "Clone" }
                        }
                        if repository_can_remove(&repository) {
                            ControlButton {
                                danger: true,
                                disabled: busy,
                                onclick: {
                                    let repository_id = repository.id.clone();
                                    move |_| onremove.call(repository_id.clone())
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
        }
    }
}
