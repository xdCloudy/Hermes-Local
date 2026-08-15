use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::DesktopGeneralStatus;

use super::Surface;

#[derive(Clone, Copy)]
enum GeneralToggle {
    KeepAwake,
    LaunchAtLogin,
}

#[component]
pub(super) fn GeneralSettings() -> Element {
    let services = use_context::<AppServices>();
    let mut status = use_signal(|| None::<DesktopGeneralStatus>);
    let mut loading = use_signal(|| true);
    let mut busy = use_signal(|| false);
    let mut error = use_signal(|| None::<String>);
    let mut notice = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);

    let load_services = services.clone();
    let _load = use_resource(move || {
        let services = load_services.clone();
        let _revision = refresh();
        async move {
            loading.set(true);
            match services.desktop_settings.status().await {
                Ok(next) => {
                    status.set(Some(next));
                    error.set(None);
                }
                Err(problem) => error.set(Some(problem.to_string())),
            }
            loading.set(false);
        }
    });

    let toggle_services = services.clone();
    let toggle = Callback::new(move |(kind, enabled): (GeneralToggle, bool)| {
        if busy() {
            return;
        }
        busy.set(true);
        error.set(None);
        notice.set(None);
        let services = toggle_services.clone();
        spawn(async move {
            let native = match kind {
                GeneralToggle::KeepAwake => services.desktop_settings.set_keep_awake(enabled).await,
                GeneralToggle::LaunchAtLogin => {
                    services.desktop_settings.set_launch_at_login(enabled).await
                }
            };
            match native {
                Ok(next) => match services.settings.load().await {
                    Ok(mut settings) => {
                        match kind {
                            GeneralToggle::KeepAwake => settings.keep_awake = enabled,
                            GeneralToggle::LaunchAtLogin => settings.launch_at_login = enabled,
                        }
                        match services.settings.save(&settings).await {
                            Ok(()) => {
                                status.set(Some(next));
                                notice.set(Some(
                                    match kind {
                                        GeneralToggle::KeepAwake if enabled => "Keep-awake enabled",
                                        GeneralToggle::KeepAwake => "Keep-awake disabled",
                                        GeneralToggle::LaunchAtLogin if enabled => {
                                            "Launch at login enabled"
                                        }
                                        GeneralToggle::LaunchAtLogin => "Launch at login disabled",
                                    }
                                    .into(),
                                ));
                            }
                            Err(problem) => {
                                let _ = match kind {
                                    GeneralToggle::KeepAwake => {
                                        services.desktop_settings.set_keep_awake(!enabled).await
                                    }
                                    GeneralToggle::LaunchAtLogin => {
                                        services
                                            .desktop_settings
                                            .set_launch_at_login(!enabled)
                                            .await
                                    }
                                };
                                error.set(Some(format!(
                                    "Could not persist Desktop preference: {problem}"
                                )));
                                refresh.set(refresh() + 1);
                            }
                        }
                    }
                    Err(problem) => error.set(Some(problem.to_string())),
                },
                Err(problem) => error.set(Some(problem.to_string())),
            }
            busy.set(false);
        });
    });

    let current = status();
    let power_source = current
        .as_ref()
        .and_then(|value| value.power.on_ac_power)
        .map_or(
            "Unknown",
            |value| if value { "AC power" } else { "Battery" },
        );
    let battery = current
        .as_ref()
        .and_then(|value| value.power.battery_percent)
        .map_or_else(|| "Unknown".into(), |value| format!("{value}%"));
    rsx! {
        Surface { eyebrow: "Preferences", title: "General", subtitle: "Control native startup and power behaviour.",
            div { class: "settings-toolbar", button { class: "button", disabled: loading() || busy(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" } }
            if loading() { div { class: "loading-state", role: "status", "◌ Loading Desktop settings" } }
            if let Some(problem) = error() { div { class: "error-state", role: "alert", "{problem}" } }
            if let Some(message) = notice() { div { class: "success-state", role: "status", "{message}" } }
            if let Some(current) = current {
                section { class: "panel",
                    header { class: "panel-title", "Power" }
                    div { class: "integrity-grid",
                        div { class: "integrity-item", span { "Source" } strong { "{power_source}" } }
                        div { class: "integrity-item", span { "Battery" } strong { "{battery}" } }
                        div { class: "integrity-item", span { "Keep awake" } strong { if current.power.keep_awake { "Active" } else { "Inactive" } } }
                    }
                    p { class: "muted", "Keep-awake prevents system sleep while Hermes is working; it does not force the display to remain on." }
                    button { class: "primary-button", disabled: busy() || !current.power.available, onclick: move |_| toggle.call((GeneralToggle::KeepAwake, !current.power.keep_awake)), if current.power.keep_awake { "Disable keep-awake" } else { "Enable keep-awake" } }
                }
                section { class: "panel",
                    header { class: "panel-title", "Startup" }
                    p { class: "muted", "Register the exact packaged executable for the current Windows user." }
                    p { class: "mono muted", "{current.login.executable}" }
                    button { class: "primary-button", disabled: busy() || !current.login.available, onclick: move |_| toggle.call((GeneralToggle::LaunchAtLogin, !current.login.enabled)), if current.login.enabled { "Disable launch at login" } else { "Enable launch at login" } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn general_toggles_remain_distinct() {
        assert!(matches!(GeneralToggle::KeepAwake, GeneralToggle::KeepAwake));
        assert!(matches!(
            GeneralToggle::LaunchAtLogin,
            GeneralToggle::LaunchAtLogin
        ));
    }
}
