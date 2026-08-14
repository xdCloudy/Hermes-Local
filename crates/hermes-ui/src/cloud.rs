use dioxus::prelude::*;

#[cfg(test)]
const MAX_CLOUD_LABEL_BYTES: usize = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudOrg {
    pub id: String,
    pub slug: Option<String>,
    pub name: String,
    pub is_personal: bool,
    pub role: String,
}

impl CloudOrg {
    pub fn selection_key(&self) -> String {
        self.slug.clone().unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudAgent {
    pub id: String,
    pub name: String,
    pub status: String,
    pub dashboard_url: Option<String>,
    pub dashboard_gateway_state: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudState {
    pub portal_base_url: String,
    pub signed_in: bool,
    pub loading: bool,
    pub orgs: Vec<CloudOrg>,
    pub selected_org: Option<String>,
    pub agents: Vec<CloudAgent>,
    pub connected_url: Option<String>,
    pub message: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CloudConnectRequest {
    pub profile: Option<String>,
    pub org: Option<String>,
    pub agent: CloudAgent,
}

#[derive(Clone)]
pub struct CloudActions {
    pub state: Signal<CloudState>,
    pub login: Callback<()>,
    pub logout: Callback<()>,
    pub discover: Callback<Option<String>>,
    pub connect: Callback<CloudConnectRequest>,
}

#[cfg(test)]
pub(crate) fn bounded_cloud_label(value: &str) -> String {
    value
        .chars()
        .take(MAX_CLOUD_LABEL_BYTES)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::{CloudOrg, MAX_CLOUD_LABEL_BYTES, bounded_cloud_label};

    #[test]
    fn org_selection_prefers_slug_and_falls_back_to_id() {
        let with_slug = CloudOrg {
            id: "org-id".into(),
            slug: Some("team".into()),
            ..CloudOrg::default()
        };
        assert_eq!(with_slug.selection_key(), "team");
        let without_slug = CloudOrg {
            id: "org-id".into(),
            ..CloudOrg::default()
        };
        assert_eq!(without_slug.selection_key(), "org-id");
    }

    #[test]
    fn labels_are_bounded_before_rendering() {
        let value = "x".repeat(MAX_CLOUD_LABEL_BYTES + 32);
        assert_eq!(bounded_cloud_label(&value).len(), MAX_CLOUD_LABEL_BYTES);
    }
}
