use dioxus::prelude::*;

pub fn install_script() -> &'static str {
    r#"(() => {
      if(window.__hermesShellFocusGuardInstalled) return;
      window.__hermesShellFocusGuardInstalled=true;
      const contextOf=(el)=>{
        if(!el) return 'global';
        if(el.closest?.('.shell-overlay')) return 'command';
        if(el.closest?.('[role="dialog"],.modal,.dialog')) return 'dialog';
        if(el.closest?.('.xterm,.terminal,[data-terminal]')) return 'terminal';
        if(el.closest?.('.monaco-editor,.cm-editor,[data-editor]')) return 'editor';
        if(el.closest?.('textarea,input,[contenteditable="true"],.composer')) return 'composer';
        return 'global';
      };
      document.addEventListener('keydown',(event)=>{
        const ctx=contextOf(document.activeElement);
        const primary=event.ctrlKey||event.metaKey;
        const key=(event.key||'').toLowerCase();
        let owned=false;
        if((ctx==='editor'||ctx==='composer')&&primary&&['f','p','k','w','t','g'].includes(key)) owned=true;
        if(ctx==='terminal'&&event.ctrlKey&&['c','v','x','z','a','f','g'].includes(key)) owned=true;
        if(owned) event.stopPropagation();
      },true);
    })()"#
}

#[component]
pub fn FocusGuard(children: Element) -> Element {
    use_effect(move || {
        spawn(async move {
            let _ = document::eval(install_script()).await;
        });
    });
    rsx! { {children} }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_guard_covers_editor_composer_and_terminal_focus_collisions() {
        let script = install_script();
        assert!(script.contains("monaco-editor"));
        assert!(script.contains("contenteditable"));
        assert!(script.contains("xterm"));
        assert!(script.contains("stopPropagation"));
        assert!(script.contains("__hermesShellFocusGuardInstalled"));
    }
}
