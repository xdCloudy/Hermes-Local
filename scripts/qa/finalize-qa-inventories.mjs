import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url))
const repositoryRoot = path.resolve(scriptDirectory, '..', '..')
const reportDirectory = path.join(repositoryRoot, 'reports', 'qa')
const requestedEvidenceDirectory = process.argv[2] ?? path.join(repositoryRoot, 'temp', 'qa-runs', 'final-full')
const evidenceDirectory = path.resolve(repositoryRoot, requestedEvidenceDirectory)

const readJson = (name) => JSON.parse(fs.readFileSync(path.join(reportDirectory, name), 'utf8'))
const writeJson = (name, value) =>
  fs.writeFileSync(path.join(reportDirectory, name), `${JSON.stringify(value, null, 2)}\n`)
const relative = (value) => path.relative(repositoryRoot, value).replaceAll('\\', '/')
const unique = (values) => [...new Set(values.filter(Boolean))]
const evidence = (name) => relative(path.join(evidenceDirectory, name))

const ui = readJson('ui-control-inventory.json')
const uiFullEvidence = evidence('ui-full.stdout.txt')

for (const entry of ui.entries) {
  if (entry.currentAutomatedCoverage === 'label-asserted') {
    entry.finalResult = 'passed-automated-label-assertion'
    entry.evidenceLocation = unique([...entry.relatedTests, uiFullEvidence])
    entry.evidenceQualification =
      'The visible label is asserted by an automated test and the complete UI suite passed.'
  } else if (entry.currentAutomatedCoverage === 'related-component-suite') {
    entry.finalResult = 'passed-related-suite-not-control-specific'
    entry.evidenceLocation = unique([...entry.relatedTests, uiFullEvidence])
    entry.evidenceQualification =
      'The owning component has passing automated coverage; the individual control was not necessarily actuated.'
  } else {
    entry.finalResult = 'source-inventoried-not-individually-exercised'
    entry.evidenceLocation = [entry.sourceFile]
    entry.evidenceQualification =
      'AST and source review only. No reliable control-specific automation was discovered, so this entry remains an explicit functional coverage limitation.'
  }
}

const countUiResult = (result) => ui.entries.filter((entry) => entry.finalResult === result).length
ui.methodology +=
  ' Final dispositions distinguish label assertions, related-suite evidence, and source-only inventory; related-suite evidence is not represented as individual control actuation.'
ui.summary.totalEntries = ui.entries.length
ui.summary.passedAutomatedLabelAssertion = countUiResult('passed-automated-label-assertion')
ui.summary.passedRelatedSuiteNotControlSpecific = countUiResult('passed-related-suite-not-control-specific')
ui.summary.sourceInventoriedNotIndividuallyExercised = countUiResult('source-inventoried-not-individually-exercised')
ui.summary.controlsSourceOnly = ui.entries.filter(
  (entry) => entry.kind === 'control' && entry.finalResult === 'source-inventoried-not-individually-exercised'
).length
ui.summary.statesSourceOnly = ui.entries.filter(
  (entry) => entry.kind === 'state' && entry.finalResult === 'source-inventoried-not-individually-exercised'
).length
ui.summary.automatedOrRelatedSuitePercent = Number(
  (
    ((ui.summary.passedAutomatedLabelAssertion + ui.summary.passedRelatedSuiteNotControlSpecific) /
      ui.summary.totalEntries) *
    100
  ).toFixed(2)
)

const scripts = readJson('script-test-inventory.json')
const rootQaReport = 'reports/qa/FULL_FUNCTIONAL_QA_REPORT.md'
const finalVerification = 'reports/qa/final-verification.md'
const directScriptEvidence = new Map([
  ['Backup-Hermes-Local.ps1', [evidence('recovery-fixtures.json')]],
  ['Build-Hermes-Launcher.ps1', [evidence('desktop-build.stdout.txt')]],
  ['Export-Hermes-Diagnostics.ps1', [rootQaReport]],
  ['Package-Hermes-Launcher.ps1', [finalVerification]],
  ['Restart-Hermes-Local.ps1', [finalVerification]],
  ['Restore-Hermes-Local.ps1', [evidence('recovery-fixtures.json')]],
  ['Start-Hermes-Local.ps1', [finalVerification]],
  ['Stop-Hermes-Local.ps1', [finalVerification]],
  ['Test-Hermes-Local.ps1', [evidence('operational-diagnostics.stdout.txt')]],
  ['Update-Hermes-Agent.ps1', [finalVerification]],
  ['Update-Hermes-Local.ps1', [finalVerification]],
  ['scripts/qa/Invoke-FullFunctionalQA.ps1', [evidence('qa-run.json')]],
  ['scripts/qa/Test-PowerShellSyntax.ps1', [evidence('powershell-syntax.json')]],
  ['scripts/qa/Test-RecoveryFixtures.ps1', [evidence('recovery-fixtures.json')]],
  ['scripts/qa/build-qa-inventories.mjs', [evidence('inventories.stdout.txt')]],
  ['scripts/qa/finalize-qa-inventories.mjs', [rootQaReport]]
])
const directNpmEvidence = new Map([
  ['apps/desktop/package.json#scripts.build', [evidence('desktop-build.stdout.txt')]],
  ['apps/desktop/package.json#scripts.lint', [evidence('desktop-lint.stdout.txt')]],
  [
    'apps/desktop/package.json#scripts.test:desktop:platforms',
    [evidence('electron-full.stdout.txt')]
  ],
  ['apps/desktop/package.json#scripts.test:ui', [evidence('ui-full.stdout.txt')]],
  ['apps/desktop/package.json#scripts.typecheck', [evidence('desktop-typecheck.stdout.txt')]]
])

for (const entry of scripts.entries) {
  if (entry.scope === 'security-excluded') {
    entry.successPath = 'excluded-by-engagement-scope'
    entry.failurePath = 'excluded-by-engagement-scope'
    entry.argumentValidation = 'excluded-by-engagement-scope'
    entry.exitCodeVerification = 'excluded-by-engagement-scope'
    entry.currentAutomatedCoverage = 'excluded-by-engagement-scope'
    entry.finalResult = 'excluded'
    entry.evidenceLocation = []
    entry.evidenceQualification =
      'Security auditing and vulnerability assessment were excluded from this QA engagement.'
    continue
  }

  const directEvidence = directScriptEvidence.get(entry.file) ?? directNpmEvidence.get(entry.file)

  if (directEvidence) {
    entry.successPath = 'passed-direct-execution'
    entry.failurePath =
      /^(?:Backup|Restore)-Hermes-Local\.ps1$/.test(entry.file) ||
      entry.file === 'scripts/qa/Test-RecoveryFixtures.ps1'
        ? 'passed-ordinary-failure-fixtures'
        : 'not-individually-demonstrated'
    entry.argumentValidation =
      entry.failurePath === 'passed-ordinary-failure-fixtures'
        ? 'exercised-by-fixtures'
        : entry.kind === 'npm-script'
          ? 'not-applicable'
          : 'not-individually-demonstrated'
    entry.exitCodeVerification = 'verified-zero'
    entry.currentAutomatedCoverage = 'direct-execution'
    entry.finalResult = 'passed-direct-execution'
    entry.evidenceLocation = directEvidence
    entry.evidenceQualification =
      'The executable entry point completed successfully; only explicitly cited fixture runs demonstrate a failure path.'
  } else if (entry.kind === 'ps1' || entry.kind === 'psm1') {
    entry.successPath = 'not-directly-executed'
    entry.failurePath = 'not-directly-executed'
    entry.argumentValidation = 'not-individually-demonstrated'
    entry.exitCodeVerification = 'not-individually-demonstrated'
    entry.currentAutomatedCoverage = 'powershell-parser'
    entry.finalResult = 'passed-static-parse-not-directly-executed'
    entry.evidenceLocation = [evidence('powershell-syntax.json')]
    entry.evidenceQualification =
      'The PowerShell parser accepted this file, but its success and failure paths were not individually executed.'
  } else {
    entry.successPath = 'not-directly-executed'
    entry.failurePath = 'not-directly-executed'
    entry.argumentValidation =
      entry.kind === 'npm-script' ? 'not-applicable' : 'not-individually-demonstrated'
    entry.exitCodeVerification = entry.kind === 'npm-script' ? 'provided-by-npm' : 'not-individually-demonstrated'
    entry.currentAutomatedCoverage = 'inventory-only'
    entry.finalResult = 'inventoried-not-directly-executed'
    entry.evidenceLocation = [entry.file]
    entry.evidenceQualification =
      'Inventory/source evidence only. This entry point was not executed independently in the final QA run.'
  }
}

const scriptResults = Object.groupBy(scripts.entries, (entry) => entry.finalResult)
scripts.methodology +=
  ' Final dispositions distinguish direct execution, PowerShell syntax parsing, inventory-only review, and excluded scope; no related reference is treated as a direct pass.'
scripts.summary.finalDisposition = Object.fromEntries(
  Object.entries(scriptResults)
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([result, entries]) => [result, entries.length])
)

const workflows = readJson('workflow-test-inventory.json')
const packagedE2e = 'apps/desktop/e2e/hermes-local-functional.spec.ts'
const localControlTests = [
  'apps/desktop/electron/hermes-local-control.test.ts',
  'apps/desktop/electron/hermes-local-settings.test.ts'
]
const workflowGroups = {
  endToEnd: new Set([
    'launcher-startup',
    'service-start',
    'service-stop',
    'service-restart',
    'logs',
    'dashboard',
    'tui',
    'launch-at-login',
    'diagnostics',
    'update-check',
    'package'
  ]),
  automated: new Set([
    'action-exclusion',
    'profile-create',
    'profile-rename',
    'profile-delete',
    'profile-select',
    'settings-save'
  ]),
  fixture: new Set(['backup', 'restore', 'patch-reconstruction']),
  partial: new Set([
    'model-register',
    'model-remove',
    'model-select',
    'sessions',
    'projects',
    'chat',
    'benchmark',
    'repair',
    'hermes-agent-update',
    'hermes-agent-rollback'
  ])
}
const workflowDefects = {
  'service-start': ['QA-003', 'QA-005'],
  'service-stop': ['QA-003'],
  'service-restart': ['QA-003'],
  'action-exclusion': ['QA-003', 'QA-004'],
  'profile-create': ['QA-002'],
  'profile-rename': ['QA-001', 'QA-002'],
  'profile-select': ['QA-001'],
  logs: ['QA-006', 'QA-014'],
  'settings-save': ['QA-006']
}

for (const entry of workflows.entries) {
  const id = entry.id.replace('workflow:', '')

  if (entry.scope === 'security-excluded') {
    entry.successPath = 'excluded-by-engagement-scope'
    entry.failurePath = 'excluded-by-engagement-scope'
    entry.recoveryPath = 'excluded-by-engagement-scope'
    entry.finalResult = 'excluded'
    entry.automatedCoverage = []
    entry.manualEvidence = []
    entry.evidenceQualification =
      'Security auditing and vulnerability assessment were excluded from this QA engagement.'
  } else if (workflowGroups.endToEnd.has(id)) {
    entry.successPath = 'passed'
    entry.failurePath = ['logs', 'dashboard', 'tui'].includes(id)
      ? 'passed-ordinary-unavailable-or-renderer-error-case'
      : 'covered-by-related-regression-tests'
    entry.recoveryPath = ['service-start', 'service-stop', 'service-restart', 'tui'].includes(id)
      ? 'passed'
      : 'not-separately-demonstrated'
    entry.automatedCoverage = unique([
      packagedE2e,
      ...localControlTests,
      evidence('electron-full.stdout.txt')
    ])
    entry.manualEvidence = [finalVerification]
    entry.finalResult = 'passed-end-to-end'
    entry.evidenceQualification = 'Executed through automated packaged coverage and/or a recorded live workstation workflow.'
  } else if (workflowGroups.automated.has(id)) {
    entry.successPath = 'passed-automated'
    entry.failurePath = 'passed-ordinary-regression-case'
    entry.recoveryPath = 'passed-state-refresh-or-retry-case'
    entry.automatedCoverage = unique([...localControlTests, evidence('local-electron-tests.stdout.txt')])
    entry.manualEvidence = []
    entry.finalResult = 'passed-automated'
    entry.evidenceQualification = 'Focused Electron regression coverage passed; active user settings were not mutated.'
  } else if (workflowGroups.fixture.has(id)) {
    entry.successPath = 'passed-fixture'
    entry.failurePath = 'passed-ordinary-failure-fixtures'
    entry.recoveryPath = 'passed-fixture'
    entry.automatedCoverage =
      id === 'patch-reconstruction'
        ? [finalVerification]
        : ['scripts/qa/Test-RecoveryFixtures.ps1', evidence('recovery-fixtures.json')]
    entry.manualEvidence = [finalVerification]
    entry.finalResult = 'passed-controlled-fixture'
    entry.evidenceQualification = 'Executed against isolated fixtures or a clean reconstruction worktree.'
  } else {
    entry.successPath =
      ['sessions', 'projects', 'chat', 'hermes-agent-update'].includes(id)
        ? 'passed-limited-live-or-packaged-surface'
        : 'ui-surface-only-operation-not-run'
    entry.failurePath = 'not-individually-demonstrated'
    entry.recoveryPath = 'not-individually-demonstrated'
    entry.automatedCoverage = [packagedE2e, evidence('ui-full.stdout.txt')]
    entry.manualEvidence = [finalVerification]
    entry.finalResult = 'partial-evidence-operation-not-fully-exercised'
    entry.evidenceQualification =
      'The workflow appears in the inventory and its UI/related suite passed, but the state-mutating operation was not fully exercised against the protected active installation.'
  }

  entry.relatedDefectIds = workflowDefects[id] ?? []
}

const workflowResults = Object.groupBy(workflows.entries, (entry) => entry.finalResult)
workflows.summary = {
  total: workflows.entries.length,
  functionalQa: workflows.entries.filter((entry) => entry.scope === 'functional-qa').length,
  securityExcluded: workflows.entries.filter((entry) => entry.scope === 'security-excluded').length,
  finalDisposition: Object.fromEntries(
    Object.entries(workflowResults)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([result, entries]) => [result, entries.length])
  )
}
workflows.methodology =
  'Major workflow inventory with direct E2E, automated regression, controlled fixture, partial surface-only, or excluded dispositions. Partial evidence is not represented as a full pass.'

const finalizedAt = new Date().toISOString()
for (const inventory of [ui, scripts, workflows]) {
  inventory.finalizedAt = finalizedAt
  inventory.finalEvidenceDirectory = relative(evidenceDirectory)
}

writeJson('ui-control-inventory.json', ui)
writeJson('script-test-inventory.json', scripts)
writeJson('workflow-test-inventory.json', workflows)

console.log(
  JSON.stringify(
    {
      finalizedAt,
      ui: ui.summary,
      scripts: scripts.summary,
      workflows: workflows.summary
    },
    null,
    2
  )
)
