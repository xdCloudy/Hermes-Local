pub const REQUIRED_INTERACTIVE_ROLES: &[&str] = &["dialog", "tablist", "tab", "search"];
pub const REQUIRED_ACCESSIBILITY_SIGNALS: &[&str] = &[
    "aria-label",
    "aria-selected",
    "aria-live",
    "focus-visible",
    "prefers-reduced-motion",
];

pub fn audit_script() -> &'static str {
    r#"(() => {
      const violations=[];
      for(const button of document.querySelectorAll('button')) {
        const name=(button.getAttribute('aria-label')||button.getAttribute('title')||button.textContent||'').trim();
        if(!name) violations.push({kind:'button-name',element:button});
      }
      for(const control of document.querySelectorAll('input,select,textarea')) {
        const name=(control.getAttribute('aria-label')||control.getAttribute('title')||'').trim();
        if(!name && !control.closest('label')) violations.push({kind:'control-name',element:control});
      }
      for(const tab of document.querySelectorAll('[role="tab"]')) {
        if(!tab.hasAttribute('aria-selected')) violations.push({kind:'tab-state',element:tab});
      }
      const ids=new Set();
      for(const el of document.querySelectorAll('[id]')) {
        if(ids.has(el.id)) violations.push({kind:'duplicate-id',element:el}); else ids.add(el.id);
      }
      document.documentElement.dataset.shellA11yViolations=String(violations.length);
      if(violations.length) console.warn('[shell-a11y]',violations.map(v=>v.kind));
      return violations.length;
    })()"#
}

pub fn focus_restore_script() -> &'static str {
    r#"(() => {
      const key='__hermesShellPreviousFocus';
      window[key]=document.activeElement;
      return true;
    })()"#
}

pub fn restore_focus_script() -> &'static str {
    r#"(() => {
      const previous=window.__hermesShellPreviousFocus;
      if(previous && previous.isConnected && typeof previous.focus==='function') previous.focus();
      window.__hermesShellPreviousFocus=null;
      return true;
    })()"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessibility_contract_covers_names_tab_state_and_duplicate_ids() {
        let script = audit_script();
        assert!(script.contains("button-name"));
        assert!(script.contains("control-name"));
        assert!(script.contains("tab-state"));
        assert!(script.contains("duplicate-id"));
        assert!(script.contains("shellA11yViolations"));
    }

    #[test]
    fn focus_restore_contract_is_symmetric() {
        assert!(focus_restore_script().contains("document.activeElement"));
        assert!(restore_focus_script().contains("previous.focus"));
    }

    #[test]
    fn semantic_contract_tracks_required_shell_roles_and_motion_signals() {
        assert!(REQUIRED_INTERACTIVE_ROLES.contains(&"dialog"));
        assert!(REQUIRED_INTERACTIVE_ROLES.contains(&"tab"));
        assert!(REQUIRED_ACCESSIBILITY_SIGNALS.contains(&"aria-live"));
        assert!(REQUIRED_ACCESSIBILITY_SIGNALS.contains(&"prefers-reduced-motion"));
    }
}
