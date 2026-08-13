use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

pub const LAYOUT_STORAGE_KEY: &str = "hermes.desktop.layoutTree.v3";
const MIN_SPLIT_RATIO: f32 = 0.15;
const MAX_SPLIT_RATIO: f32 = 0.85;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneKind {
    Workspace,
    Files,
    Terminal,
    Review,
    Preview,
}

impl PaneKind {
    pub const fn title(self) -> &'static str {
        match self {
            Self::Workspace => "Workspace",
            Self::Files => "Files",
            Self::Terminal => "Terminal",
            Self::Review => "Review",
            Self::Preview => "Preview",
        }
    }

    pub const fn route(self) -> &'static str {
        match self {
            Self::Workspace => "/chat",
            Self::Files => "/files",
            Self::Terminal => "/terminal",
            Self::Review => "/review",
            Self::Preview => "/files",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaneTab {
    pub id: String,
    pub kind: PaneKind,
    pub title: String,
}

impl PaneTab {
    fn new(id: String, kind: PaneKind) -> Self {
        Self {
            id,
            kind,
            title: kind.title().to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaneGroup {
    pub id: String,
    pub tabs: Vec<PaneTab>,
    pub active: usize,
}

impl PaneGroup {
    fn placeholder() -> Self {
        Self {
            id: "__replace__".into(),
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn active_tab(&self) -> Option<&PaneTab> {
        self.tabs.get(self.active)
    }

    fn normalize(&mut self) {
        if self.tabs.is_empty() {
            self.active = 0;
        } else {
            self.active = self.active.min(self.tabs.len() - 1);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "snake_case")]
pub enum LayoutNode {
    Group(PaneGroup),
    Split {
        id: String,
        axis: SplitAxis,
        ratio: f32,
        first: Box<LayoutNode>,
        second: Box<LayoutNode>,
    },
}

impl LayoutNode {
    fn find_group(&self, id: &str) -> Option<&PaneGroup> {
        match self {
            Self::Group(group) => (group.id == id).then_some(group),
            Self::Split { first, second, .. } => {
                first.find_group(id).or_else(|| second.find_group(id))
            }
        }
    }

    fn find_group_mut(&mut self, id: &str) -> Option<&mut PaneGroup> {
        match self {
            Self::Group(group) => (group.id == id).then_some(group),
            Self::Split { first, second, .. } => {
                if first.find_group(id).is_some() {
                    first.find_group_mut(id)
                } else {
                    second.find_group_mut(id)
                }
            }
        }
    }

    fn group_ids(&self, out: &mut Vec<String>) {
        match self {
            Self::Group(group) => out.push(group.id.clone()),
            Self::Split { first, second, .. } => {
                first.group_ids(out);
                second.group_ids(out);
            }
        }
    }

    fn pane_ids(&self, out: &mut Vec<String>) {
        match self {
            Self::Group(group) => out.extend(group.tabs.iter().map(|tab| tab.id.clone())),
            Self::Split { first, second, .. } => {
                first.pane_ids(out);
                second.pane_ids(out);
            }
        }
    }

    fn split_group(
        &mut self,
        target: &str,
        new_group: PaneGroup,
        split_id: String,
        axis: SplitAxis,
    ) -> bool {
        match self {
            Self::Group(group) if group.id == target => {
                let old = std::mem::replace(self, Self::Group(PaneGroup::placeholder()));
                *self = Self::Split {
                    id: split_id,
                    axis,
                    ratio: 0.7,
                    first: Box::new(old),
                    second: Box::new(Self::Group(new_group)),
                };
                true
            }
            Self::Group(_) => false,
            Self::Split { first, second, .. } => {
                first.split_group(target, new_group.clone(), split_id.clone(), axis)
                    || second.split_group(target, new_group, split_id, axis)
            }
        }
    }

    fn remove_group(self, target: &str) -> Option<Self> {
        match self {
            Self::Group(group) => (group.id != target).then_some(Self::Group(group)),
            Self::Split {
                id,
                axis,
                ratio,
                first,
                second,
            } => {
                let first = first.remove_group(target);
                let second = second.remove_group(target);
                match (first, second) {
                    (Some(first), Some(second)) => Some(Self::Split {
                        id,
                        axis,
                        ratio,
                        first: Box::new(first),
                        second: Box::new(second),
                    }),
                    (Some(remaining), None) | (None, Some(remaining)) => Some(remaining),
                    (None, None) => None,
                }
            }
        }
    }

    fn resize_split(&mut self, target: &str, ratio: f32) -> bool {
        match self {
            Self::Group(_) => false,
            Self::Split {
                id,
                ratio: current,
                first,
                second,
                ..
            } => {
                if id == target {
                    *current = ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
                    true
                } else {
                    first.resize_split(target, ratio) || second.resize_split(target, ratio)
                }
            }
        }
    }

    fn normalize(&mut self) {
        match self {
            Self::Group(group) => group.normalize(),
            Self::Split {
                ratio,
                first,
                second,
                ..
            } => {
                *ratio = ratio.clamp(MIN_SPLIT_RATIO, MAX_SPLIT_RATIO);
                first.normalize();
                second.normalize();
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatingBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Default for FloatingBounds {
    fn default() -> Self {
        Self {
            x: 0.18,
            y: 0.16,
            width: 0.64,
            height: 0.62,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FloatingPane {
    pub id: String,
    pub tab: PaneTab,
    pub bounds: FloatingBounds,
    pub z_index: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutModel {
    pub root: LayoutNode,
    pub floating: Vec<FloatingPane>,
    pub focused_group: String,
    next_id: u64,
}

impl Default for LayoutModel {
    fn default() -> Self {
        let workspace = PaneTab::new("pane-workspace".into(), PaneKind::Workspace);
        Self {
            root: LayoutNode::Group(PaneGroup {
                id: "group-workspace".into(),
                tabs: vec![workspace],
                active: 0,
            }),
            floating: Vec::new(),
            focused_group: "group-workspace".into(),
            next_id: 1,
        }
    }
}

impl LayoutModel {
    fn alloc(&mut self, prefix: &str) -> String {
        let id = format!("{prefix}-{}", self.next_id);
        self.next_id += 1;
        id
    }

    pub fn group_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        self.root.group_ids(&mut ids);
        ids
    }

    pub fn pane_ids(&self) -> Vec<String> {
        let mut ids = Vec::new();
        self.root.pane_ids(&mut ids);
        ids.extend(self.floating.iter().map(|pane| pane.tab.id.clone()));
        ids
    }

    pub fn focused_group(&self) -> Option<&PaneGroup> {
        self.root.find_group(&self.focused_group)
    }

    pub fn focus_group(&mut self, group_id: &str) -> bool {
        if self.root.find_group(group_id).is_some() {
            self.focused_group = group_id.to_owned();
            true
        } else {
            false
        }
    }

    pub fn add_tab(&mut self, kind: PaneKind) -> String {
        let id = self.alloc("pane");
        let tab = PaneTab::new(id.clone(), kind);
        let target = self.focused_group.clone();
        if let Some(group) = self.root.find_group_mut(&target) {
            group.tabs.push(tab);
            group.active = group.tabs.len() - 1;
        }
        id
    }

    pub fn ensure_tool_tab(&mut self, kind: PaneKind) -> String {
        if let Some(existing) = self
            .root
            .find_group(&self.focused_group)
            .and_then(|group| group.tabs.iter().find(|tab| tab.kind == kind))
            .map(|tab| tab.id.clone())
        {
            return existing;
        }
        self.add_tab(kind)
    }

    pub fn split_focused(&mut self, axis: SplitAxis, kind: PaneKind) -> Option<String> {
        let target = self.focused_group.clone();
        if self.root.find_group(&target).is_none() {
            return None;
        }
        let group_id = self.alloc("group");
        let pane_id = self.alloc("pane");
        let split_id = self.alloc("split");
        let group = PaneGroup {
            id: group_id.clone(),
            tabs: vec![PaneTab::new(pane_id, kind)],
            active: 0,
        };
        if self.root.split_group(&target, group, split_id, axis) {
            self.focused_group = group_id.clone();
            Some(group_id)
        } else {
            None
        }
    }

    pub fn resize_split(&mut self, split_id: &str, ratio: f32) -> bool {
        self.root.resize_split(split_id, ratio)
    }

    pub fn activate_tab(&mut self, group_id: &str, index: usize) -> bool {
        let Some(group) = self.root.find_group_mut(group_id) else {
            return false;
        };
        if index >= group.tabs.len() {
            return false;
        }
        group.active = index;
        self.focused_group = group_id.to_owned();
        true
    }

    pub fn cycle_tab(&mut self, backwards: bool) -> bool {
        let target = self.focused_group.clone();
        let Some(group) = self.root.find_group_mut(&target) else {
            return false;
        };
        if group.tabs.len() < 2 {
            return false;
        }
        group.active = if backwards {
            (group.active + group.tabs.len() - 1) % group.tabs.len()
        } else {
            (group.active + 1) % group.tabs.len()
        };
        true
    }

    pub fn reorder_active_tab(&mut self, delta: isize) -> bool {
        let target = self.focused_group.clone();
        let Some(group) = self.root.find_group_mut(&target) else {
            return false;
        };
        if group.tabs.len() < 2 {
            return false;
        }
        let current = group.active as isize;
        let next = (current + delta).clamp(0, group.tabs.len() as isize - 1) as usize;
        if next == group.active {
            return false;
        }
        group.tabs.swap(group.active, next);
        group.active = next;
        true
    }

    pub fn close_active_tab(&mut self) -> bool {
        let target = self.focused_group.clone();
        let Some(group) = self.root.find_group(&target) else {
            return false;
        };
        let Some(tab) = group.active_tab() else {
            return false;
        };
        if tab.kind == PaneKind::Workspace && self.pane_ids().len() == 1 {
            return false;
        }

        let remove_group = group.tabs.len() == 1;
        if remove_group {
            let root = std::mem::replace(
                &mut self.root,
                LayoutNode::Group(PaneGroup::placeholder()),
            );
            self.root = root.remove_group(&target).unwrap_or_default_node();
            self.focused_group = self
                .group_ids()
                .into_iter()
                .next()
                .unwrap_or_else(|| "group-workspace".into());
        } else if let Some(group) = self.root.find_group_mut(&target) {
            group.tabs.remove(group.active);
            group.normalize();
        }
        true
    }

    pub fn float_active(&mut self) -> Option<String> {
        let target = self.focused_group.clone();
        let group = self.root.find_group(&target)?;
        let tab = group.active_tab()?.clone();
        if tab.kind == PaneKind::Workspace && self.pane_ids().len() == 1 {
            return None;
        }
        let floating_id = self.alloc("floating");
        let z_index = self
            .floating
            .iter()
            .map(|pane| pane.z_index)
            .max()
            .unwrap_or(0)
            + 1;
        self.floating.push(FloatingPane {
            id: floating_id.clone(),
            tab,
            bounds: FloatingBounds::default(),
            z_index,
        });
        let _ = self.close_active_tab();
        Some(floating_id)
    }

    pub fn dock_floating(&mut self, floating_id: &str) -> bool {
        let Some(index) = self
            .floating
            .iter()
            .position(|pane| pane.id == floating_id)
        else {
            return false;
        };
        let floating = self.floating.remove(index);
        let target = self.focused_group.clone();
        let Some(group) = self.root.find_group_mut(&target) else {
            self.floating.insert(index, floating);
            return false;
        };
        group.tabs.push(floating.tab);
        group.active = group.tabs.len() - 1;
        true
    }

    pub fn bring_floating_to_front(&mut self, floating_id: &str) -> bool {
        let next_z = self
            .floating
            .iter()
            .map(|pane| pane.z_index)
            .max()
            .unwrap_or(0)
            + 1;
        let Some(pane) = self
            .floating
            .iter_mut()
            .find(|pane| pane.id == floating_id)
        else {
            return false;
        };
        pane.z_index = next_z;
        true
    }

    pub fn set_floating_bounds(&mut self, floating_id: &str, bounds: FloatingBounds) -> bool {
        let Some(pane) = self
            .floating
            .iter_mut()
            .find(|pane| pane.id == floating_id)
        else {
            return false;
        };
        pane.bounds = FloatingBounds {
            x: bounds.x.clamp(0.0, 0.95),
            y: bounds.y.clamp(0.0, 0.95),
            width: bounds.width.clamp(0.15, 1.0),
            height: bounds.height.clamp(0.15, 1.0),
        };
        true
    }

    pub fn normalize(&mut self) {
        self.root.normalize();
        if self.root.find_group(&self.focused_group).is_none() {
            if let Some(first) = self.group_ids().first() {
                self.focused_group.clone_from(first);
            }
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        let groups = self.group_ids();
        if groups.is_empty() {
            return Err("layout must contain a pane group".into());
        }
        let unique_groups = groups.iter().collect::<BTreeSet<_>>();
        if unique_groups.len() != groups.len() {
            return Err("pane group ids must be unique".into());
        }
        if !groups.iter().any(|group| group == &self.focused_group) {
            return Err("focused group must exist".into());
        }
        let panes = self.pane_ids();
        let unique_panes = panes.iter().collect::<BTreeSet<_>>();
        if unique_panes.len() != panes.len() {
            return Err("pane ids must be unique".into());
        }
        self.validate_node(&self.root)
    }

    fn validate_node(&self, node: &LayoutNode) -> Result<(), String> {
        match node {
            LayoutNode::Group(group) => {
                if group.tabs.is_empty() {
                    return Err(format!("pane group {} is empty", group.id));
                }
                if group.active >= group.tabs.len() {
                    return Err(format!("pane group {} has invalid active tab", group.id));
                }
                Ok(())
            }
            LayoutNode::Split {
                ratio,
                first,
                second,
                ..
            } => {
                if !(MIN_SPLIT_RATIO..=MAX_SPLIT_RATIO).contains(ratio) {
                    return Err("split ratio is outside supported bounds".into());
                }
                self.validate_node(first)?;
                self.validate_node(second)
            }
        }
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(value: &str) -> Result<Self, serde_json::Error> {
        let mut model: Self = serde_json::from_str(value)?;
        model.normalize();
        Ok(model)
    }
}

trait DefaultNode {
    fn unwrap_or_default_node(self) -> LayoutNode;
}

impl DefaultNode for Option<LayoutNode> {
    fn unwrap_or_default_node(self) -> LayoutNode {
        self.unwrap_or_else(|| LayoutModel::default().root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_is_valid_and_workspace_owned() {
        let layout = LayoutModel::default();
        assert!(layout.validate().is_ok());
        assert_eq!(
            layout.focused_group().unwrap().active_tab().unwrap().kind,
            PaneKind::Workspace
        );
    }

    #[test]
    fn split_tabs_resize_reorder_and_collapse_are_deterministic() {
        let mut layout = LayoutModel::default();
        let tool_group = layout
            .split_focused(SplitAxis::Horizontal, PaneKind::Files)
            .unwrap();
        layout.add_tab(PaneKind::Terminal);
        assert!(layout.cycle_tab(true));
        assert!(layout.reorder_active_tab(1));
        assert!(layout.activate_tab(&tool_group, 0));
        let split_id = match &layout.root {
            LayoutNode::Split { id, .. } => id.clone(),
            LayoutNode::Group(_) => panic!("expected split"),
        };
        assert!(layout.resize_split(&split_id, 0.99));
        if let LayoutNode::Split { ratio, .. } = &layout.root {
            assert_eq!(*ratio, MAX_SPLIT_RATIO);
        }
        assert!(layout.close_active_tab());
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn floating_panes_keep_identity_bounds_and_z_order() {
        let mut layout = LayoutModel::default();
        layout.add_tab(PaneKind::Review);
        let floating_id = layout.float_active().unwrap();
        assert_eq!(layout.floating.len(), 1);
        assert!(layout.set_floating_bounds(
            &floating_id,
            FloatingBounds {
                x: -1.0,
                y: 2.0,
                width: 0.01,
                height: 4.0,
            },
        ));
        let bounds = layout.floating[0].bounds;
        assert_eq!(bounds.x, 0.0);
        assert_eq!(bounds.y, 0.95);
        assert_eq!(bounds.width, 0.15);
        assert_eq!(bounds.height, 1.0);
        assert!(layout.bring_floating_to_front(&floating_id));
        assert!(layout.dock_floating(&floating_id));
        assert!(layout.floating.is_empty());
        assert!(layout.validate().is_ok());
    }

    #[test]
    fn layout_round_trips_for_persistence() {
        let mut layout = LayoutModel::default();
        layout.split_focused(SplitAxis::Vertical, PaneKind::Terminal);
        layout.add_tab(PaneKind::Review);
        let encoded = layout.to_json().unwrap();
        let decoded = LayoutModel::from_json(&encoded).unwrap();
        assert_eq!(decoded, layout);
        assert!(decoded.validate().is_ok());
        assert_eq!(LAYOUT_STORAGE_KEY, "hermes.desktop.layoutTree.v3");
    }

    #[test]
    fn workspace_cannot_be_closed_or_floated_when_it_is_the_last_pane() {
        let mut layout = LayoutModel::default();
        assert!(!layout.close_active_tab());
        assert!(layout.float_active().is_none());
        assert!(layout.validate().is_ok());
    }
}
