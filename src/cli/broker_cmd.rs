use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::extract::State;
use axum::http::header::{AUTHORIZATION, CACHE_CONTROL, WWW_AUTHENTICATE};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde_json::json;
use tokio::sync::RwLock;

use crate::cli::output::{OutputFormat, OutputOptions};
use crate::core::broker::{
    authorize_bearer, build_response, parse_loopback_bind, parse_loopback_url, sanitize_failure,
    sanitize_success, validate_broker_token, BrokerProviderUsage, BrokerResponse,
};
use crate::core::config::AppConfig;
use crate::core::providers::Provider;

const BROKER_TOKEN_ENV: &str = "AIT_BROKER_TOKEN";
const BROKER_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone)]
struct CachedUsage {
    generated_at: DateTime<Utc>,
    providers: Vec<BrokerProviderUsage>,
}

#[derive(Clone)]
struct BrokerState {
    token: Arc<str>,
    cache: Arc<RwLock<CachedUsage>>,
    max_age_seconds: u64,
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    response
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn usage(State(state): State<BrokerState>, headers: HeaderMap) -> Response {
    let authorization = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    if !authorize_bearer(authorization, &state.token) {
        let mut response = (
            StatusCode::UNAUTHORIZED,
            Json(json!({ "error": "unauthorized" })),
        )
            .into_response();
        response
            .headers_mut()
            .insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer"));
        return no_store(response);
    }

    let cache = state.cache.read().await.clone();
    let response = build_response(
        cache.providers,
        cache.generated_at,
        Utc::now(),
        state.max_age_seconds,
    );
    no_store(Json(response).into_response())
}

fn broker_router(token: String, cache: Arc<RwLock<CachedUsage>>, max_age_seconds: u64) -> Router {
    let state = BrokerState {
        token: Arc::from(token),
        cache,
        max_age_seconds,
    };
    Router::new()
        .route("/healthz", get(health))
        .route("/v1/usage", get(usage))
        .with_state(state)
}

async fn collect_usage(
    providers: &[Provider],
    provider_timeout_seconds: u64,
) -> Vec<BrokerProviderUsage> {
    let handles: Vec<_> = providers
        .iter()
        .copied()
        .map(|provider| {
            tokio::spawn(async move {
                let observed_at = Utc::now();
                match tokio::time::timeout(
                    Duration::from_secs(provider_timeout_seconds),
                    crate::core::providers::fetch(provider),
                )
                .await
                {
                    Ok(Ok(result)) => sanitize_success(provider, result, observed_at),
                    Ok(Err(error)) => {
                        eprintln!("Broker refresh failed for {}: {:#}", provider.id(), error);
                        sanitize_failure(provider, observed_at)
                    }
                    Err(_) => {
                        eprintln!(
                            "Broker refresh timed out for {} after {} seconds",
                            provider.id(),
                            provider_timeout_seconds
                        );
                        sanitize_failure(provider, observed_at)
                    }
                }
            })
        })
        .collect();

    let mut usage = Vec::with_capacity(handles.len());
    for (provider, handle) in providers.iter().copied().zip(handles) {
        match handle.await {
            Ok(result) => usage.push(result),
            Err(error) => {
                eprintln!(
                    "Broker refresh task failed for {}: {}",
                    provider.id(),
                    error
                );
                usage.push(sanitize_failure(provider, Utc::now()));
            }
        }
    }
    usage
}

pub async fn serve(
    bind: String,
    refresh_seconds: u64,
    provider_timeout_seconds: u64,
    _opts: &OutputOptions,
) -> Result<()> {
    if refresh_seconds == 0 {
        anyhow::bail!("--refresh-seconds must be greater than zero");
    }
    if provider_timeout_seconds == 0 {
        anyhow::bail!("--provider-timeout-seconds must be greater than zero");
    }
    let address = parse_loopback_bind(&bind)?;

    crate::core::secrets::load_env_file(&AppConfig::secrets_path())?;
    let token = std::env::var(BROKER_TOKEN_ENV)
        .with_context(|| format!("{} is required to run the broker", BROKER_TOKEN_ENV))?;
    validate_broker_token(&token)?;

    let config = AppConfig::load().context("Failed to load broker configuration")?;
    let providers: Vec<_> = config
        .providers
        .iter()
        .filter(|provider| provider.enabled)
        .filter_map(|provider| Provider::from_id(&provider.id))
        .filter(|provider| !provider.is_stub())
        .collect();
    if providers.is_empty() {
        anyhow::bail!("No supported providers enabled; run `ait config edit <provider>` first");
    }

    let initial = collect_usage(&providers, provider_timeout_seconds).await;
    let cache = Arc::new(RwLock::new(CachedUsage {
        generated_at: Utc::now(),
        providers: initial,
    }));
    let app = broker_router(token, Arc::clone(&cache), refresh_seconds.saturating_mul(3));

    let refresh_cache = Arc::clone(&cache);
    let refresh_providers = providers;
    let refresh_task = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(refresh_seconds)).await;
            let refreshed = collect_usage(&refresh_providers, provider_timeout_seconds).await;
            *refresh_cache.write().await = CachedUsage {
                generated_at: Utc::now(),
                providers: refreshed,
            };
        }
    });

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("Failed to bind broker to {}", address))?;
    eprintln!(
        "AIT usage broker listening on http://{}",
        listener.local_addr()?
    );
    let result = axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await;
    refresh_task.abort();
    result.context("Broker server failed")
}

fn build_broker_client(builder: reqwest::ClientBuilder) -> Result<reqwest::Client> {
    builder
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(BROKER_REQUEST_TIMEOUT)
        .build()
        .context("Failed to initialize the broker client")
}

async fn fetch_broker_usage(
    client: &reqwest::Client,
    endpoint: reqwest::Url,
    token: &str,
) -> Result<BrokerResponse> {
    let response = client
        .get(endpoint)
        .bearer_auth(token)
        .send()
        .await
        .context("Failed to connect to the usage broker")?;
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        anyhow::bail!("Broker rejected AIT_BROKER_TOKEN");
    }
    if response.status().is_redirection() {
        anyhow::bail!("Broker returned a redirect; redirects are not allowed");
    }
    let response = response
        .error_for_status()
        .context("Broker returned an error")?;
    response
        .json()
        .await
        .context("Failed to decode the broker response")
}

pub async fn client_usage(
    url: String,
    provider: Option<String>,
    opts: &OutputOptions,
) -> Result<()> {
    let token = std::env::var(BROKER_TOKEN_ENV)
        .with_context(|| format!("{} is required to query the broker", BROKER_TOKEN_ENV))?;
    validate_broker_token(&token)?;
    let mut endpoint = parse_loopback_url(&url)?;
    endpoint.set_path("/v1/usage");

    let client = build_broker_client(reqwest::Client::builder())?;
    let mut payload = fetch_broker_usage(&client, endpoint, &token).await?;

    if let Some(provider_id) = provider {
        let provider = Provider::from_id(&provider_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: '{}'", provider_id))?;
        payload
            .providers
            .retain(|usage| usage.provider == provider.id());
        if payload.providers.is_empty() {
            anyhow::bail!(
                "Provider '{}' is not published by this broker",
                provider.id()
            );
        }
    }

    match opts.format {
        OutputFormat::Json => {
            if opts.pretty {
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!("{}", serde_json::to_string(&payload)?);
            }
        }
        OutputFormat::Text => render_text(&payload),
    }
    Ok(())
}

fn render_text(payload: &BrokerResponse) {
    let freshness = if payload.stale { "stale" } else { "fresh" };
    println!("Broker usage ({}; {}s old)", freshness, payload.age_seconds);
    for provider in &payload.providers {
        if !provider.available {
            println!("  {}: unavailable", provider.provider);
            continue;
        }
        println!("  {}:", provider.provider);
        for window in &provider.windows {
            println!(
                "    {}: {:.1}% remaining",
                window.name, window.remaining_percent
            );
        }
        if let Some(credits) = &provider.credits {
            if credits.unlimited {
                println!("    credits: unlimited");
            } else {
                println!("    credits: {:.2} remaining", credits.remaining);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::RwLock;
    use tower::ServiceExt;

    fn test_cache() -> Arc<RwLock<CachedUsage>> {
        Arc::new(RwLock::new(CachedUsage {
            generated_at: "2026-08-18T12:00:00Z".parse().unwrap(),
            providers: Vec::new(),
        }))
    }

    async fn test_server(app: Router) -> (reqwest::Url, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/v1/usage", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (endpoint, task)
    }

    #[tokio::test]
    async fn broker_client_bypasses_configured_proxies() {
        let token = "0123456789abcdef0123456789abcdef";
        let proxy_hits = Arc::new(AtomicUsize::new(0));
        let hits = Arc::clone(&proxy_hits);
        let proxy = Router::new().fallback(move || async move {
            hits.fetch_add(1, Ordering::SeqCst);
            StatusCode::IM_A_TEAPOT
        });
        let (proxy_endpoint, proxy_task) = test_server(proxy).await;
        let (endpoint, broker_task) =
            test_server(broker_router(token.to_string(), test_cache(), 120)).await;
        let client = build_broker_client(
            reqwest::Client::builder().proxy(reqwest::Proxy::all(proxy_endpoint.as_str()).unwrap()),
        )
        .unwrap();

        let payload = fetch_broker_usage(&client, endpoint, token).await.unwrap();

        assert!(payload.providers.is_empty());
        assert_eq!(proxy_hits.load(Ordering::SeqCst), 0);
        broker_task.abort();
        proxy_task.abort();
    }

    #[tokio::test]
    async fn broker_client_rejects_redirects_without_following_them() {
        let redirect_hits = Arc::new(AtomicUsize::new(0));
        let hits = Arc::clone(&redirect_hits);
        let app = Router::new()
            .route(
                "/v1/usage",
                get(|| async { (StatusCode::FOUND, [("location", "/redirected")], "redirect") }),
            )
            .route(
                "/redirected",
                get(move || async move {
                    hits.fetch_add(1, Ordering::SeqCst);
                    StatusCode::NO_CONTENT
                }),
            );
        let (endpoint, task) = test_server(app).await;
        let client = build_broker_client(reqwest::Client::builder()).unwrap();

        let error = fetch_broker_usage(&client, endpoint, "0123456789abcdef0123456789abcdef")
            .await
            .unwrap_err();

        assert_eq!(
            error.to_string(),
            "Broker returned a redirect; redirects are not allowed"
        );
        assert_eq!(redirect_hits.load(Ordering::SeqCst), 0);
        task.abort();
    }

    #[tokio::test]
    async fn usage_endpoint_requires_exact_bearer_token() {
        let token = "0123456789abcdef0123456789abcdef";
        let app = broker_router(token.to_string(), test_cache(), 120);

        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/v1/usage")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/v1/usage")
                    .header("authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(authorized.status(), StatusCode::OK);
        assert_eq!(authorized.headers()["cache-control"], "no-store");
        assert_eq!(authorized.headers()["x-content-type-options"], "nosniff");
    }

    #[tokio::test]
    async fn health_endpoint_contains_no_usage_data_and_needs_no_token() {
        let app = broker_router(
            "0123456789abcdef0123456789abcdef".to_string(),
            test_cache(),
            120,
        );
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body = axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap();

        assert_eq!(body.as_ref(), br#"{"status":"ok"}"#);
    }
}
