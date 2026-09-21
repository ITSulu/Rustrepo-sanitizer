//! Server-side Forgejo and GitHub integration.
//!
//! Tokens are read from the server environment and are only ever presented to
//! the upstream API. They are never serialized into a response the browser can
//! read, and the browser never receives a credentialed clone URL.

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum IntegrationsError {
    #[error("{0} integration is not configured on the server")]
    NotConfigured(&'static str),
    #[error("invalid repository owner or name")]
    InvalidRepository,
    #[error("upstream request failed: {0}")]
    Upstream(String),
}

#[derive(Clone)]
pub struct IntegrationsConfig {
    pub forgejo_base: Option<Url>,
    pub forgejo_token: Option<String>,
    pub github_api: Url,
    pub github_token: Option<String>,
}

impl Default for IntegrationsConfig {
    fn default() -> Self {
        Self {
            forgejo_base: None,
            forgejo_token: None,
            github_api: Url::parse("https://api.github.com").expect("valid default"),
            github_token: None,
        }
    }
}

impl IntegrationsConfig {
    /// Reads configuration from the process environment.
    pub fn from_env() -> Self {
        let forgejo_base = std::env::var("RUSTREPO_WEB_FORGEJO_BASE")
            .ok()
            .and_then(|value| Url::parse(&value).ok());
        let github_api = std::env::var("RUSTREPO_WEB_GITHUB_API")
            .ok()
            .and_then(|value| Url::parse(&value).ok())
            .unwrap_or_else(|| Url::parse("https://api.github.com").expect("valid default"));
        Self {
            forgejo_base,
            forgejo_token: std::env::var("RUSTREPO_WEB_FORGEJO_TOKEN").ok(),
            github_api,
            github_token: std::env::var("RUSTREPO_WEB_GITHUB_TOKEN").ok(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RepoSummary {
    pub full_name: String,
    pub clone_url: String,
    #[serde(default)]
    pub default_branch: Option<String>,
    #[serde(default)]
    pub private: bool,
}

#[derive(Debug, Deserialize)]
struct ForgejoRepo {
    full_name: String,
    clone_url: String,
    #[serde(default)]
    default_branch: Option<String>,
    #[serde(default)]
    private: bool,
}

pub struct Integrations {
    config: IntegrationsConfig,
    client: reqwest::Client,
}

impl Integrations {
    pub fn new(config: IntegrationsConfig) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("rustrepo-sanitizer-web")
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("building HTTP client");
        Self { config, client }
    }

    pub fn forgejo_configured(&self) -> bool {
        self.config.forgejo_base.is_some() && self.config.forgejo_token.is_some()
    }

    pub fn github_configured(&self) -> bool {
        self.config.github_token.is_some()
    }

    /// Server-side token for authenticated clones. Never serialized.
    pub fn forgejo_token(&self) -> Option<&str> {
        self.config.forgejo_token.as_deref()
    }

    pub fn github_token(&self) -> Option<&str> {
        self.config.github_token.as_deref()
    }

    pub fn forgejo_clone_url(&self, owner: &str, repo: &str) -> Result<Url, IntegrationsError> {
        validate_component(owner)?;
        validate_component(repo)?;
        let base = self
            .config
            .forgejo_base
            .as_ref()
            .ok_or(IntegrationsError::NotConfigured("Forgejo"))?;
        let base = base.as_str().trim_end_matches('/');
        Url::parse(&format!("{base}/{owner}/{repo}.git"))
            .map_err(|_| IntegrationsError::InvalidRepository)
    }

    pub fn github_clone_url(owner: &str, repo: &str) -> Url {
        Url::parse(&format!("https://github.com/{owner}/{repo}.git"))
            .expect("owner/repo validated by callers")
    }

    pub async fn list_forgejo_repos(&self) -> Result<Vec<RepoSummary>, IntegrationsError> {
        let token = self
            .config
            .forgejo_token
            .as_ref()
            .ok_or(IntegrationsError::NotConfigured("Forgejo"))?;
        let base = self
            .config
            .forgejo_base
            .as_ref()
            .ok_or(IntegrationsError::NotConfigured("Forgejo"))?;
        let url = format!(
            "{}/api/v1/user/repos?limit=50",
            base.as_str().trim_end_matches('/')
        );
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("token {token}"))
            .send()
            .await
            .map_err(|err| IntegrationsError::Upstream(err.to_string()))?;
        if !response.status().is_success() {
            return Err(IntegrationsError::Upstream(format!(
                "Forgejo returned {}",
                response.status()
            )));
        }
        let repos: Vec<ForgejoRepo> = response
            .json()
            .await
            .map_err(|err| IntegrationsError::Upstream(err.to_string()))?;
        Ok(summarize(repos.into_iter().map(|repo| {
            (
                repo.full_name,
                repo.clone_url,
                repo.default_branch,
                repo.private,
            )
        })))
    }

    pub async fn list_github_repos(&self) -> Result<Vec<RepoSummary>, IntegrationsError> {
        let token = self
            .config
            .github_token
            .as_ref()
            .ok_or(IntegrationsError::NotConfigured("GitHub"))?;
        let url = format!(
            "{}/user/repos?per_page=50&sort=updated",
            self.config.github_api.as_str().trim_end_matches('/')
        );
        let response = self
            .client
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|err| IntegrationsError::Upstream(err.to_string()))?;
        if !response.status().is_success() {
            return Err(IntegrationsError::Upstream(format!(
                "GitHub returned {}",
                response.status()
            )));
        }
        let repos: Vec<ForgejoRepo> = response
            .json()
            .await
            .map_err(|err| IntegrationsError::Upstream(err.to_string()))?;
        Ok(summarize(repos.into_iter().map(|repo| {
            (
                repo.full_name,
                repo.clone_url,
                repo.default_branch,
                repo.private,
            )
        })))
    }
}

fn summarize(
    repos: impl Iterator<Item = (String, String, Option<String>, bool)>,
) -> Vec<RepoSummary> {
    let mut summaries: Vec<RepoSummary> = repos
        .map(
            |(full_name, clone_url, default_branch, private)| RepoSummary {
                full_name,
                clone_url,
                default_branch,
                private,
            },
        )
        .collect();
    summaries.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    summaries
}

/// Owner/repo components must be safe single path segments.
pub fn validate_component(value: &str) -> Result<(), IntegrationsError> {
    if value.is_empty()
        || value.len() > 128
        || value == "."
        || value == ".."
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(IntegrationsError::InvalidRepository);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::get, Router};

    #[test]
    fn clone_urls_are_constructed_and_validated() {
        let config = IntegrationsConfig {
            forgejo_base: Some(Url::parse("https://git.example.com").unwrap()),
            forgejo_token: Some("secret".into()),
            ..IntegrationsConfig::default()
        };
        let integrations = Integrations::new(config);
        assert_eq!(
            integrations
                .forgejo_clone_url("org", "repo")
                .unwrap()
                .as_str(),
            "https://git.example.com/org/repo.git"
        );
        assert!(matches!(
            integrations.forgejo_clone_url("../etc", "repo"),
            Err(IntegrationsError::InvalidRepository)
        ));
        assert!(matches!(
            integrations.forgejo_clone_url("org", "re po"),
            Err(IntegrationsError::InvalidRepository)
        ));
        assert_eq!(
            Integrations::github_clone_url("org", "repo").as_str(),
            "https://github.com/org/repo.git"
        );
    }

    #[test]
    fn unconfigured_integrations_report_not_configured() {
        let integrations = Integrations::new(IntegrationsConfig::default());
        assert!(!integrations.forgejo_configured());
        assert!(!integrations.github_configured());
        assert!(matches!(
            integrations.forgejo_clone_url("org", "repo"),
            Err(IntegrationsError::NotConfigured("Forgejo"))
        ));
    }

    async fn mock_server(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn lists_forgejo_repositories_with_the_server_token() {
        let seen_auth = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen = seen_auth.clone();
        let router = Router::new().route(
            "/api/v1/user/repos",
            get(move |headers: axum::http::HeaderMap| {
                let seen = seen.clone();
                async move {
                    *seen.lock().unwrap() = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned);
                    axum::Json(serde_json::json!([
                        {"full_name":"itsulu/repo","clone_url":"https://git.example.com/itsulu/repo.git","default_branch":"main","private":true}
                    ]))
                }
            }),
        );
        let base = mock_server(router).await;
        let integrations = Integrations::new(IntegrationsConfig {
            forgejo_base: Some(Url::parse(&base).unwrap()),
            forgejo_token: Some("tok".into()),
            ..IntegrationsConfig::default()
        });
        let repos = integrations.list_forgejo_repos().await.unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].full_name, "itsulu/repo");
        assert_eq!(seen_auth.lock().unwrap().as_deref(), Some("token tok"));
    }

    #[tokio::test]
    async fn lists_github_repositories_and_uses_bearer_auth() {
        let seen_auth = std::sync::Arc::new(std::sync::Mutex::new(None));
        let seen = seen_auth.clone();
        let router = Router::new().route(
            "/user/repos",
            get(move |headers: axum::http::HeaderMap| {
                let seen = seen.clone();
                async move {
                    *seen.lock().unwrap() = headers
                        .get("authorization")
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_owned);
                    axum::Json(serde_json::json!([
                        {"full_name":"ITSulu/Repo","clone_url":"https://github.com/ITSulu/Repo.git","default_branch":"main","private":false}
                    ]))
                }
            }),
        );
        let base = mock_server(router).await;
        let integrations = Integrations::new(IntegrationsConfig {
            github_api: Url::parse(&base).unwrap(),
            github_token: Some("gh".into()),
            ..IntegrationsConfig::default()
        });
        let repos = integrations.list_github_repos().await.unwrap();
        assert_eq!(repos[0].full_name, "ITSulu/Repo");
        assert_eq!(seen_auth.lock().unwrap().as_deref(), Some("Bearer gh"));
    }

    #[tokio::test]
    async fn upstream_errors_are_surfaced_without_leaking_tokens() {
        let router = Router::new().route(
            "/api/v1/user/repos",
            get(|| async { (axum::http::StatusCode::UNAUTHORIZED, "nope") }),
        );
        let base = mock_server(router).await;
        let integrations = Integrations::new(IntegrationsConfig {
            forgejo_base: Some(Url::parse(&base).unwrap()),
            forgejo_token: Some("super-secret".into()),
            ..IntegrationsConfig::default()
        });
        let err = integrations.list_forgejo_repos().await.unwrap_err();
        assert!(!err.to_string().contains("super-secret"));
    }
}
