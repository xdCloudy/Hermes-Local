use std::collections::BTreeSet;

const MAX_CONTRIBUTIONS: usize = 256;
const MAX_SOURCE_CONTRIBUTIONS: usize = 128;
const MAX_FIELD_BYTES: usize = 256;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ContributionArea {
    Route,
    PrimaryNavigation,
    SecondaryNavigation,
    Launcher,
    Pane,
    Command,
    Status,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributionHost {
    Shared,
    Desktop,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributionCommand {
    Navigate(&'static str),
    ToggleSidebar,
    ToggleRightRail,
    ToggleStatus,
    OpenFind,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusContribution {
    Brand,
    Gateway,
    Runtime,
    Spacer,
    Tasks,
    Encoding,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContributionPayload {
    Route {
        path: &'static str,
    },
    Navigation {
        route: &'static str,
    },
    Launcher {
        route: &'static str,
        detail: &'static str,
    },
    Pane {
        route: &'static str,
    },
    Command {
        action: ContributionCommand,
        category: &'static str,
        shortcut: &'static str,
    },
    Status {
        slot: StatusContribution,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Contribution {
    pub id: &'static str,
    pub source: &'static str,
    pub area: ContributionArea,
    pub host: ContributionHost,
    pub order: i16,
    pub label: &'static str,
    pub icon: &'static str,
    pub payload: ContributionPayload,
}

#[derive(Clone, Debug)]
pub struct ContributionRegistry {
    entries: Vec<Contribution>,
}

impl Default for ContributionRegistry {
    fn default() -> Self {
        Self::built_in()
    }
}

impl ContributionRegistry {
    pub fn built_in() -> Self {
        Self::from_entries(BUILT_INS).expect("built-in contribution registry must be valid")
    }

    pub fn from_entries(entries: &[Contribution]) -> Result<Self, String> {
        validate_entries(entries)?;
        let mut entries = entries.to_vec();
        entries.sort_by_key(|entry| (entry.area, entry.order, entry.id));
        Ok(Self { entries })
    }

    pub fn extend_source(
        &self,
        source: &'static str,
        entries: &[Contribution],
    ) -> Result<Self, String> {
        validate_field(source, "contribution source")?;
        if source == "core" || entries.len() > MAX_SOURCE_CONTRIBUTIONS {
            return Err("external contribution source exceeds its authority".into());
        }
        if entries.iter().any(|entry| entry.source != source) {
            return Err("contribution source does not match its registration scope".into());
        }
        let mut combined = self.entries.clone();
        combined.extend_from_slice(entries);
        Self::from_entries(&combined)
    }

    pub fn entries(&self, area: ContributionArea, host: ContributionHost) -> Vec<Contribution> {
        self.entries
            .iter()
            .copied()
            .filter(|entry| entry.area == area && entry.host == host)
            .collect()
    }

    pub fn route_path(&self, id: &str) -> Option<&'static str> {
        self.entries.iter().find_map(|entry| {
            (entry.id == id).then_some(entry).and_then(|entry| {
                if let ContributionPayload::Route { path } = entry.payload {
                    Some(path)
                } else {
                    None
                }
            })
        })
    }

    pub fn all(&self) -> &[Contribution] {
        &self.entries
    }
}

fn validate_entries(entries: &[Contribution]) -> Result<(), String> {
    if entries.len() > MAX_CONTRIBUTIONS {
        return Err("contribution registry exceeds its entry limit".into());
    }
    let mut ids = BTreeSet::new();
    for entry in entries {
        validate_id(entry.id)?;
        validate_field(entry.source, "contribution source")?;
        validate_field(entry.label, "contribution label")?;
        validate_optional_field(entry.icon, "contribution icon")?;
        if !ids.insert(entry.id) {
            return Err(format!("duplicate contribution id: {}", entry.id));
        }
        if !payload_matches_area(entry.area, entry.payload) {
            return Err(format!(
                "contribution {} has a payload outside its declared area",
                entry.id
            ));
        }
        match entry.payload {
            ContributionPayload::Route { path } => validate_route(path)?,
            ContributionPayload::Navigation { route }
            | ContributionPayload::Pane { route }
            | ContributionPayload::Launcher { route, .. } => validate_id(route)?,
            ContributionPayload::Command {
                action,
                category,
                shortcut,
            } => {
                validate_field(category, "command category")?;
                validate_optional_field(shortcut, "command shortcut")?;
                if let ContributionCommand::Navigate(route) = action {
                    validate_id(route)?;
                }
            }
            ContributionPayload::Status { .. } => {}
        }
    }
    for entry in entries {
        let route = match entry.payload {
            ContributionPayload::Navigation { route }
            | ContributionPayload::Pane { route }
            | ContributionPayload::Launcher { route, .. } => Some(route),
            ContributionPayload::Command {
                action: ContributionCommand::Navigate(route),
                ..
            } => Some(route),
            _ => None,
        };
        if route.is_some_and(|route| {
            !entries.iter().any(|candidate| {
                candidate.id == route
                    && matches!(candidate.payload, ContributionPayload::Route { .. })
            })
        }) {
            return Err(format!(
                "contribution {} references an unknown route",
                entry.id
            ));
        }
    }
    Ok(())
}

fn payload_matches_area(area: ContributionArea, payload: ContributionPayload) -> bool {
    matches!(
        (area, payload),
        (ContributionArea::Route, ContributionPayload::Route { .. })
            | (
                ContributionArea::PrimaryNavigation | ContributionArea::SecondaryNavigation,
                ContributionPayload::Navigation { .. }
            )
            | (
                ContributionArea::Launcher,
                ContributionPayload::Launcher { .. }
            )
            | (ContributionArea::Pane, ContributionPayload::Pane { .. })
            | (
                ContributionArea::Command,
                ContributionPayload::Command { .. }
            )
            | (ContributionArea::Status, ContributionPayload::Status { .. })
    )
}

fn validate_id(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value.len() > MAX_FIELD_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err("invalid contribution id".into());
    }
    Ok(())
}

fn validate_field(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty()
        || value.len() > MAX_FIELD_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(format!("invalid {field}"));
    }
    Ok(())
}

fn validate_optional_field(value: &str, field: &str) -> Result<(), String> {
    if value.len() > MAX_FIELD_BYTES || value.chars().any(char::is_control) {
        return Err(format!("invalid {field}"));
    }
    Ok(())
}

fn validate_route(path: &str) -> Result<(), String> {
    if !path.starts_with('/')
        || path.len() > MAX_FIELD_BYTES
        || path.contains(['?', '#', '\\'])
        || path.chars().any(char::is_control)
    {
        return Err("invalid contribution route".into());
    }
    Ok(())
}

macro_rules! route {
    ($id:literal, $path:literal) => {
        Contribution {
            id: $id,
            source: "core",
            area: ContributionArea::Route,
            host: ContributionHost::Shared,
            order: 0,
            label: $id,
            icon: "",
            payload: ContributionPayload::Route { path: $path },
        }
    };
}

macro_rules! nav {
    ($id:literal, $area:ident, $order:literal, $label:literal, $icon:literal, $route:literal) => {
        Contribution {
            id: $id,
            source: "core",
            area: ContributionArea::$area,
            host: ContributionHost::Shared,
            order: $order,
            label: $label,
            icon: $icon,
            payload: ContributionPayload::Navigation { route: $route },
        }
    };
}

macro_rules! command {
    ($id:literal, $order:literal, $label:literal, $category:literal, $shortcut:literal, $action:expr) => {
        Contribution {
            id: $id,
            source: "core",
            area: ContributionArea::Command,
            host: ContributionHost::Desktop,
            order: $order,
            label: $label,
            icon: "",
            payload: ContributionPayload::Command {
                action: $action,
                category: $category,
                shortcut: $shortcut,
            },
        }
    };
}

pub const BUILT_INS: &[Contribution] = &[
    route!("route.home", "/"),
    route!("route.chat", "/chat"),
    route!("route.tui", "/tui"),
    route!("route.dashboard", "/dashboard"),
    route!("route.tasks", "/tasks"),
    route!("route.services", "/services"),
    route!("route.models", "/models"),
    route!("route.profiles", "/profiles"),
    route!("route.tools", "/tools"),
    route!("route.memory", "/memory"),
    route!("route.skills", "/skills"),
    route!("route.sessions", "/sessions"),
    route!("route.projects", "/projects"),
    route!("route.integrations", "/integrations"),
    route!("route.benchmarks", "/benchmarks"),
    route!("route.security", "/security"),
    route!("route.logs", "/logs"),
    route!("route.artifacts", "/artifacts"),
    route!("route.starmap", "/starmap"),
    route!("route.settings", "/settings"),
    route!("route.about", "/about"),
    route!("route.files", "/files"),
    route!("route.terminal", "/terminal"),
    route!("route.review", "/review"),
    nav!(
        "nav.home",
        PrimaryNavigation,
        10,
        "Home",
        "home",
        "route.home"
    ),
    nav!(
        "nav.chat",
        PrimaryNavigation,
        20,
        "Chat",
        "comment-discussion",
        "route.chat"
    ),
    nav!(
        "nav.tui",
        PrimaryNavigation,
        30,
        "TUI",
        "terminal",
        "route.tui"
    ),
    nav!(
        "nav.dashboard",
        PrimaryNavigation,
        40,
        "Web Dashboard",
        "dashboard",
        "route.dashboard"
    ),
    nav!(
        "nav.tasks",
        PrimaryNavigation,
        50,
        "Tasks",
        "checklist",
        "route.tasks"
    ),
    nav!(
        "nav.services",
        PrimaryNavigation,
        60,
        "Services",
        "server-process",
        "route.services"
    ),
    nav!(
        "nav.models",
        PrimaryNavigation,
        70,
        "Models",
        "hubot",
        "route.models"
    ),
    nav!(
        "nav.profiles",
        PrimaryNavigation,
        80,
        "Profiles",
        "settings-gear",
        "route.profiles"
    ),
    nav!(
        "nav.tools",
        PrimaryNavigation,
        90,
        "Tools",
        "tools",
        "route.tools"
    ),
    nav!(
        "nav.memory",
        PrimaryNavigation,
        100,
        "Memory",
        "database",
        "route.memory"
    ),
    nav!(
        "nav.skills",
        PrimaryNavigation,
        110,
        "Skills",
        "symbol-misc",
        "route.skills"
    ),
    nav!(
        "nav.sessions",
        PrimaryNavigation,
        120,
        "Sessions",
        "history",
        "route.sessions"
    ),
    nav!(
        "nav.projects",
        PrimaryNavigation,
        130,
        "Projects",
        "project",
        "route.projects"
    ),
    nav!(
        "nav.integrations",
        PrimaryNavigation,
        140,
        "Integrations",
        "plug",
        "route.integrations"
    ),
    nav!(
        "nav.benchmarks",
        PrimaryNavigation,
        150,
        "Benchmarks",
        "graph-line",
        "route.benchmarks"
    ),
    nav!(
        "nav.security",
        PrimaryNavigation,
        160,
        "Security",
        "shield",
        "route.security"
    ),
    nav!(
        "nav.logs",
        PrimaryNavigation,
        170,
        "Logs",
        "output",
        "route.logs"
    ),
    nav!(
        "nav.artifacts",
        PrimaryNavigation,
        180,
        "Artifacts",
        "package",
        "route.artifacts"
    ),
    nav!(
        "nav.starmap",
        PrimaryNavigation,
        190,
        "Starmap",
        "type-hierarchy",
        "route.starmap"
    ),
    nav!(
        "nav.settings",
        SecondaryNavigation,
        10,
        "Settings",
        "settings",
        "route.settings"
    ),
    nav!(
        "nav.about",
        SecondaryNavigation,
        20,
        "About",
        "info",
        "route.about"
    ),
    Contribution {
        id: "launcher.chat",
        source: "core",
        area: ContributionArea::Launcher,
        host: ContributionHost::Shared,
        order: 10,
        label: "Open Chat",
        icon: "rocket",
        payload: ContributionPayload::Launcher {
            route: "route.chat",
            detail: "Chat with Hermes through the local Agent.",
        },
    },
    Contribution {
        id: "launcher.tui",
        source: "core",
        area: ContributionArea::Launcher,
        host: ContributionHost::Shared,
        order: 20,
        label: "Open TUI",
        icon: "terminal",
        payload: ContributionPayload::Launcher {
            route: "route.tui",
            detail: "Run the keyboard-driven Hermes terminal UI.",
        },
    },
    Contribution {
        id: "launcher.logs",
        source: "core",
        area: ContributionArea::Launcher,
        host: ContributionHost::Shared,
        order: 30,
        label: "View Logs",
        icon: "output",
        payload: ContributionPayload::Launcher {
            route: "route.logs",
            detail: "Inspect service logs without exposing secrets.",
        },
    },
    Contribution {
        id: "pane.files",
        source: "core",
        area: ContributionArea::Pane,
        host: ContributionHost::Desktop,
        order: 10,
        label: "Files",
        icon: "files",
        payload: ContributionPayload::Pane {
            route: "route.files",
        },
    },
    Contribution {
        id: "pane.terminal",
        source: "core",
        area: ContributionArea::Pane,
        host: ContributionHost::Desktop,
        order: 20,
        label: "Terminal",
        icon: "terminal",
        payload: ContributionPayload::Pane {
            route: "route.terminal",
        },
    },
    Contribution {
        id: "pane.review",
        source: "core",
        area: ContributionArea::Pane,
        host: ContributionHost::Desktop,
        order: 30,
        label: "Review",
        icon: "git-pull-request",
        payload: ContributionPayload::Pane {
            route: "route.review",
        },
    },
    command!(
        "command.nav.home",
        10,
        "Go to Home",
        "Navigation",
        "",
        ContributionCommand::Navigate("route.home")
    ),
    command!(
        "command.nav.chat",
        20,
        "Go to Chat",
        "Navigation",
        "",
        ContributionCommand::Navigate("route.chat")
    ),
    command!(
        "command.nav.projects",
        30,
        "Go to Projects",
        "Navigation",
        "",
        ContributionCommand::Navigate("route.projects")
    ),
    command!(
        "command.nav.skills",
        40,
        "Go to Skills",
        "Navigation",
        "",
        ContributionCommand::Navigate("route.skills")
    ),
    command!(
        "command.nav.settings",
        50,
        "Open Settings",
        "Navigation",
        "Ctrl/Cmd+,",
        ContributionCommand::Navigate("route.settings")
    ),
    command!(
        "command.view.files",
        60,
        "Show Files",
        "View",
        "",
        ContributionCommand::Navigate("route.files")
    ),
    command!(
        "command.view.terminal",
        70,
        "Show Terminal",
        "View",
        "Ctrl+`",
        ContributionCommand::Navigate("route.terminal")
    ),
    command!(
        "command.view.review",
        80,
        "Show Review",
        "View",
        "Ctrl/Cmd+G",
        ContributionCommand::Navigate("route.review")
    ),
    command!(
        "command.view.sidebar",
        90,
        "Toggle Sidebar",
        "View",
        "Ctrl/Cmd+B",
        ContributionCommand::ToggleSidebar
    ),
    command!(
        "command.view.rightRail",
        100,
        "Toggle Right Sidebar",
        "View",
        "Ctrl/Cmd+J",
        ContributionCommand::ToggleRightRail
    ),
    command!(
        "command.view.status",
        110,
        "Toggle Status Bar",
        "View",
        "Ctrl/Cmd+Shift+S",
        ContributionCommand::ToggleStatus
    ),
    command!(
        "command.view.find",
        120,
        "Find in Page",
        "View",
        "Ctrl/Cmd+F",
        ContributionCommand::OpenFind
    ),
    command!(
        "command.view.zoomIn",
        130,
        "Zoom In",
        "View",
        "Ctrl/Cmd++",
        ContributionCommand::ZoomIn
    ),
    command!(
        "command.view.zoomOut",
        140,
        "Zoom Out",
        "View",
        "Ctrl/Cmd+-",
        ContributionCommand::ZoomOut
    ),
    command!(
        "command.view.zoomReset",
        150,
        "Reset Zoom to 90%",
        "View",
        "Ctrl/Cmd+0",
        ContributionCommand::ZoomReset
    ),
    Contribution {
        id: "status.brand",
        source: "core",
        area: ContributionArea::Status,
        host: ContributionHost::Desktop,
        order: 10,
        label: "Hermes Local",
        icon: "",
        payload: ContributionPayload::Status {
            slot: StatusContribution::Brand,
        },
    },
    Contribution {
        id: "status.gateway",
        source: "core",
        area: ContributionArea::Status,
        host: ContributionHost::Desktop,
        order: 20,
        label: "Gateway",
        icon: "",
        payload: ContributionPayload::Status {
            slot: StatusContribution::Gateway,
        },
    },
    Contribution {
        id: "status.runtime",
        source: "core",
        area: ContributionArea::Status,
        host: ContributionHost::Desktop,
        order: 30,
        label: "Runtime",
        icon: "",
        payload: ContributionPayload::Status {
            slot: StatusContribution::Runtime,
        },
    },
    Contribution {
        id: "status.spacer",
        source: "core",
        area: ContributionArea::Status,
        host: ContributionHost::Desktop,
        order: 40,
        label: "Spacer",
        icon: "",
        payload: ContributionPayload::Status {
            slot: StatusContribution::Spacer,
        },
    },
    Contribution {
        id: "status.tasks",
        source: "core",
        area: ContributionArea::Status,
        host: ContributionHost::Desktop,
        order: 50,
        label: "Tasks",
        icon: "",
        payload: ContributionPayload::Status {
            slot: StatusContribution::Tasks,
        },
    },
    Contribution {
        id: "status.encoding",
        source: "core",
        area: ContributionArea::Status,
        host: ContributionHost::Desktop,
        order: 60,
        label: "Encoding",
        icon: "",
        payload: ContributionPayload::Status {
            slot: StatusContribution::Encoding,
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_are_ordered_unique_and_resolve_every_reference() {
        let registry = ContributionRegistry::built_in();
        assert!(registry.all().len() > 60);
        let primary = registry.entries(
            ContributionArea::PrimaryNavigation,
            ContributionHost::Shared,
        );
        assert_eq!(primary.first().map(|entry| entry.id), Some("nav.home"));
        assert_eq!(primary.last().map(|entry| entry.id), Some("nav.starmap"));
        assert_eq!(registry.route_path("route.terminal"), Some("/terminal"));
    }

    #[test]
    fn host_and_area_filters_do_not_leak_desktop_authority() {
        let registry = ContributionRegistry::built_in();
        assert!(
            registry
                .entries(ContributionArea::Pane, ContributionHost::Shared)
                .is_empty()
        );
        assert_eq!(
            registry
                .entries(ContributionArea::Pane, ContributionHost::Desktop)
                .len(),
            3
        );
    }

    #[test]
    fn rejected_source_cannot_mutate_or_shadow_core_entries() {
        let registry = ContributionRegistry::built_in();
        let before = registry.all().len();
        let shadow = Contribution {
            id: "nav.home",
            source: "plugin.example",
            area: ContributionArea::PrimaryNavigation,
            host: ContributionHost::Shared,
            order: 1,
            label: "Shadow home",
            icon: "home",
            payload: ContributionPayload::Navigation {
                route: "route.home",
            },
        };
        assert!(registry.extend_source("plugin.example", &[shadow]).is_err());
        assert_eq!(registry.all().len(), before);
        assert_eq!(registry.route_path("route.home"), Some("/"));
    }

    #[test]
    fn invalid_and_dangling_contributions_fail_closed() {
        let dangling = Contribution {
            id: "plugin.nav",
            source: "plugin.example",
            area: ContributionArea::PrimaryNavigation,
            host: ContributionHost::Shared,
            order: 1,
            label: "Plugin",
            icon: "plug",
            payload: ContributionPayload::Navigation {
                route: "route.missing",
            },
        };
        assert!(ContributionRegistry::from_entries(&[dangling]).is_err());

        let mismatched = Contribution {
            id: "plugin.route",
            source: "plugin.example",
            area: ContributionArea::Status,
            host: ContributionHost::Desktop,
            order: 1,
            label: "Mismatched",
            icon: "",
            payload: ContributionPayload::Route {
                path: "/plugin-route",
            },
        };
        assert!(ContributionRegistry::from_entries(&[mismatched]).is_err());
    }
}
