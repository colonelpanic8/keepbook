pub(crate) fn git_settings_from_remote(remote: &str) -> Result<(String, String, String), String> {
    let trimmed = remote.trim();
    if trimmed.is_empty() {
        return Err("Enter a remote.".to_string());
    }

    if is_explicit_git_remote(trimmed) {
        return Ok((
            remote_host(trimmed).unwrap_or_else(|| "github.com".to_string()),
            trimmed.to_string(),
            remote_user(trimmed).unwrap_or_else(|| "git".to_string()),
        ));
    }

    normalize_github_repo_input(trimmed)
        .map(|repo| ("github.com".to_string(), repo, "git".to_string()))
}

pub(crate) fn non_empty_client(value: &str, default: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        default.to_string()
    } else {
        trimmed.to_string()
    }
}

pub(crate) fn is_explicit_git_remote(remote: &str) -> bool {
    remote.contains("://") || (remote.contains('@') && remote.contains(':'))
}

pub(crate) fn remote_user(remote: &str) -> Option<String> {
    remote
        .split('@')
        .next()
        .and_then(|prefix| prefix.rsplit(['/', ':']).next())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn remote_host(remote: &str) -> Option<String> {
    let without_scheme = remote.split("://").nth(1).unwrap_or(remote);
    let after_user = without_scheme.split('@').nth(1).unwrap_or(without_scheme);
    after_user
        .split([':', '/'])
        .next()
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

pub(crate) fn normalize_github_repo_input(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a repository as owner/repo.".to_string());
    }

    let repo = trim_github_repo_prefix(trimmed)
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or_else(|| trim_github_repo_prefix(trimmed).trim_matches('/'));

    let mut parts = repo.split('/');
    let Some(owner) = parts.next() else {
        return Err("Enter a repository as owner/repo.".to_string());
    };
    let Some(name) = parts.next() else {
        return Err("Enter a repository as owner/repo.".to_string());
    };
    if owner.is_empty() || name.is_empty() || parts.next().is_some() {
        return Err("Enter a repository as owner/repo.".to_string());
    }

    Ok(format!("{owner}/{name}"))
}

pub(crate) fn trim_github_repo_prefix(input: &str) -> &str {
    input
        .strip_prefix("https://github.com/")
        .or_else(|| input.strip_prefix("http://github.com/"))
        .or_else(|| input.strip_prefix("git@github.com:"))
        .unwrap_or(input)
}

pub(crate) fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}
