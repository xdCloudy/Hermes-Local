from pathlib import Path

path = Path("docs/DIOXUS_MIGRATION_MATRIX.md")
text = path.read_text(encoding="utf-8")

replacements = [
    ("| A1 Designed | 15 | Largest remaining implementation backlog. |", "| A1 Designed | 14 | Largest remaining implementation backlog. |"),
    ("| A4 Auto-verified | 107 | Automated slice is green; human/live acceptance still required. |", "| A4 Auto-verified | 108 | Automated slice is green; human/live acceptance still required. |"),
    ("At least-service coverage is **111/127 capabilities (87.4%)**.", "At least-service coverage is **112/127 capabilities (88.2%)**."),
    ("- **Hermes Cloud:** discovery/org selection/agent connection is not yet ported\n  (`CN-06`).\n", ""),
    ("1. Port Hermes Cloud discovery/org/agent connection and finish complete Gateway\n   re-home behavior.", "1. Finish complete Gateway cross-mode re-home behavior now that Cloud discovery,\n   org selection and Agent connection are auto-verified."),
    ("| CN-06 | Hermes Cloud portal discovery, org selection and agent connection | future `AuthService`/`ConnectionService` | Settings → Gateway | A1 Designed | OG Cloud discovery/org/agent cascade remains unported. | Sign in to Cloud, handle zero/one/multiple orgs, discover agents, connect/switch/reconnect and separate portal auth from Agent connectivity. | ⬜ |", "| CN-06 | Hermes Cloud portal discovery, org selection and agent connection | `ConnectionService` + Desktop Cloud authority | Settings → Gateway | A4 Auto-verified | PR #223 ports the persistent Cloud portal WebView, zero/one/multiple-org discovery, bounded agent selection/login, allowlisted gateway-cookie capture into native keyring storage, one-time WS-ticket reuse, Cloud profile persistence and reconnect. Exact-head Rust/WASM, native-client, packaging, install and footprint gates pass; live portal/Agent diversity remains A5 evidence. | Sign in to Cloud, handle zero/one/multiple orgs, discover agents, connect/switch/reconnect and separate portal auth from Agent connectivity. | ⬜ |"),
]

for old, new in replacements:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one match for {old!r}; found {count}")
    text = text.replace(old, new, 1)

marker = "\n## Roadmap waves\n"
if text.count(marker) != 1:
    raise SystemExit("roadmap insertion marker is not unique")

evidence = """

PR #223 exact head `0629c2d` completes the machine-verifiable Hermes Cloud
connection slice. Desktop authority owns a persistent portal WebView for sign-in,
zero/one/multiple-org discovery and bounded Agent selection/login, captures only
allowlisted gateway cookies into the native keyring boundary, and reuses the
existing one-time WS-ticket connection path for Cloud persistence/reconnect.
Dioxus Rust validation `31859325687`, Windows packaging `31859325667`, install
lifecycle `31859325702`, footprint `31859325670` and native-client boundary
`31859325653` all passed for that exact head; SSH interoperability was the
expected skip. It merged as `d733beae`, so `CN-06` is A4. Live Cloud account,
multi-org, Agent-switching and portal-auth separation comparison remain A5
evidence.
"""
text = text.replace(marker, evidence + marker, 1)
path.write_text(text, encoding="utf-8")
