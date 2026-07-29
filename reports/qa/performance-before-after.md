# Performance before and after

Measurements were taken on the same Windows 11 workstation and checkout
family. The baseline build is a single pre-change observation; the post-change
value is the median of repeated direct-icon builds.

| Measurement | Before | After | Change |
|---|---:|---:|---:|
| Desktop build wall time | 6.137 s | 5.693 s median | -0.444 s / -7.2% |
| Final release-gate build | — | 5.532 s | Reference |
| Renderer modules transformed | 4,539 | 4,538 | -1 |
| Tabler barrel warning | 6,149 exports | Absent | Removed |
| Malformed CSS warning | Present | Absent | Removed |
| Renderer CSS | — | 323.96 kB / 54.58 kB gzip | Reference |
| Renderer JS | — | 28,585.93 kB / 6,202.23 kB gzip | Reference |

The implementation replaced three top-level Tabler icon barrel imports with
direct ESM module imports. This was behavior-neutral, retained full type/UI
coverage, removed the high-export-count warning, and reduced measured build
time. The application still emits one intentional large-chunk warning; broad
code splitting was not introduced because it would be a higher-risk runtime
change without a measured end-user startup baseline.

Additional resource controls were implemented as reliability work rather than
claimed speedups:

- concurrent workstation snapshots share one in-flight request;
- completed native tasks are bounded at 50;
- renderer polling rejects stale generations and stops updates after unmount.

These have deterministic regression assertions, but no synthetic percentage is
claimed because the engagement did not obtain a controlled idle CPU/memory
profile.

Observed lifecycle timings:

| Operation | Final observed time |
|---|---:|
| Managed stop | 30.725 s |
| Managed start | 45.125 s |
| Managed restart | 72.079 s |
| Model-process recovery | 54.208 s |

Package size changed from the pre-existing 214,281,216-byte launcher to the
final 126,983,589-byte portable artifact (40.7% smaller). This is recorded as
an observed package-state difference, not attributed solely to the source
optimisation because the initial executable was not produced in the same
controlled packaging run.

Security auditing and vulnerability assessment were excluded from this QA
engagement.
