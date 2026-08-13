#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FocusContext {
    Global,
    Pane,
    Composer,
    Editor,
    Terminal,
    Dialog,
    CommandOverlay,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyChord {
    pub key: String,
    pub primary: bool,
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
}

impl KeyChord {
    pub fn new(key: impl Into<String>, primary: bool, control: bool, shift: bool, alt: bool) -> Self {
        Self {
            key: key.into().to_ascii_lowercase(),
            primary,
            control,
            shift,
            alt,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShortcutAction {
    CommandPalette,
    QuickOpen,
    CommandCentre,
    Settings,
    ToggleSidebar,
    ToggleRightRail,
    ToggleStatusbar,
    Find,
    FindNext,
    FindPrevious,
    Review,
    Terminal,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SelectPaneTab(usize),
    NextPaneTab,
    PreviousPaneTab,
    ClosePaneTab,
    NewPaneTab,
    ReopenPaneTab,
    SplitHorizontal,
    SplitVertical,
    FloatPane,
    CloseDialog,
    ComposerSend,
    ComposerNewline,
    SurfaceOwned,
}

fn global_shortcut(chord: &KeyChord) -> Option<ShortcutAction> {
    let key = chord.key.as_str();
    if chord.primary && !chord.shift && !chord.alt {
        return match key {
            "k" => Some(ShortcutAction::CommandPalette),
            "p" => Some(ShortcutAction::QuickOpen),
            "." => Some(ShortcutAction::CommandCentre),
            "," => Some(ShortcutAction::Settings),
            "b" => Some(ShortcutAction::ToggleSidebar),
            "j" => Some(ShortcutAction::ToggleRightRail),
            "f" => Some(ShortcutAction::Find),
            "g" => Some(ShortcutAction::Review),
            "0" => Some(ShortcutAction::ZoomReset),
            "+" | "=" => Some(ShortcutAction::ZoomIn),
            "-" => Some(ShortcutAction::ZoomOut),
            _ => None,
        };
    }
    if chord.primary && chord.shift && !chord.alt {
        return match key {
            "s" => Some(ShortcutAction::ToggleStatusbar),
            "[" => Some(ShortcutAction::PreviousPaneTab),
            "]" => Some(ShortcutAction::NextPaneTab),
            "\\" => Some(ShortcutAction::SplitVertical),
            "f" => Some(ShortcutAction::FloatPane),
            _ => None,
        };
    }
    if chord.control && !chord.shift && !chord.alt && key == "`" {
        return Some(ShortcutAction::Terminal);
    }
    None
}

pub fn resolve_shortcut(context: FocusContext, chord: &KeyChord) -> Option<ShortcutAction> {
    let key = chord.key.as_str();

    if key == "escape" && matches!(context, FocusContext::Dialog | FocusContext::CommandOverlay) {
        return Some(ShortcutAction::CloseDialog);
    }

    // Text-editing surfaces own the chords which would otherwise destroy or
    // navigate away from local editing state. This is the collision boundary
    // the Electron registry enforced through focus scopes.
    if matches!(context, FocusContext::Editor | FocusContext::Composer) {
        if chord.primary && matches!(key, "f" | "p" | "k" | "w" | "t" | "g") {
            return Some(ShortcutAction::SurfaceOwned);
        }
        if context == FocusContext::Composer && key == "enter" {
            return Some(if chord.shift {
                ShortcutAction::ComposerNewline
            } else if chord.primary {
                ShortcutAction::ComposerSend
            } else {
                ShortcutAction::SurfaceOwned
            });
        }
    }

    // Terminal owns conventional control sequences and text/navigation keys,
    // but the dedicated terminal visibility chord remains global.
    if context == FocusContext::Terminal
        && chord.control
        && !chord.primary
        && matches!(key, "c" | "v" | "x" | "z" | "a" | "f" | "g")
    {
        return Some(ShortcutAction::SurfaceOwned);
    }

    if matches!(context, FocusContext::Dialog | FocusContext::CommandOverlay) {
        return None;
    }

    if let Some(action) = global_shortcut(chord) {
        return Some(action);
    }

    if chord.primary && !chord.shift && !chord.alt {
        if let Ok(number) = key.parse::<usize>() {
            if (1..=9).contains(&number) {
                return Some(ShortcutAction::SelectPaneTab(number - 1));
            }
        }
        return match key {
            "w" => Some(ShortcutAction::ClosePaneTab),
            "t" => Some(ShortcutAction::NewPaneTab),
            _ => None,
        };
    }

    if chord.primary && chord.shift && !chord.alt && key == "t" {
        return Some(ShortcutAction::ReopenPaneTab);
    }

    if chord.control && key == "tab" {
        return Some(if chord.shift {
            ShortcutAction::PreviousPaneTab
        } else {
            ShortcutAction::NextPaneTab
        });
    }

    if chord.primary && chord.alt && !chord.shift {
        return match key {
            "h" => Some(ShortcutAction::SplitHorizontal),
            "v" => Some(ShortcutAction::SplitVertical),
            _ => None,
        };
    }

    None
}

pub fn context_from_dom_hint(hint: &str) -> FocusContext {
    match hint.trim().to_ascii_lowercase().as_str() {
        "composer" | "textarea" | "input" => FocusContext::Composer,
        "editor" | "monaco" | "codemirror" => FocusContext::Editor,
        "terminal" | "xterm" => FocusContext::Terminal,
        "dialog" | "modal" => FocusContext::Dialog,
        "command" | "palette" | "command-centre" => FocusContext::CommandOverlay,
        "pane" | "tablist" => FocusContext::Pane,
        _ => FocusContext::Global,
    }
}

pub const fn dom_focus_probe_script() -> &'static str {
    r#"(() => {
      const el = document.activeElement;
      if (!el) return 'global';
      if (el.closest?.('.shell-overlay')) return 'command';
      if (el.closest?.('[role="dialog"],.modal,.dialog')) return 'dialog';
      if (el.closest?.('.xterm,.terminal,[data-terminal]')) return 'terminal';
      if (el.closest?.('.monaco-editor,.cm-editor,[data-editor]')) return 'editor';
      if (el.closest?.('textarea,input,[contenteditable="true"],.composer')) return 'composer';
      if (el.closest?.('[role="tablist"],[data-tree-group]')) return 'pane';
      return 'global';
    })()"#
}

#[cfg(test)]
mod tests {
    use super::*;

    fn primary(key: &str) -> KeyChord {
        KeyChord::new(key, true, true, false, false)
    }

    #[test]
    fn global_inventory_routes_major_shell_chords() {
        assert_eq!(
            resolve_shortcut(FocusContext::Global, &primary("k")),
            Some(ShortcutAction::CommandPalette)
        );
        assert_eq!(
            resolve_shortcut(FocusContext::Global, &primary(",")),
            Some(ShortcutAction::Settings)
        );
        assert_eq!(
            resolve_shortcut(FocusContext::Global, &primary("1")),
            Some(ShortcutAction::SelectPaneTab(0))
        );
        assert_eq!(
            resolve_shortcut(
                FocusContext::Pane,
                &KeyChord::new("tab", false, true, true, false)
            ),
            Some(ShortcutAction::PreviousPaneTab)
        );
    }

    #[test]
    fn editor_and_composer_keep_destructive_collisions() {
        for context in [FocusContext::Editor, FocusContext::Composer] {
            for key in ["f", "p", "k", "w", "t", "g"] {
                assert_eq!(
                    resolve_shortcut(context, &primary(key)),
                    Some(ShortcutAction::SurfaceOwned),
                    "{context:?} must own {key}"
                );
            }
        }
        assert_eq!(
            resolve_shortcut(FocusContext::Composer, &primary("enter")),
            Some(ShortcutAction::ComposerSend)
        );
        assert_eq!(
            resolve_shortcut(
                FocusContext::Composer,
                &KeyChord::new("enter", false, false, true, false)
            ),
            Some(ShortcutAction::ComposerNewline)
        );
    }

    #[test]
    fn terminal_keeps_control_sequences_but_shell_toggle_survives() {
        assert_eq!(
            resolve_shortcut(
                FocusContext::Terminal,
                &KeyChord::new("c", false, true, false, false)
            ),
            Some(ShortcutAction::SurfaceOwned)
        );
        assert_eq!(
            resolve_shortcut(
                FocusContext::Terminal,
                &KeyChord::new("`", false, true, false, false)
            ),
            Some(ShortcutAction::Terminal)
        );
    }

    #[test]
    fn modal_scope_blocks_background_navigation() {
        assert_eq!(
            resolve_shortcut(FocusContext::Dialog, &primary("1")),
            None
        );
        assert_eq!(
            resolve_shortcut(
                FocusContext::Dialog,
                &KeyChord::new("escape", false, false, false, false)
            ),
            Some(ShortcutAction::CloseDialog)
        );
    }

    #[test]
    fn dom_focus_hints_cover_every_interactive_scope() {
        assert_eq!(context_from_dom_hint("monaco"), FocusContext::Editor);
        assert_eq!(context_from_dom_hint("xterm"), FocusContext::Terminal);
        assert_eq!(context_from_dom_hint("textarea"), FocusContext::Composer);
        assert_eq!(context_from_dom_hint("modal"), FocusContext::Dialog);
        assert_eq!(context_from_dom_hint("tablist"), FocusContext::Pane);
        assert!(dom_focus_probe_script().contains("document.activeElement"));
    }
}
