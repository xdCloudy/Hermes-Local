from pathlib import Path

p = Path('crates/hermes-ui/assets/app.css')
s = p.read_text(encoding='utf-8')
if '.composer-attachments {' not in s:
    s = s.rstrip() + '''

.composer-attachments {
  display: flex;
  min-width: 0;
  max-width: 100%;
  flex-wrap: wrap;
  gap: 4px;
  padding: 2px 1px;
}
.composer-card:has(.composer-attachments) { flex-wrap: wrap; }
.composer-card:has(.composer-attachments) textarea { min-width: 240px; }
.composer-attachment-chip {
  display: inline-flex;
  min-width: 0;
  max-width: 220px;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--stroke-2);
  border-radius: 6px;
  padding: 3px 4px;
  background: var(--input-bg);
}
.composer-attachment-preview,
.composer-attachment-icon {
  width: 28px;
  height: 28px;
  flex: 0 0 28px;
  border-radius: 5px;
}
.composer-attachment-preview { object-fit: cover; border: 1px solid var(--stroke-3); }
.composer-attachment-icon { display: grid; place-items: center; background: var(--hover); color: var(--text-3); }
.composer-attachment-copy { display: grid; min-width: 0; line-height: 1.15; }
.composer-attachment-copy strong,
.composer-attachment-copy small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.composer-attachment-copy strong { color: var(--text-1); font-size: .6875rem; font-weight: 600; }
.composer-attachment-copy small { margin-top: 2px; color: var(--text-4); font-size: .5625rem; }
.composer-attachment-remove {
  display: grid;
  width: 20px;
  height: 20px;
  flex: 0 0 20px;
  place-items: center;
  border: 0;
  border-radius: 4px;
  padding: 0;
  background: transparent;
  color: var(--text-4);
}
.composer-attachment-remove:hover,
.composer-attachment-remove:focus-visible { background: var(--hover); color: var(--text-1); }
.composer-attachment-remove .codicon { width: 12px; height: 12px; }
'''
p.write_text(s, encoding='utf-8')
print('attachment CSS applied')
