use std::ffi::CString;
use std::os::raw::c_char;
use std::path::Path;

/// Allocates a C string containing the keepbook-ffi version.
///
/// Call `keepbook_ffi_string_free` to free the returned pointer.
#[no_mangle]
pub extern "C" fn keepbook_ffi_version() -> *mut c_char {
    CString::new(env!("CARGO_PKG_VERSION"))
        .expect("version should be valid C string")
        .into_raw()
}

/// Frees a string allocated by this library.
#[no_mangle]
pub extern "C" fn keepbook_ffi_string_free(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(s));
    }
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
struct ConnectionSummary {
    id: String,
    name: String,
    synchronizer: String,
    status: String,
    created_at: String,
    last_sync_at: Option<String>,
    last_sync_status: Option<String>,
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
struct AccountSummary {
    id: String,
    name: String,
    connection_id: String,
    created_at: String,
    active: bool,
}

#[allow(dead_code)]
fn demo_connection_id() -> keepbook::models::Id {
    keepbook::models::Id::from_string("conn-demo")
}

#[allow(dead_code)]
fn demo_account_id() -> keepbook::models::Id {
    keepbook::models::Id::from_string("acct-demo")
}

#[allow(dead_code)]
fn ensure_demo_repo(data_dir: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(data_dir.join("connections"))?;
    std::fs::create_dir_all(data_dir.join("accounts"))?;

    let conn_id = demo_connection_id();
    let acct_id = demo_account_id();
    let now = chrono::Utc::now();

    let conn_config = keepbook::models::ConnectionConfig {
        name: "Demo Bank".to_string(),
        synchronizer: "demo".to_string(),
        credentials: None,
        balance_staleness: None,
    };

    let conn_state = keepbook::models::ConnectionState {
        id: conn_id.clone(),
        status: keepbook::models::ConnectionStatus::Active,
        created_at: now,
        last_sync: None,
        account_ids: vec![acct_id.clone()],
        synchronizer_data: serde_json::Value::Null,
    };

    let acct = keepbook::models::Account {
        id: acct_id.clone(),
        name: "Demo Checking".to_string(),
        connection_id: conn_id.clone(),
        tags: vec![],
        created_at: now,
        active: true,
        synchronizer_data: serde_json::Value::Null,
    };

    let conn_dir = data_dir.join("connections").join(conn_id.to_string());
    std::fs::create_dir_all(&conn_dir)?;
    std::fs::write(
        conn_dir.join("connection.toml"),
        toml::to_string_pretty(&conn_config)?,
    )?;
    std::fs::write(
        conn_dir.join("connection.json"),
        serde_json::to_string_pretty(&conn_state)?,
    )?;

    let acct_dir = data_dir.join("accounts").join(acct_id.to_string());
    std::fs::create_dir_all(&acct_dir)?;
    std::fs::write(
        acct_dir.join("account.json"),
        serde_json::to_string_pretty(&acct)?,
    )?;

    Ok(())
}

#[allow(dead_code)]
fn list_connections_json(data_dir: &Path) -> anyhow::Result<String> {
    use keepbook::storage::{JsonFileStorage, Storage};

    let storage = JsonFileStorage::new(data_dir);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let conns = rt.block_on(storage.list_connections())?;
    let out = conns
        .into_iter()
        .map(|c| ConnectionSummary {
            id: c.state.id.to_string(),
            name: c.config.name,
            synchronizer: c.config.synchronizer,
            status: c.state.status.as_str().to_string(),
            created_at: c.state.created_at.to_rfc3339(),
            last_sync_at: c.state.last_sync.as_ref().map(|s| s.at.to_rfc3339()),
            last_sync_status: c
                .state
                .last_sync
                .as_ref()
                .map(|s| format!("{:?}", s.status).to_lowercase()),
        })
        .collect::<Vec<_>>();

    Ok(serde_json::to_string(&out)?)
}

#[allow(dead_code)]
fn list_accounts_json(data_dir: &Path) -> anyhow::Result<String> {
    use keepbook::storage::{JsonFileStorage, Storage};

    let storage = JsonFileStorage::new(data_dir);
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let accts = rt.block_on(storage.list_accounts())?;
    let out = accts
        .into_iter()
        .map(|a| AccountSummary {
            id: a.id.to_string(),
            name: a.name,
            connection_id: a.connection_id.to_string(),
            created_at: a.created_at.to_rfc3339(),
            active: a.active,
        })
        .collect::<Vec<_>>();

    Ok(serde_json::to_string(&out)?)
}

fn normalize_repo_path(repo: &str) -> String {
    let repo = repo.trim();
    if repo.ends_with(".git") {
        repo.to_string()
    } else {
        format!("{repo}.git")
    }
}

#[allow(dead_code)]
fn build_ssh_remote_url(host: &str, repo: &str, ssh_user: &str) -> String {
    // If repo looks like a full URL already, just pass it through.
    if repo.contains("://") {
        return repo.to_string();
    }

    let repo = normalize_repo_path(repo);
    let host = host.trim();

    // If the host includes a scheme or a port, use an explicit ssh:// URL.
    if host.contains("://") || host.contains(':') {
        let host = host.strip_prefix("ssh://").unwrap_or(host);
        format!("ssh://{ssh_user}@{host}/{repo}")
    } else {
        // scp-like URL, e.g. git@github.com:owner/repo.git
        format!("{ssh_user}@{host}:{repo}")
    }
}

#[allow(dead_code)]
fn git_sync_ssh(
    repo_dir: &Path,
    host: &str,
    repo: &str,
    ssh_user: &str,
    private_key_pem: &str,
) -> anyhow::Result<()> {
    use git2::{Cred, FetchOptions, RemoteCallbacks, Repository, ResetType};

    std::fs::create_dir_all(repo_dir)?;

    let remote_url = build_ssh_remote_url(host, repo, ssh_user);

    fn make_fetch_options(ssh_user: &str, private_key_pem: &str) -> FetchOptions<'static> {
        let ssh_user = ssh_user.to_string();
        let private_key_pem = private_key_pem.to_string();

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(move |_url, _username_from_url, _allowed| {
            Cred::ssh_key_from_memory(&ssh_user, None, &private_key_pem, None)
        });

        // For now, accept the host key. We'll add strict host-key pinning later.
        callbacks.certificate_check(|_cert, _host| Ok(git2::CertificateCheckStatus::CertificateOk));

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);
        fetch_options
    }

    let repo = if repo_dir.join(".git").exists() {
        let repo = Repository::open(repo_dir)?;
        // Ensure "origin" points at the configured remote.
        match repo.find_remote("origin") {
            Ok(r) => {
                if r.url() != Some(remote_url.as_str()) {
                    repo.remote_set_url("origin", &remote_url)?;
                }
            }
            Err(_) => {
                repo.remote("origin", &remote_url)?;
            }
        }
        repo
    } else {
        let mut builder = git2::build::RepoBuilder::new();
        builder.fetch_options(make_fetch_options(ssh_user, private_key_pem));
        builder.clone(&remote_url, repo_dir)?
    };

    // Fetch updates (even after clone) so our "origin/*" refs are up to date.
    {
        let mut remote = repo.find_remote("origin")?;
        let mut fetch_options = make_fetch_options(ssh_user, private_key_pem);
        remote.fetch(&[] as &[&str], Some(&mut fetch_options), None)?;
    }

    // Reset the working tree to the remote default branch (read-only friendly).
    let branch = if let Ok(r) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Some(sym) = r.symbolic_target() {
            sym.rsplit('/').next().unwrap_or("main").to_string()
        } else {
            "main".to_string()
        }
    } else if repo.find_reference("refs/remotes/origin/main").is_ok() {
        "main".to_string()
    } else if repo.find_reference("refs/remotes/origin/master").is_ok() {
        "master".to_string()
    } else {
        anyhow::bail!("Could not determine default branch after fetch");
    };

    let remote_ref = format!("refs/remotes/origin/{branch}");
    let obj = repo.revparse_single(&remote_ref)?;
    let commit = obj.peel_to_commit()?;

    let local_ref = format!("refs/heads/{branch}");
    if repo.find_reference(&local_ref).is_err() {
        repo.branch(&branch, &commit, true)?;
    }

    repo.set_head(&local_ref)?;
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))?;
    repo.reset(commit.as_object(), ResetType::Hard, None)?;

    Ok(())
}

// Android entrypoint used by the Expo module's Kotlin code.
#[cfg(target_os = "android")]
mod android {
    use std::path::Path;

    use jni::objects::{JClass, JString};
    use jni::sys::jstring;
    use jni::JNIEnv;

    fn jstring_to_string(env: &mut JNIEnv, s: JString) -> anyhow::Result<String> {
        Ok(env.get_string(&s)?.into())
    }

    fn ok(env: &mut JNIEnv, s: impl AsRef<str>) -> jstring {
        env.new_string(s.as_ref())
            .expect("Couldn't create java string")
            .into_raw()
    }

    #[no_mangle]
    pub extern "system" fn Java_expo_modules_keepbooknative_KeepbookNativeRust_version(
        mut env: JNIEnv,
        _class: JClass,
    ) -> jstring {
        let s = env!("CARGO_PKG_VERSION");
        ok(&mut env, s)
    }

    #[no_mangle]
    pub extern "system" fn Java_expo_modules_keepbooknative_KeepbookNativeRust_initDemo(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let data_dir = match jstring_to_string(&mut env, data_dir) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };

        match super::ensure_demo_repo(Path::new(&data_dir)) {
            Ok(()) => ok(&mut env, ""),
            Err(e) => ok(&mut env, e.to_string()),
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_expo_modules_keepbooknative_KeepbookNativeRust_listConnections(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let data_dir = match jstring_to_string(&mut env, data_dir) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("{{\"error\":{:?}}}", e.to_string())),
        };

        match super::list_connections_json(Path::new(&data_dir)) {
            Ok(json) => ok(&mut env, json),
            Err(e) => ok(&mut env, format!("{{\"error\":{:?}}}", e.to_string())),
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_expo_modules_keepbooknative_KeepbookNativeRust_listAccounts(
        mut env: JNIEnv,
        _class: JClass,
        data_dir: JString,
    ) -> jstring {
        let data_dir = match jstring_to_string(&mut env, data_dir) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("{{\"error\":{:?}}}", e.to_string())),
        };

        match super::list_accounts_json(Path::new(&data_dir)) {
            Ok(json) => ok(&mut env, json),
            Err(e) => ok(&mut env, format!("{{\"error\":{:?}}}", e.to_string())),
        }
    }

    #[no_mangle]
    pub extern "system" fn Java_expo_modules_keepbooknative_KeepbookNativeRust_gitSync(
        mut env: JNIEnv,
        _class: JClass,
        repo_dir: JString,
        host: JString,
        repo: JString,
        ssh_user: JString,
        private_key_pem: JString,
        branch: JString,
        auth_token: JString,
    ) -> jstring {
        let repo_dir = match jstring_to_string(&mut env, repo_dir) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };
        let host = match jstring_to_string(&mut env, host) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };
        let repo = match jstring_to_string(&mut env, repo) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };
        let ssh_user = match jstring_to_string(&mut env, ssh_user) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };
        let private_key_pem = match jstring_to_string(&mut env, private_key_pem) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };
        // Android uses the SSH remote's default branch; branch is primarily for the web fallback.
        let _branch = match jstring_to_string(&mut env, branch) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };
        // Token is used by the web fallback (GitHub HTTP). Android uses SSH-based git sync.
        let _auth_token = match jstring_to_string(&mut env, auth_token) {
            Ok(s) => s,
            Err(e) => return ok(&mut env, format!("invalid args: {e:#}")),
        };

        if private_key_pem.trim().is_empty() {
            return ok(&mut env, "SSH private key is empty");
        }

        match super::git_sync_ssh(
            Path::new(&repo_dir),
            &host,
            &repo,
            &ssh_user,
            &private_key_pem,
        ) {
            Ok(()) => ok(&mut env, ""),
            Err(e) => ok(&mut env, e.to_string()),
        }
    }
}
