use crate::{
    shell_accessibility::{REQUIRED_ACCESSIBILITY_SIGNALS, REQUIRED_INTERACTIVE_ROLES},
    shell_i18n::{Locale, Message, translate},
    shell_keymap::{FocusContext, KeyChord, ShortcutAction, resolve_shortcut},
    shell_layout::{LayoutModel, PaneKind, SplitAxis},
    shell_window_contract::{TITLEBAR_HEIGHT_PX, WINDOW_MIN_HEIGHT, WINDOW_MIN_WIDTH},
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShellParityReport {
    pub supported_locales: usize,
    pub translated_messages: usize,
    pub accessibility_roles: usize,
    pub accessibility_signals: usize,
    pub pane_groups: usize,
    pub pane_count: usize,
}

pub fn validate_contracts() -> Result<ShellParityReport, String> {
    if TITLEBAR_HEIGHT_PX != 34 || WINDOW_MIN_WIDTH != 400 || WINDOW_MIN_HEIGHT != 620 {
        return Err("window/titlebar contract drifted".into());
    }

    for locale in Locale::ALL {
        for message in Message::ALL {
            if translate(locale, message).trim().is_empty() {
                return Err(format!("missing {locale:?} translation for {message:?}"));
            }
        }
    }

    let mut layout = LayoutModel::default();
    layout
        .split_focused(SplitAxis::Horizontal, PaneKind::Files)
        .ok_or_else(|| "horizontal split failed".to_owned())?;
    layout.add_tab(PaneKind::Terminal);
    layout
        .split_focused(SplitAxis::Vertical, PaneKind::Review)
        .ok_or_else(|| "vertical split failed".to_owned())?;
    layout.validate()?;

    let close = resolve_shortcut(
        FocusContext::Pane,
        &KeyChord::new("w", true, true, false, false),
    );
    if close != Some(ShortcutAction::ClosePaneTab) {
        return Err("pane keymap contract drifted".into());
    }
    let editor = resolve_shortcut(
        FocusContext::Editor,
        &KeyChord::new("w", true, true, false, false),
    );
    if editor != Some(ShortcutAction::SurfaceOwned) {
        return Err("editor focus collision contract drifted".into());
    }

    Ok(ShellParityReport {
        supported_locales: Locale::ALL.len(),
        translated_messages: Message::ALL.len(),
        accessibility_roles: REQUIRED_INTERACTIVE_ROLES.len(),
        accessibility_signals: REQUIRED_ACCESSIBILITY_SIGNALS.len(),
        pane_groups: layout.group_ids().len(),
        pane_count: layout.pane_ids().len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integrated_shell_contract_is_self_consistent() {
        let report = validate_contracts().expect("shell parity contract");
        assert_eq!(report.supported_locales, 5);
        assert_eq!(report.translated_messages, 70);
        assert!(report.accessibility_roles >= 4);
        assert!(report.accessibility_signals >= 5);
        assert!(report.pane_groups >= 3);
        assert!(report.pane_count >= 4);
    }
}
