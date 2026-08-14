use std::{path::Path, time::Duration};

use dioxus::{
    desktop::{Config, DesktopContext, LogicalSize, WindowBuilder},
    prelude::*,
};
use hermes_core::{AppServices, CloudGatewayCookie};
use hermes_protocol::{ConnectionConfigInput, ConnectionMode, RemoteAuthMode};
use hermes_ui::{CloudActions, CloudAgent, CloudConnectRequest, CloudOrg, CloudState};
use serde_json::Value;
use url::Url;

use super::DesktopDataDir;

const DEFAULT_PORTAL_BASE_URL: &str = "https://portal.nousresearch.com";
const MAX_CLOUD_AGENTS: usize = 256;
const MAX_CLOUD_ORGS: usize = 64;
const MAX_FIELD_CHARS: usize = 512;
const LOGIN_POLL_ATTEMPTS: usize = 240;
const AGENT_LOGIN_POLL_ATTEMPTS: usize = 160;

const PORTAL_COOKIE_NAMES: &[&str] = &[
    "__Host-privy-token",
    "__Secure-privy-token",
    "privy-token",
    "privy-session",
];
const GATEWAY_COOKIE_NAMES: &[&str] = &[
    "__Host-hermes_session_at",
    "__Secure-hermes_session_at",
    "hermes_session_at",
    "__Host-hermes_session_rt",
    "__Secure-hermes_session_rt",
    "hermes_session_rt",
];

fn cloud_window_shell() -> Element {
    rsx! {
        div {
            style: "height:100vh;display:grid;place-items:center;background:#090b10;color:#e5e7eb;font:13px system-ui,sans-serif;",
            "Opening Hermes Cloud…"
        }
    }
}

fn bounded(value: &str) -> String {
    value.chars().take(MAX_FIELD_CHARS).collect()
}

fn portal_base_url() -> Result<String, String> {
    let raw = std::env::var("HERMES_PORTAL_BASE_URL")
        .ok()
        .or_else(|| std::env::var("NOUS_PORTAL_BASE_URL").ok())
        .unwrap_or_else(|| DEFAULT_PORTAL_BASE_URL.to_owned());
    normalize_http_url(&raw)
}

fn normalize_http_url(raw: &str) -> Result<String, String> {
    let mut parsed = Url::parse(raw.trim()).map_err(|error| format!("invalid Cloud URL: {error}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Cloud URLs must use http:// or https://".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("credentialed Cloud URLs are not allowed".into());
    }
    parsed.set_fragment(None);
    parsed.set_query(None);
    let normalized = parsed.as_str().trim_end_matches('/').to_owned();
    if normalized.len() > 2_048 {
        return Err("Cloud URL is too long".into());
    }
    Ok(normalized)
}

fn cloud_config(data_dir: &Path, visible: bool, title: &str) -> Config {
    Config::new()
        .with_window(
            WindowBuilder::new()
                .with_title(title)
                .with_inner_size(LogicalSize::new(520.0, 720.0))
                .with_visible(visible),
        )
        .with_menu(None)
        .with_data_directory(data_dir.join("cloud-webview"))
        .with_navigation_handler(|target| {
            Url::parse(target)
                .ok()
                .is_some_and(|url| matches!(url.scheme(), "http" | "https"))
        })
}

async fn open_cloud_window(
    desktop: &DesktopContext,
    data_dir: &Path,
    visible: bool,
    title: &str,
) -> DesktopContext {
    desktop
        .new_window(
            VirtualDom::new(cloud_window_shell),
            cloud_config(data_dir, visible, title),
        )
        .await
}

fn has_portal_session(window: &DesktopContext, portal: &str) -> Result<bool, String> {
    let cookies = window
        .webview
        .cookies_for_url(portal)
        .map_err(|error| error.to_string())?;
    Ok(cookies
        .iter()
        .any(|cookie| PORTAL_COOKIE_NAMES.contains(&cookie.name())))
}

fn gateway_session_cookies(
    window: &DesktopContext,
    base_url: &str,
) -> Result<Vec<CloudGatewayCookie>, String> {
    let cookies = window
        .webview
        .cookies_for_url(base_url)
        .map_err(|error| error.to_string())?;
    let mut selected = Vec::new();
    for cookie in cookies {
        if GATEWAY_COOKIE_NAMES.contains(&cookie.name()) {
            selected.push(CloudGatewayCookie {
                name: cookie.name().to_owned(),
                value: cookie.value().to_owned(),
            });
        }
    }
    Ok(selected)
}

fn clear_gateway_session(window: &DesktopContext, base_url: &str) -> Result<(), String> {
    let cookies = window
        .webview
        .cookies_for_url(base_url)
        .map_err(|error| error.to_string())?;
    for cookie in cookies {
        if GATEWAY_COOKIE_NAMES.contains(&cookie.name()) {
            window
                .webview
                .delete_cookie(&cookie)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn clear_portal_session(window: &DesktopContext, portal: &str) -> Result<(), String> {
    for cookie in window
        .webview
        .cookies_for_url(portal)
        .map_err(|error| error.to_string())?
    {
        window
            .webview
            .delete_cookie(&cookie)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn decode_script_result(raw: &str) -> Result<Value, String> {
    let first: Value = serde_json::from_str(raw)
        .map_err(|error| format!("Cloud browser returned invalid JSON: {error}"))?;
    if let Value::String(inner) = first {
        serde_json::from_str(&inner)
            .map_err(|error| format!("Cloud browser returned invalid nested JSON: {error}"))
    } else {
        Ok(first)
    }
}

async fn browser_fetch_json(window: &DesktopContext, target: &str) -> Result<(u16, Value), String> {
    let target = serde_json::to_string(target).map_err(|error| error.to_string())?;
    let script = format!(
        r#"(async () => {{
            try {{
                const response = await fetch({target}, {{ credentials: 'include', headers: {{ 'Accept': 'application/json' }} }});
                const body = await response.text();
                return {{ status: response.status, body }};
            }} catch (error) {{
                return {{ status: 0, body: '', error: String(error) }};
            }}
        }})()"#
    );
    let (sender, receiver) = tokio::sync::oneshot::channel::<String>();
    window
        .webview
        .evaluate_script_with_callback(&script, move |result| {
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;
    let raw = tokio::time::timeout(Duration::from_secs(20), receiver)
        .await
        .map_err(|_| "Hermes Cloud discovery timed out".to_owned())?
        .map_err(|_| "Hermes Cloud browser closed during discovery".to_owned())?;
    let result = decode_script_result(&raw)?;
    let status = result
        .get("status")
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .unwrap_or(0);
    if status == 0 {
        let error = result
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown browser fetch failure");
        return Err(format!("Hermes Cloud discovery failed: {}", bounded(error)));
    }
    let body = result
        .get("body")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let decoded = if body.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(body)
            .map_err(|error| format!("Hermes Cloud returned invalid JSON: {error}"))?
    };
    Ok((status, decoded))
}

fn trim_org(value: &Value) -> Option<CloudOrg> {
    let id = value.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    Some(CloudOrg {
        id: bounded(id),
        slug: value
            .get("slug")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(bounded),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(bounded)
            .unwrap_or_else(|| bounded(id)),
        is_personal: value
            .get("isPersonal")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        role: value
            .get("role")
            .and_then(Value::as_str)
            .map(bounded)
            .unwrap_or_else(|| "MEMBER".into()),
    })
}

fn trim_agent(value: &Value) -> Option<CloudAgent> {
    let id = value.get("id")?.as_str()?.trim();
    if id.is_empty() {
        return None;
    }
    let dashboard_url = value
        .get("dashboardUrl")
        .and_then(Value::as_str)
        .and_then(|value| normalize_http_url(value).ok());
    Some(CloudAgent {
        id: bounded(id),
        name: value
            .get("name")
            .and_then(Value::as_str)
            .map(bounded)
            .unwrap_or_else(|| bounded(id)),
        status: value
            .get("status")
            .and_then(Value::as_str)
            .map(bounded)
            .unwrap_or_else(|| "unknown".into()),
        dashboard_url,
        dashboard_gateway_state: value
            .get("dashboardGatewayState")
            .and_then(Value::as_str)
            .map(bounded)
            .unwrap_or_else(|| "unknown".into()),
    })
}

fn parse_discovery(
    status: u16,
    body: &Value,
) -> Result<(Vec<CloudOrg>, Vec<CloudAgent>, Option<String>), String> {
    match status {
        200 => {
            let agents = body
                .get("agents")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(trim_agent)
                .take(MAX_CLOUD_AGENTS)
                .collect::<Vec<_>>();
            let org = body.get("org").and_then(trim_org);
            let selected = org.as_ref().map(CloudOrg::selection_key);
            Ok((org.into_iter().collect(), agents, selected))
        }
        401 => Err("Your Hermes Cloud portal session has expired. Sign in again.".into()),
        409 if body.get("error").and_then(Value::as_str) == Some("org_selection_required") => {
            let orgs = body
                .get("orgs")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(trim_org)
                .take(MAX_CLOUD_ORGS)
                .collect::<Vec<_>>();
            if orgs.is_empty() {
                Err("Hermes Cloud requested organization selection without returning organizations.".into())
            } else {
                Ok((orgs, Vec::new(), None))
            }
        }
        _ => Err(format!("Hermes Cloud discovery returned HTTP {status}")),
    }
}

async fn sign_in_portal(
    desktop: DesktopContext,
    data_dir: std::path::PathBuf,
    portal: String,
    mut state: Signal<CloudState>,
) {
    {
        let mut current = state.write();
        current.loading = true;
        current.error = None;
        current.message = None;
    }
    let window = open_cloud_window(&desktop, &data_dir, true, "Sign in to Hermes Cloud").await;
    if let Err(error) = window.webview.load_url(&portal) {
        window.close();
        let mut current = state.write();
        current.loading = false;
        current.error = Some(error.to_string());
        return;
    }
    for _ in 0..LOGIN_POLL_ATTEMPTS {
        tokio::time::sleep(Duration::from_millis(750)).await;
        match has_portal_session(&window, &portal) {
            Ok(true) => {
                window.close();
                let mut current = state.write();
                current.loading = false;
                current.signed_in = true;
                current.message = Some("Signed in to Hermes Cloud. Discover agents to continue.".into());
                return;
            }
            Ok(false) => {}
            Err(error) => {
                window.close();
                let mut current = state.write();
                current.loading = false;
                current.error = Some(error);
                return;
            }
        }
    }
    window.close();
    let mut current = state.write();
    current.loading = false;
    current.error = Some("Hermes Cloud sign-in timed out.".into());
}

async fn discover_agents(
    desktop: DesktopContext,
    data_dir: std::path::PathBuf,
    portal: String,
    org: Option<String>,
    mut state: Signal<CloudState>,
) {
    {
        let mut current = state.write();
        current.loading = true;
        current.error = None;
        current.message = None;
    }
    let window = open_cloud_window(&desktop, &data_dir, false, "Hermes Cloud").await;
    if let Err(error) = window.webview.load_url(&portal) {
        window.close();
        let mut current = state.write();
        current.loading = false;
        current.error = Some(error.to_string());
        return;
    }
    tokio::time::sleep(Duration::from_millis(750)).await;
    match has_portal_session(&window, &portal) {
        Ok(true) => {}
        Ok(false) => {
            window.close();
            let mut current = state.write();
            current.loading = false;
            current.signed_in = false;
            current.error = Some("Sign in to Hermes Cloud before discovering agents.".into());
            return;
        }
        Err(error) => {
            window.close();
            let mut current = state.write();
            current.loading = false;
            current.error = Some(error);
            return;
        }
    }
    let target = org.as_deref().map_or_else(
        || format!("{portal}/api/agents"),
        |org| {
            let encoded = url::form_urlencoded::byte_serialize(org.as_bytes()).collect::<String>();
            format!("{portal}/api/agents?org={encoded}")
        },
    );
    let result = browser_fetch_json(&window, &target).await;
    window.close();
    let mut current = state.write();
    current.loading = false;
    match result.and_then(|(status, body)| parse_discovery(status, &body)) {
        Ok((orgs, agents, selected)) => {
            current.signed_in = true;
            current.orgs = orgs;
            current.agents = agents;
            current.selected_org = selected.or(org);
            current.message = Some(if current.agents.is_empty() {
                if current.orgs.len() > 1 {
                    "Choose an organization to discover its agents.".into()
                } else {
                    "No hosted agents were returned for this Cloud account.".into()
                }
            } else {
                format!("Discovered {} Hermes Cloud agent(s).", current.agents.len())
            });
        }
        Err(error) => {
            if error.contains("expired") {
                current.signed_in = false;
            }
            current.error = Some(error);
        }
    }
}

async fn connect_agent(
    desktop: DesktopContext,
    data_dir: std::path::PathBuf,
    portal: String,
    services: AppServices,
    request: CloudConnectRequest,
    mut state: Signal<CloudState>,
) {
    {
        let mut current = state.write();
        current.loading = true;
        current.error = None;
        current.message = None;
    }
    let Some(raw_url) = request.agent.dashboard_url.as_deref() else {
        let mut current = state.write();
        current.loading = false;
        current.error = Some("The selected Cloud agent does not publish a dashboard URL.".into());
        return;
    };
    let base_url = match normalize_http_url(raw_url) {
        Ok(url) => url,
        Err(error) => {
            let mut current = state.write();
            current.loading = false;
            current.error = Some(error);
            return;
        }
    };
    let window = open_cloud_window(&desktop, &data_dir, false, "Hermes Cloud agent sign-in").await;
    match has_portal_session(&window, &portal) {
        Ok(true) => {}
        Ok(false) => {
            window.close();
            let mut current = state.write();
            current.loading = false;
            current.signed_in = false;
            current.error = Some("Your Hermes Cloud session expired. Sign in again.".into());
            return;
        }
        Err(error) => {
            window.close();
            let mut current = state.write();
            current.loading = false;
            current.error = Some(error);
            return;
        }
    }
    if let Err(error) = clear_gateway_session(&window, &base_url) {
        window.close();
        let mut current = state.write();
        current.loading = false;
        current.error = Some(error);
        return;
    }
    let login_url = format!("{base_url}/login");
    if let Err(error) = window.webview.load_url(&login_url) {
        window.close();
        let mut current = state.write();
        current.loading = false;
        current.error = Some(error.to_string());
        return;
    }
    let mut cookies = Vec::new();
    for _ in 0..AGENT_LOGIN_POLL_ATTEMPTS {
        tokio::time::sleep(Duration::from_millis(250)).await;
        match gateway_session_cookies(&window, &base_url) {
            Ok(found) if !found.is_empty() => {
                cookies = found;
                break;
            }
            Ok(_) => {}
            Err(error) => {
                window.close();
                let mut current = state.write();
                current.loading = false;
                current.error = Some(error);
                return;
            }
        }
    }
    window.close();
    if cookies.is_empty() {
        let mut current = state.write();
        current.loading = false;
        current.error = Some("Hermes Cloud could not establish the selected agent session.".into());
        return;
    }
    if let Err(error) = services
        .connection
        .adopt_cloud_gateway_session(base_url.clone(), cookies)
        .await
    {
        let mut current = state.write();
        current.loading = false;
        current.error = Some(error.to_string());
        return;
    }
    let input = ConnectionConfigInput {
        mode: ConnectionMode::Cloud,
        profile: request.profile,
        remote_auth_mode: Some(RemoteAuthMode::Oauth),
        remote_url: Some(base_url.clone()),
        cloud_org: request.org,
        ..ConnectionConfigInput::default()
    };
    match services.connection.apply_config(&input).await {
        Ok(_) => {
            let mut current = state.write();
            current.loading = false;
            current.signed_in = true;
            current.connected_url = Some(base_url);
            current.message = Some(format!("Connected to {} through Hermes Cloud.", request.agent.name));
        }
        Err(error) => {
            let mut current = state.write();
            current.loading = false;
            current.error = Some(error.to_string());
        }
    }
}

#[component]
pub fn CloudBridge(children: Element) -> Element {
    let desktop = dioxus::desktop::window();
    let data_dir = use_context::<DesktopDataDir>().0.clone();
    let services = use_context::<AppServices>();
    let portal_result = portal_base_url();
    let portal = portal_result
        .clone()
        .unwrap_or_else(|_| DEFAULT_PORTAL_BASE_URL.to_owned());
    let initial_error = portal_result.err();
    let mut state = use_signal(move || CloudState {
        portal_base_url: portal.clone(),
        error: initial_error,
        ..CloudState::default()
    });

    let login_desktop = desktop.clone();
    let login_data_dir = data_dir.clone();
    let login_portal = portal.clone();
    let login = Callback::new(move |()| {
        let desktop = login_desktop.clone();
        let data_dir = login_data_dir.clone();
        let portal = login_portal.clone();
        spawn(sign_in_portal(desktop, data_dir, portal, state));
    });

    let discover_desktop = desktop.clone();
    let discover_data_dir = data_dir.clone();
    let discover_portal = portal.clone();
    let discover = Callback::new(move |org: Option<String>| {
        let desktop = discover_desktop.clone();
        let data_dir = discover_data_dir.clone();
        let portal = discover_portal.clone();
        spawn(discover_agents(desktop, data_dir, portal, org, state));
    });

    let connect_desktop = desktop.clone();
    let connect_data_dir = data_dir.clone();
    let connect_portal = portal.clone();
    let connect_services = services.clone();
    let connect = Callback::new(move |request: CloudConnectRequest| {
        let desktop = connect_desktop.clone();
        let data_dir = connect_data_dir.clone();
        let portal = connect_portal.clone();
        let services = connect_services.clone();
        spawn(connect_agent(desktop, data_dir, portal, services, request, state));
    });

    let logout_desktop = desktop.clone();
    let logout_data_dir = data_dir;
    let logout_portal = portal;
    let logout = Callback::new(move |()| {
        let desktop = logout_desktop.clone();
        let data_dir = logout_data_dir.clone();
        let portal = logout_portal.clone();
        spawn(async move {
            state.write().loading = true;
            let window = open_cloud_window(&desktop, &data_dir, false, "Hermes Cloud").await;
            let result = clear_portal_session(&window, &portal);
            window.close();
            let mut current = state.write();
            current.loading = false;
            match result {
                Ok(()) => {
                    current.signed_in = false;
                    current.orgs.clear();
                    current.agents.clear();
                    current.selected_org = None;
                    current.message = Some("Signed out of the Hermes Cloud portal. Existing agent connectivity is unchanged.".into());
                    current.error = None;
                }
                Err(error) => current.error = Some(error),
            }
        });
    });

    use_context_provider(move || CloudActions {
        state,
        login,
        logout,
        discover,
        connect,
    });

    rsx! { {children} }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{decode_script_result, normalize_http_url, parse_discovery};

    #[test]
    fn cloud_urls_reject_credentials_and_non_http_schemes() {
        assert_eq!(
            normalize_http_url("https://cloud.example.test/path/").unwrap(),
            "https://cloud.example.test/path"
        );
        assert!(normalize_http_url("file:///secret").is_err());
        assert!(normalize_http_url("https://user:pass@cloud.example.test").is_err());
    }

    #[test]
    fn multi_org_discovery_is_bounded_and_explicit() {
        let body = json!({
            "error": "org_selection_required",
            "orgs": [
                {"id": "a", "slug": "alpha", "name": "Alpha"},
                {"id": "b", "name": "Beta"}
            ]
        });
        let (orgs, agents, selected) = parse_discovery(409, &body).unwrap();
        assert_eq!(orgs.len(), 2);
        assert!(agents.is_empty());
        assert!(selected.is_none());
        assert_eq!(orgs[0].selection_key(), "alpha");
        assert_eq!(orgs[1].selection_key(), "b");
    }

    #[test]
    fn discovery_trims_agents_and_drops_unsafe_dashboard_urls() {
        let body = json!({
            "agents": [
                {"id":"one","name":"One","dashboardUrl":"https://agent.example.test/base"},
                {"id":"two","dashboardUrl":"file:///escape"}
            ],
            "org": {"id":"org","slug":"team","name":"Team"}
        });
        let (_, agents, selected) = parse_discovery(200, &body).unwrap();
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].dashboard_url.as_deref(), Some("https://agent.example.test/base"));
        assert!(agents[1].dashboard_url.is_none());
        assert_eq!(selected.as_deref(), Some("team"));
    }

    #[test]
    fn script_result_accepts_direct_and_string_wrapped_json() {
        let direct = decode_script_result(r#"{"status":200,"body":"{}"}"#).unwrap();
        assert_eq!(direct["status"], 200);
        let wrapped = decode_script_result(r#""{\"status\":200,\"body\":\"{}\"}""#).unwrap();
        assert_eq!(wrapped["status"], 200);
    }
}
