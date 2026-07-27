# Hermes Launcher design specification

Status: accepted implementation reference  
Dark concept: `hermes-launcher-home-dark-concept.png`  
Light concept: `hermes-launcher-home-light-concept.png`

The concepts are the production visual reference for the primary control-centre
surface. They extend Hermes Desktop's existing flat, token-driven design system;
they do not replace the official Chat, overlay, terminal, file-preview, project,
or settings primitives.

## Visual direction

- Theme paradigm: compact desktop developer tool, not a marketing surface.
- Background character: graphite in dark mode; true white in light mode.
- Typography: existing Hermes Desktop UI sans stack for chrome and content;
  existing monospaced stack for ports, PIDs, timings, and resource values.
- Density: compact rows with deliberate whitespace; no nested cards.
- Containers: navigation rail, open status band, table/list surface, operational
  summary, activity region, and status bar.
- Signature motifs: restrained warm-gold active/action line, semantic status dot
  plus text, thin neutral hairlines, flat rows.
- Motion: functional state transitions around 100 ms; no layout animation;
  respect `prefers-reduced-motion`.

## Color lock

Use the official Desktop variables and add semantic aliases only when an
existing token cannot express the concept. Components must not contain raw
colors.

| Role | Dark reference | Light reference | Implementation source |
| --- | --- | --- | --- |
| Window | cool near-black graphite | true white | existing `--ui-bg-primary` |
| Secondary surface | slightly lighter graphite | cool light gray | existing `--ui-bg-secondary` |
| Primary text | near-white | graphite | `--ui-text-primary` |
| Secondary text | cool gray | slate | `--ui-text-secondary` |
| Hairline | subdued neutral | pale neutral | `--ui-stroke-tertiary` |
| Hermes accent | restrained warm gold | restrained warm gold | `--theme-primary` / `--ui-accent` |
| Healthy | green plus text | green plus text | semantic success token |
| Degraded | amber plus text | amber plus text | semantic warning token |
| Offline/error | red plus text | red plus text | semantic destructive token |

The light reference's top-right theme control still reads `Dark` because the
variant preserved visible copy during image editing. The implementation must
show the actual selected theme; this is an intentional state correction, not a
layout deviation.

## Typography

- Product name: 16 px, 600.
- Page title: 24 px, 600.
- Section title: 14 px, 600.
- Navigation and table body: 14 px, 450–500.
- Utility labels: 12–13 px, 500.
- Status/resource values: 13 px; monospaced where numeric.
- Controls: explicitly use existing `Button`, `Select`, and title-bar type
  rules; no inherited browser-default text sizing.

## Geometry and spacing

- Window reference: approximately 1920 × 1200 at 100% scaling.
- Navigation rail: approximately 252 px plus existing resizable-shell rules.
- Main gutters: use official `PAGE_INSET_X`; no page-local hardcoded gutters.
- Row height: 52–56 px for service and summary rows.
- Radius: existing Hermes 4–6 px control radius only; large content regions
  remain flat.
- Dividers: a single `--ui-stroke-tertiary` hairline between materially
  separate rows or regions.
- Primary content split: service/activity area about 64%; operational summary
  about 36%, collapsing to a single column at narrow desktop widths.

## Visible copy inventory

No extra above-the-fold copy is allowed without a functional requirement.

Navigation, in order:

1. Home
2. Chat
3. TUI
4. Web Dashboard
5. Services
6. Model
7. Tasks
8. Tools
9. Skills
10. Memory
11. Sessions
12. Projects
13. Logs
14. Benchmarks
15. Security
16. Settings
17. About

Home controls and state:

- Home
- Profile
- Daily
- Start all
- Restart
- Stop
- Stack offline
- Local only · 127.0.0.1
- Safe Recovery

Service table:

- Services
- Service
- State
- PID / Port
- Health
- Uptime
- CPU
- RAM
- VRAM
- Model server
- Hermes backend
- TUI gateway
- Web dashboard
- Browser
- Voice

Operational summary:

- Current profile
- Context limit
- Generation tok/s
- Prompt tok/s
- VRAM
- RAM
- CPU
- GPU
- Active session
- Active task
- Next scheduled task
- Recent errors
- Awaiting benchmark

Activity and status:

- Recent activity
- No activity yet. Start services to see logs and events.
- All systems offline

## Component inventory

- Existing Desktop title bar and window controls.
- `LauncherNavigation`: one flat rail using the existing icon and shell
  primitives; selected state has gold line, icon, text, and accessible focus.
- `StackActions`: existing `Button` variants; Start all is primary, Restart and
  Stop are quiet/destructive according to current state.
- `LocalOnlyStatus`: open status band, semantic dot plus text; not a badge.
- `ServiceTable`: virtualisation-ready table/list; rows subscribe only to their
  service's coarse status atom.
- `OperationalSummary`: definition-list rows; values are real snapshots or
  explicit unavailable states.
- `ActivityLogPreview`: bounded recent lifecycle events with a link to Logs;
  the empty state is the existing `EmptyState` treatment.
- Existing profile/theme selects and `Safe Recovery` action.
- Existing status bar surface, extended with launcher version and aggregate
  service state.

## Icon inventory

Use the curated aliases from `src/lib/icons.ts`; add an alias there only when
the exact semantic icon is missing. Do not import another icon package.

| Surface | Metaphor |
| --- | --- |
| Home | house |
| Chat | messages |
| TUI | terminal |
| Web Dashboard | browser/dashboard |
| Services | settings/gears |
| Model | neural/hex model |
| Tasks | briefcase/checklist |
| Tools | crossed tools |
| Skills | shield/spark |
| Memory | stacked layers |
| Sessions | conversation history |
| Projects | folder |
| Logs | clipboard/list |
| Benchmarks | bar chart |
| Security | shield |
| Settings | gear |
| About | information |
| Service state | semantic dot plus state text |

Every icon-only button uses the official `Tip` component and matching
`aria-label`. The concepts' icon strokes are approximately 1.5 px; preserve the
existing Tabler/Codicon vocabulary and optical sizes.

## Responsive and accessibility contract

- At narrow desktop widths, collapse the operational summary below the service
  table; do not transform rows into cards.
- The navigation rail may enter the official compact icon mode, preserving
  tooltips and keyboard order.
- No horizontal content clipping at 1024 × 720.
- Every status uses icon/dot, text, and accessible name; color is not the only
  signal.
- Keyboard focus follows the existing shell. Terminal and editor surfaces own
  their keys while focused.
- `Esc` performs one topmost cancellation only.
- Respect Windows scaling at 100%, 125%, 150%, and 200%.
- Respect reduced motion and maintain WCAG AA text contrast.

## Data integrity contract

- Never synthesize resource values, throughput, PIDs, ports, health, uptime,
  active work, or errors.
- Use an em dash, `Offline`, `Unavailable`, or `Awaiting benchmark` until a
  structured backend or supervisor source provides a value.
- A listening port alone does not mean healthy; use service-specific health
  probes.
- Background events may update rows and status indicators but must not navigate,
  open panes, or steal focus.

