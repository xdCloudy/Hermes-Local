from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one replacement, found {count}: {old[:100]!r}")
    target.write_text(text.replace(old, new), encoding="utf-8", newline="\n")


replace_once(
    "crates/hermes-ui/src/lib.rs",
    "collections::{BTreeMap, BTreeSet},",
    "collections::{BTreeMap, BTreeSet, VecDeque},",
)
replace_once(
    "crates/hermes-ui/src/lib.rs",
    """#[derive(Clone)]
pub struct WindowActions {
    pub drag: Callback<()>,
    pub minimize: Callback<()>,
    pub toggle_maximized: Callback<()>,
    pub close: Callback<()>,
}
""",
    """#[derive(Clone)]
pub struct WindowActions {
    pub drag: Callback<()>,
    pub minimize: Callback<()>,
    pub toggle_maximized: Callback<()>,
    pub close: Callback<()>,
}

/// A bounded, typed activation delivered by a Desktop host. The shared UI owns
/// navigation/composer effects; native hosts can only enqueue these explicit intents.
#[derive(Clone, Debug, PartialEq)]
pub enum ExternalActivation {
    Navigate(Route),
    Blueprint {
        name: String,
        params: BTreeMap<String, String>,
    },
}

pub fn use_external_activation_queue() -> Signal<VecDeque<ExternalActivation>> {
    use_root_context(|| Signal::new(VecDeque::new()))
}
""",
)
replace_once(
    "crates/hermes-ui/src/lib.rs",
    """    let mut projects_error = use_signal(|| None::<String>);
    let projects_refresh = use_signal(|| 0_u64);
""",
    """    let mut projects_error = use_signal(|| None::<String>);
    let projects_refresh = use_signal(|| 0_u64);
    let mut external_activations = use_external_activation_queue();
    let navigator = use_navigator();
    use_effect(move || {
        let next = external_activations.read().front().cloned();
        match next {
            Some(ExternalActivation::Navigate(route)) => {
                external_activations.write().pop_front();
                navigator.replace(route);
            }
            Some(ExternalActivation::Blueprint { .. }) => {
                navigator.replace(Route::Chat {});
            }
            None => {}
        }
    });
""",
)

replace_once(
    "crates/hermes-ui/src/chat/legacy.rs",
    "use dioxus::prelude::*;",
    "use std::collections::BTreeMap;\n\nuse dioxus::prelude::*;",
)
replace_once(
    "crates/hermes-ui/src/chat/legacy.rs",
    """use super::{
    Codicon, ErrorState, LoadingState, ProjectPicker, ProjectUiState, Route, SettingsUiState,
};""",
    """use super::{
    Codicon, ErrorState, ExternalActivation, LoadingState, ProjectPicker, ProjectUiState, Route,
    SettingsUiState, use_external_activation_queue,
};""",
)
replace_once(
    "crates/hermes-ui/src/chat/legacy.rs",
    """fn mark_draft_changed(mut revision: Signal<u64>) {
    let next = revision().wrapping_add(1);
    revision.set(next);
}
""",
    """fn mark_draft_changed(mut revision: Signal<u64>) {
    let next = revision().wrapping_add(1);
    revision.set(next);
}

fn blueprint_command(name: &str, params: &BTreeMap<String, String>) -> String {
    let slots = params
        .iter()
        .map(|(key, value)| {
            let value = if value.chars().any(char::is_whitespace) {
                format!(\"\\\"{}\\\"\", value.replace('\\\"', \"\\\\\\\"\"))
            } else {
                value.clone()
            };
            format!(\"{key}={value}\")
        })
        .collect::<Vec<_>>()
        .join(\" \" );
    if slots.is_empty() {
        format!(\"/blueprint {name}\")
    } else {
        format!(\"/blueprint {name} {slots}\")
    }
}

fn insert_composer_block(existing: &str, block: &str) -> String {
    let existing = existing.trim_end();
    if existing.is_empty() {
        block.to_owned()
    } else {
        format!(\"{existing}\\n\\n{block}\")
    }
}
""",
)
replace_once(
    "crates/hermes-ui/src/chat/legacy.rs",
    """    let mut prompt = use_signal(String::new);
    let mut prompt_bound = use_signal(|| false);
    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);
""",
    """    let mut prompt = use_signal(String::new);
    let mut prompt_bound = use_signal(|| false);
    let mut composer_element = use_signal(|| None::<MountedData>);
    let mut external_activations = use_external_activation_queue();
    let mut attachments = use_signal(Vec::<SelectedAttachment>::new);
""",
)
replace_once(
    "crates/hermes-ui/src/chat/legacy.rs",
    """    });
    let mut submitting = use_signal(|| false);
""",
    """    });
    use_effect(move || {
        if !(chat_runtime.drafts_hydrated)() {
            return;
        }
        let next = external_activations.read().front().cloned();
        let Some(ExternalActivation::Blueprint { name, params }) = next else {
            return;
        };
        let command = blueprint_command(&name, &params);
        let existing = (chat_runtime.drafts)().text(NEW_CHAT_DRAFT_KEY);
        let value = insert_composer_block(&existing, &command);
        prompt.set(value.clone());
        prompt_bound.set(true);
        chat_runtime.drafts.write().edit(NEW_CHAT_DRAFT_KEY, value);
        mark_draft_changed(chat_runtime.draft_revision);
        external_activations.write().pop_front();
        if let Some(element) = composer_element() {
            spawn(async move {
                let _ = element.set_focus(true).await;
            });
        }
    });
    let mut submitting = use_signal(|| false);
""",
)
replace_once(
    "crates/hermes-ui/src/chat/legacy.rs",
    '                        aria_label: "Start a conversation",\n                        placeholder: "What are we building?",',
    '                        aria_label: "Start a conversation",\n                        onmounted: move |element| composer_element.set(Some(element.data())),\n                        placeholder: "What are we building?",',
)

replace_once("apps/desktop/src/main.rs", "mod deep_link;\n", "mod deep_link;\nmod deep_link_bridge;\n")
replace_once(
    "apps/desktop/src/main.rs",
    """use hermes_desktop::NativeApp;

fn desktop_root() -> Element {""",
    """use hermes_desktop::NativeApp;

#[derive(Clone)]
struct DesktopDataDir(PathBuf);

fn desktop_root() -> Element {""",
)
replace_once(
    "apps/desktop/src/main.rs",
    """        Some(Ok(())) => rsx! {
            clipboard_bridge::ClipboardBridge {
                subagent_bridge::SubagentBridge {
                    shell_focus_guard::FocusGuard {
                        shell_parity::ParityShellHost {
                            shell_interaction::ShellHost {
                                hermes_ui::App {}
                            }
                        }
                    }
                }
            }
        },""",
    """        Some(Ok(())) => rsx! {
            deep_link_bridge::DeepLinkBridge {
                clipboard_bridge::ClipboardBridge {
                    subagent_bridge::SubagentBridge {
                        shell_focus_guard::FocusGuard {
                            shell_parity::ParityShellHost {
                                shell_interaction::ShellHost {
                                    hermes_ui::App {}
                                }
                            }
                        }
                    }
                }
            }
        },""",
)
replace_once(
    "apps/desktop/src/main.rs",
    """    let data_dir = std::env::var_os("APPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Hermes Local");
    let _instance_guard = match shell_instance::InstanceGuard::acquire(&data_dir) {""",
    """    let data_dir = std::env::var_os("APPDATA")
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join("Hermes Local");
    let startup_deep_link = deep_link::extract_from_args(std::env::args_os());
    let _instance_guard = match shell_instance::InstanceGuard::acquire(&data_dir) {""",
)
replace_once(
    "apps/desktop/src/main.rs",
    """        Ok(None) => {
            eprintln!("Hermes Local is already running; refusing a second Desktop authority.");
            return;
        }""",
    """        Ok(None) => {
            if let Some(uri) = startup_deep_link.as_deref() {
                if let Err(error) = deep_link::enqueue(&data_dir, uri) {
                    eprintln!("Hermes Local could not forward the deep-link activation: {error}");
                }
            } else {
                eprintln!("Hermes Local is already running; refusing a second Desktop authority.");
            }
            return;
        }""",
)
replace_once(
    "apps/desktop/src/main.rs",
    """    if let Err(error) = deep_link::register() {
        eprintln!("Hermes Local protocol registration is unavailable: {error}");
    }
    let saved_window_state =""",
    """    if let Err(error) = deep_link::register() {
        eprintln!("Hermes Local protocol registration is unavailable: {error}");
    }
    if let Some(uri) = startup_deep_link.as_deref()
        && let Err(error) = deep_link::enqueue(&data_dir, uri)
    {
        eprintln!("Hermes Local could not queue the startup deep link: {error}");
    }
    let saved_window_state =""",
)
replace_once(
    "apps/desktop/src/main.rs",
    "ssh_service::install_ssh_probe(&mut native.services, data_dir);",
    "ssh_service::install_ssh_probe(&mut native.services, data_dir.clone());",
)
replace_once(
    "apps/desktop/src/main.rs",
    """        .with_cfg(config)
        .with_context(native.services)
        .launch(desktop_root);""",
    """        .with_cfg(config)
        .with_context(native.services)
        .with_context(DesktopDataDir(data_dir))
        .launch(desktop_root);""",
)

replace_once(
    "scripts/ci/rust_windows_package.py",
    'APP_ID = "F55A89C1-897E-4A89-9E07-11851CE65E51"\n',
    'APP_ID = "F55A89C1-897E-4A89-9E07-11851CE65E51"\nAPP_USER_MODEL_ID = "xdCloudy.HermesLocal"\n',
)
replace_once(
    "scripts/ci/rust_windows_package.py",
    'Name: "{{autoprograms}}\\\\Hermes Local"; Filename: "{{app}}\\\\{EXECUTABLE_NAME}"; WorkingDir: "{{app}}"',
    'Name: "{{autoprograms}}\\\\Hermes Local"; Filename: "{{app}}\\\\{EXECUTABLE_NAME}"; WorkingDir: "{{app}}"; AppUserModelID: "{APP_USER_MODEL_ID}"',
)
replace_once(
    "tests/test_rust_windows_package.py",
    '''            self.assertIn('Name: "{autoprograms}\\\\Hermes Local"', iss)\n            self.assertIn("OutputBaseFilename=Hermes-Local-Setup", iss)''',
    '''            self.assertIn('Name: "{autoprograms}\\\\Hermes Local"', iss)\n            self.assertIn('AppUserModelID: "xdCloudy.HermesLocal"', iss)\n            self.assertEqual(package.APP_USER_MODEL_ID, "xdCloudy.HermesLocal")\n            self.assertIn("OutputBaseFilename=Hermes-Local-Setup", iss)''',
)
