use std::{collections::BTreeMap, f64::consts::TAU};

use dioxus::prelude::*;
use hermes_core::AppServices;
use hermes_protocol::{LearningEdge, LearningGraph, LearningNode};

use super::{SettingsUiState, Surface};

const MAX_RENDERED_NODES: usize = 300;
const VIEW_WIDTH: f64 = 1_000.0;
const VIEW_HEIGHT: f64 = 680.0;

#[derive(Clone, Debug, PartialEq)]
struct PositionedNode {
    node: LearningNode,
    x: f64,
    y: f64,
}

fn node_matches(node: &LearningNode, query: &str, kind: &str, category: &str) -> bool {
    let query = query.trim().to_ascii_lowercase();
    (kind == "all" || node.kind == kind)
        && (category == "all" || node.category == category)
        && (query.is_empty()
            || node.label.to_ascii_lowercase().contains(&query)
            || node.category.to_ascii_lowercase().contains(&query)
            || node.state.to_ascii_lowercase().contains(&query))
}

fn radial_layout(nodes: &[LearningNode]) -> Vec<PositionedNode> {
    let bounded_count = nodes.len().clamp(1, MAX_RENDERED_NODES);
    let count = f64::from(u32::try_from(bounded_count).expect("rendered node count fits u32"));
    nodes
        .iter()
        .take(MAX_RENDERED_NODES)
        .enumerate()
        .map(|(index, node)| {
            let index = u32::try_from(index).expect("rendered node index fits u32");
            let angle = f64::from(index) / count * TAU - TAU / 4.0;
            let layer = 0.55 + 0.45 * (f64::from(index % 7) / 6.0);
            PositionedNode {
                node: node.clone(),
                x: VIEW_WIDTH / 2.0 + angle.cos() * 410.0 * layer,
                y: VIEW_HEIGHT / 2.0 + angle.sin() * 275.0 * layer,
            }
        })
        .collect()
}

fn visible_edges(
    edges: &[LearningEdge],
    positions: &BTreeMap<String, (f64, f64)>,
) -> Vec<(String, f64, f64, f64, f64)> {
    edges
        .iter()
        .filter_map(|edge| {
            let (x1, y1) = positions.get(&edge.source)?;
            let (x2, y2) = positions.get(&edge.target)?;
            Some((
                format!("{}:{}", edge.source, edge.target),
                *x1,
                *y1,
                *x2,
                *y2,
            ))
        })
        .take(1_200)
        .collect()
}

fn node_colour(kind: &str) -> &'static str {
    match kind {
        "skill" => "#7c9cff",
        "memory" => "#b690ff",
        _ => "#7ccfc0",
    }
}

#[component]
pub(super) fn Starmap() -> Element {
    let services = use_context::<AppServices>();
    let settings = use_context::<SettingsUiState>();
    let settings_signal = settings.settings;
    let mut graph = use_signal(|| None::<LearningGraph>);
    let mut loading = use_signal(|| true);
    let mut error = use_signal(|| None::<String>);
    let mut refresh = use_signal(|| 0_u64);
    let mut query = use_signal(String::new);
    let mut kind = use_signal(|| "all".to_owned());
    let mut category = use_signal(|| "all".to_owned());
    let mut selected_id = use_signal(|| None::<String>);

    let graph_service = services.learning.clone();
    let _loading = use_resource(move || {
        let service = graph_service.clone();
        let profile = settings_signal().profile;
        let _revision = refresh();
        async move {
            loading.set(true);
            match service.graph(profile.as_deref()).await {
                Ok(next) => {
                    if selected_id()
                        .as_ref()
                        .is_some_and(|id| !next.nodes.iter().any(|node| &node.id == id))
                    {
                        selected_id.set(None);
                    }
                    graph.set(Some(next));
                    error.set(None);
                }
                Err(problem) => {
                    graph.set(None);
                    error.set(Some(problem.to_string()));
                }
            }
            loading.set(false);
        }
    });

    let current = graph();
    let current_query = query();
    let current_kind = kind();
    let current_category = category();
    let mut categories = current
        .as_ref()
        .map(|value| {
            value
                .nodes
                .iter()
                .map(|node| node.category.clone())
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    categories.sort();
    categories.dedup();
    let mut visible_nodes = current
        .as_ref()
        .map(|value| {
            value
                .nodes
                .iter()
                .filter(|node| node_matches(node, &current_query, &current_kind, &current_category))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    visible_nodes.sort_by(|left, right| {
        right
            .use_count
            .cmp(&left.use_count)
            .then_with(|| right.pinned.cmp(&left.pinned))
            .then_with(|| left.label.cmp(&right.label))
    });
    let filtered_total = visible_nodes.len();
    visible_nodes.truncate(MAX_RENDERED_NODES);
    let positioned = radial_layout(&visible_nodes);
    let positions = positioned
        .iter()
        .map(|item| (item.node.id.clone(), (item.x, item.y)))
        .collect::<BTreeMap<_, _>>();
    let edges = current
        .as_ref()
        .map(|value| visible_edges(&value.edges, &positions))
        .unwrap_or_default();
    let selected = current.as_ref().and_then(|value| {
        selected_id()
            .as_ref()
            .and_then(|id| value.nodes.iter().find(|node| &node.id == id))
            .cloned()
    });
    let profile = settings_signal()
        .profile
        .unwrap_or_else(|| "default".to_owned());

    rsx! {
        Surface { eyebrow: "Learning", title: "Starmap", subtitle: "Explore the profile-scoped skills and memories Hermes has learned through a bounded native graph boundary.",
            div { class: "settings-toolbar",
                span { class: "scope-pill", "profile: {profile}" }
                input {
                    class: "settings-input",
                    aria_label: "Search Starmap nodes",
                    placeholder: "Search label, category, or state",
                    value: "{current_query}",
                    oninput: move |event| query.set(event.value())
                }
                select { class: "settings-select", aria_label: "Filter Starmap kind", value: "{current_kind}", onchange: move |event| kind.set(event.value()),
                    option { value: "all", "All kinds" }
                    option { value: "skill", "Skills" }
                    option { value: "memory", "Memories" }
                }
                select { class: "settings-select", aria_label: "Filter Starmap category", value: "{current_category}", onchange: move |event| category.set(event.value()),
                    option { value: "all", "All categories" }
                    for value in categories { option { value: "{value}", "{value}" } }
                }
                button { class: "button", disabled: loading(), onclick: move |_| refresh.set(refresh() + 1), "Refresh" }
            }
            if loading() {
                div { class: "loading-state", role: "status", "◌ Loading bounded learning graph" }
            }
            if let Some(problem) = error() {
                div { class: "error-state", role: "alert", h2 { "Starmap unavailable" } p { "{problem}" } }
            }
            if let Some(current) = current {
                div { class: "settings-toolbar",
                    span { class: "badge", "{current.nodes.len()} nodes" }
                    span { class: "badge", "{current.edges.len()} relationships" }
                    span { class: "badge", "{current.memory.len()} memory cards" }
                    if filtered_total > MAX_RENDERED_NODES { span { class: "muted", "Showing the top {MAX_RENDERED_NODES} matching nodes by usage." } }
                }
                div { style: "display:grid;grid-template-columns:minmax(0,3fr) minmax(17rem,1fr);gap:1rem;min-height:0;",
                    section { class: "panel", style: "min-width:0;overflow:hidden;",
                        header { class: "panel-title", "Learning graph ({filtered_total} matches)" }
                        if positioned.is_empty() {
                            div { class: "settings-empty", h2 { "No matching nodes" } p { "Change the filters or refresh after Hermes learns something new." } }
                        } else {
                            svg { view_box: "0 0 1000 680", role: "img", "aria-label": "Starmap learning graph", style: "display:block;width:100%;min-height:30rem;background:radial-gradient(circle at center,rgba(124,156,255,.09),transparent 65%);",
                                for (id, x1, y1, x2, y2) in edges {
                                    line { key: "{id}", x1: "{x1}", y1: "{y1}", x2: "{x2}", y2: "{y2}", stroke: "rgba(140,160,195,.28)", stroke_width: "1.5" }
                                }
                                for item in positioned {
                                    {
                                        let id = item.node.id.clone();
                                        let is_selected = selected_id().as_deref() == Some(item.node.id.as_str());
                                        let bounded_uses = u32::try_from(item.node.use_count.min(80)).expect("bounded use count fits u32");
                                        let radius = (7.0 + f64::from(bounded_uses).sqrt()).min(16.0);
                                        let shown_radius = if is_selected { radius + 4.0 } else { radius };
                                        let colour = node_colour(&item.node.kind);
                                        let stroke = if is_selected { "white" } else { "rgba(255,255,255,.45)" };
                                        let stroke_width = if is_selected { "3" } else { "1" };
                                        rsx! { g { key: "{item.node.id}", class: "starmap-node", onclick: move |_| selected_id.set(Some(id.clone())),
                                            title { "{item.node.label} · {item.node.category}" }
                                            circle { cx: "{item.x}", cy: "{item.y}", r: "{shown_radius}", fill: "{colour}", stroke: "{stroke}", stroke_width: "{stroke_width}", style: "cursor:pointer;" }
                                        } }
                                    }
                                }
                            }
                        }
                    }
                    section { class: "panel", style: "min-width:0;max-height:38rem;overflow:auto;",
                        header { class: "panel-title", "Selection" }
                        if let Some(node) = selected {
                            h2 { "{node.label}" }
                            p { class: "muted", "{node.kind} · {node.category}" }
                            div { class: "integrity-grid",
                                div { class: "integrity-item", span { "State" } strong { "{node.state}" } }
                                div { class: "integrity-item", span { "Use count" } strong { "{node.use_count}" } }
                                div { class: "integrity-item", span { "Pinned" } strong { if node.pinned { "Yes" } else { "No" } } }
                                if let Some(source) = node.memory_source { div { class: "integrity-item", span { "Memory source" } strong { "{source}" } } }
                                if let Some(created_by) = node.created_by { div { class: "integrity-item", span { "Created by" } strong { "{created_by}" } } }
                            }
                            code { title: "{node.id}", style: "display:block;word-break:break-all;", "{node.id}" }
                        } else {
                            p { class: "muted", "Select a node in the map or the ranked list." }
                        }
                        header { class: "panel-title", style: "margin-top:1rem;", "Ranked nodes" }
                        for node in visible_nodes.into_iter().take(100) {
                            {
                                let id = node.id.clone();
                                rsx! { button { class: "settings-row", style: "width:100%;text-align:left;", onclick: move |_| selected_id.set(Some(id.clone())),
                                    div { class: "settings-row-copy", strong { "{node.label}" } p { "{node.kind} · {node.category} · used {node.use_count}×" } }
                                } }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, label: &str, kind: &str, category: &str) -> LearningNode {
        LearningNode {
            id: id.into(),
            label: label.into(),
            kind: kind.into(),
            category: category.into(),
            ..LearningNode::default()
        }
    }

    #[test]
    fn filters_case_insensitively_across_kind_and_category() {
        let value = node("1", "Cargo Patterns", "skill", "Rust");
        assert!(node_matches(&value, "cargo", "skill", "Rust"));
        assert!(!node_matches(&value, "cargo", "memory", "Rust"));
        assert!(!node_matches(&value, "cargo", "skill", "Python"));
    }

    #[test]
    fn radial_layout_is_deterministic_and_render_bounded() {
        let nodes = (0..500)
            .map(|index| node(&index.to_string(), "Node", "memory", "General"))
            .collect::<Vec<_>>();
        let first = radial_layout(&nodes);
        let second = radial_layout(&nodes);
        assert_eq!(first, second);
        assert_eq!(first.len(), MAX_RENDERED_NODES);
        assert!(
            first
                .iter()
                .all(|item| item.x.is_finite() && item.y.is_finite())
        );
    }

    #[test]
    fn edges_only_render_when_both_nodes_are_visible() {
        let positions = [("a".into(), (1.0, 2.0)), ("b".into(), (3.0, 4.0))]
            .into_iter()
            .collect();
        let edges = [
            LearningEdge {
                source: "a".into(),
                target: "b".into(),
            },
            LearningEdge {
                source: "a".into(),
                target: "hidden".into(),
            },
        ];
        assert_eq!(visible_edges(&edges, &positions).len(), 1);
    }
}
