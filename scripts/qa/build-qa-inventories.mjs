import { execFileSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url))
const repositoryRoot = path.resolve(scriptDirectory, '..', '..')
const hermesAgentRoot = path.join(repositoryRoot, 'source', 'hermes-agent')
const reportDirectory = path.join(repositoryRoot, 'reports', 'qa')
const typescriptPath = path.join(hermesAgentRoot, 'node_modules', 'typescript', 'lib', 'typescript.js')
const typescriptModule = await import(pathToFileURL(typescriptPath).href)
const ts = typescriptModule.default ?? typescriptModule

const gitFiles = (workingDirectory) =>
  execFileSync('git.exe', ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], {
    cwd: workingDirectory,
    encoding: 'utf8',
    maxBuffer: 32 * 1024 * 1024,
    windowsHide: true
  })
    .split('\0')
    .filter(Boolean)

const rootFiles = gitFiles(repositoryRoot)
const nestedFiles = gitFiles(hermesAgentRoot)
const desktopSources = nestedFiles.filter(
  (file) =>
    file.startsWith('apps/desktop/src/') &&
    file.endsWith('.tsx') &&
    !/\.(?:test|spec)\.tsx$/.test(file) &&
    !file.includes('/__snapshots__/')
)
const desktopTests = nestedFiles.filter((file) => /apps\/desktop\/.+\.(?:test|spec)\.(?:ts|tsx|mjs)$/.test(file))
const testText = desktopTests
  .map((file) => fs.readFileSync(path.join(hermesAgentRoot, file), 'utf8').toLocaleLowerCase())
  .join('\n')

const interactiveHtml = new Set(['a', 'button', 'input', 'option', 'select', 'textarea'])
const interactiveComponents =
  /(?:Button|Checkbox|Combobox|CommandItem|ContextMenuItem|DropdownMenuItem|Input|Link|MenuItem|RadioGroupItem|SelectItem|SelectTrigger|Slider|Switch|TabsTrigger|Textarea|Toggle)$/
const stateComponents =
  /(?:Alert|Dialog|Drawer|EmptyState|ErrorBoundary|Loading|Modal|Popover|Progress|Skeleton|Spinner|Toast|Tooltip)$/
const handlerAttributes = new Set([
  'onChange',
  'onCheckedChange',
  'onClick',
  'onKeyDown',
  'onOpenChange',
  'onSelect',
  'onSubmit',
  'onValueChange'
])

function jsxTagName(node) {
  return node.tagName.getText()
}

function attribute(node, name) {
  return node.attributes.properties.find(
    (property) => ts.isJsxAttribute(property) && property.name.getText() === name
  )
}

function attributeValue(node, name) {
  const property = attribute(node, name)

  if (!property?.initializer) {
    return ''
  }

  if (ts.isStringLiteral(property.initializer)) {
    return property.initializer.text.trim()
  }

  if (ts.isJsxExpression(property.initializer) && property.initializer.expression) {
    const expression = property.initializer.expression

    if (ts.isStringLiteralLike(expression)) {
      return expression.text.trim()
    }
  }

  return ''
}

function literalChildren(node) {
  if (!ts.isJsxElement(node)) {
    return ''
  }

  const parts = []

  const visit = (child) => {
    if (ts.isJsxText(child)) {
      const value = child.getText().replace(/\s+/g, ' ').trim()

      if (value) {
        parts.push(value)
      }
    } else if (ts.isJsxExpression(child) && child.expression && ts.isStringLiteralLike(child.expression)) {
      parts.push(child.expression.text.trim())
    } else if (ts.isJsxElement(child)) {
      child.children.forEach(visit)
    }
  }

  node.children.forEach(visit)

  return parts.filter(Boolean).join(' ').slice(0, 160)
}

function componentName(node) {
  let current = node.parent

  while (current) {
    if (ts.isFunctionDeclaration(current) && current.name) {
      return current.name.text
    }

    if (
      (ts.isArrowFunction(current) || ts.isFunctionExpression(current)) &&
      ts.isVariableDeclaration(current.parent) &&
      ts.isIdentifier(current.parent.name)
    ) {
      return current.parent.name.text
    }

    current = current.parent
  }

  return 'module'
}

function screenFor(file) {
  const appPrefix = 'apps/desktop/src/app/'

  if (file.startsWith(appPrefix)) {
    return file.slice(appPrefix.length).split('/')[0]
  }

  return file.startsWith('apps/desktop/src/components/') ? 'shared' : 'shell'
}

function relatedTestFiles(sourceFile) {
  const directory = path.posix.dirname(sourceFile)
  const basename = path.posix.basename(sourceFile, '.tsx')

  return desktopTests.filter(
    (testFile) =>
      path.posix.dirname(testFile) === directory ||
      path.posix.basename(testFile).startsWith(`${basename}.`) ||
      testFile.includes(`/${basename}/`)
  )
}

const uiControls = []

for (const file of desktopSources) {
  const absolutePath = path.join(hermesAgentRoot, file)
  const sourceText = fs.readFileSync(absolutePath, 'utf8')
  const source = ts.createSourceFile(file, sourceText, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX)
  const relatedTests = relatedTestFiles(file)

  const visit = (node) => {
    if (ts.isJsxElement(node) || ts.isJsxSelfClosingElement(node)) {
      const opening = ts.isJsxElement(node) ? node.openingElement : node
      const tag = jsxTagName(opening)
      const role = attributeValue(opening, 'role')
      const handler = opening.attributes.properties
        .filter((property) => ts.isJsxAttribute(property) && handlerAttributes.has(property.name.getText()))
        .map((property) => {
          const value = property.initializer?.getText() ?? 'true'
          return `${property.name.getText()}=${value}`
        })
        .join('; ')
        .slice(0, 320)
      const kind =
        interactiveHtml.has(tag) || interactiveComponents.test(tag) || handler || /^(button|link|menuitem|tab)$/.test(role)
          ? 'control'
          : stateComponents.test(tag)
            ? 'state'
            : null

      if (kind) {
        const position = source.getLineAndCharacterOfPosition(opening.getStart(source))
        const label =
          attributeValue(opening, 'aria-label') ||
          attributeValue(opening, 'title') ||
          literalChildren(node) ||
          attributeValue(opening, 'placeholder') ||
          `${tag} at line ${position.line + 1}`
        const lowerLabel = label.toLocaleLowerCase()
        const labelMentioned = lowerLabel.length >= 3 && testText.includes(lowerLabel)
        const bridgeReferences = [
          ...new Set(
            (node.getText(source).match(/(?:window\.)?hermesDesktop(?:\.[A-Za-z][A-Za-z0-9]*)+/g) ?? []).map(
              (value) => value.replace(/^window\./, '')
            )
          )
        ]

        uiControls.push({
          id: `ui:${file}:${position.line + 1}:${position.character + 1}`,
          kind,
          screen: screenFor(file),
          visibleLabel: label,
          sourceFile: `source/hermes-agent/${file}`,
          line: position.line + 1,
          component: componentName(opening),
          element: tag,
          role: role || null,
          handler: handler || null,
          relatedPreloadOrIpcMethod: bridgeReferences,
          expectedBehaviour: handler ? 'Invoke the declared handler and provide visible state or feedback.' : 'Render the declared UI state.',
          persistenceBehaviour: 'Inspect the owning component or workflow record.',
          happyPathTest: labelMentioned ? 'Visible label is asserted in an automated desktop test.' : null,
          failurePathTest: null,
          keyboardTest:
            interactiveHtml.has(tag) || /(?:Button|Input|Link|Select|Switch|TabsTrigger|Textarea)$/.test(tag)
              ? 'Native or shared-component keyboard semantics apply; workflow evidence is required.'
              : null,
          currentAutomatedCoverage: labelMentioned
            ? 'label-asserted'
            : relatedTests.length
              ? 'related-component-suite'
              : 'not-demonstrated',
          relatedTests: relatedTests.map((testFile) => `source/hermes-agent/${testFile}`),
          finalResult: 'pending-final-evidence',
          evidenceLocation: [],
          relatedDefectId: null
        })
      }
    }

    ts.forEachChild(node, visit)
  }

  visit(source)
}

const rootScriptPattern = /\.(?:bat|cmd|mjs|ps1|psm1|py)$/i
const nestedExecutable = (file) =>
  /^(?:apps\/desktop\/scripts|scripts|bin)\//.test(file) && /\.(?:bat|cmd|js|mjs|ps1|py|sh)$/i.test(file)
const nestedEntryPoint = (file) =>
  /^(?:run_agent\.py|cli\.py)$/.test(file) || /(?:^|\/)__main__\.py$/.test(file)
const scriptFiles = [
  ...rootFiles.filter((file) => rootScriptPattern.test(file)).map((file) => ({
    repository: 'root',
    file,
    absolute: path.join(repositoryRoot, file)
  })),
  ...nestedFiles
    .filter((file) => nestedExecutable(file) || nestedEntryPoint(file))
    .map((file) => ({
      repository: 'nested-hermes-agent',
      file: `source/hermes-agent/${file}`,
      absolute: path.join(hermesAgentRoot, file)
    }))
]

const inventorySearchText = [
  ...rootFiles
    .filter((file) => /(?:test|spec|docs?|readme)/i.test(file) && /\.(?:md|ps1|psm1|py|json)$/i.test(file))
    .map((file) => fs.readFileSync(path.join(repositoryRoot, file), 'utf8')),
  ...nestedFiles
    .filter((file) => /(?:test|spec|docs?|readme)/i.test(file) && /\.(?:md|mjs|py|ts|tsx)$/i.test(file))
    .map((file) => fs.readFileSync(path.join(hermesAgentRoot, file), 'utf8'))
].join('\n')

const scriptInventory = scriptFiles.map(({ repository, file, absolute }) => {
  const basename = path.basename(file)
  const excluded = /(?:^|[\\/])security(?:[\\/]|-)|Security-Scan-Hermes-Local\.ps1/i.test(file)
  const referenced = inventorySearchText.includes(basename)

  return {
    id: `script:${file.replaceAll('\\', '/')}`,
    repository,
    file: file.replaceAll('\\', '/'),
    kind: path.extname(file).slice(1).toLocaleLowerCase(),
    bytes: fs.statSync(absolute).size,
    scope: excluded ? 'security-excluded' : 'functional-qa',
    successPath: excluded ? 'excluded-by-engagement-scope' : referenced ? 'referenced-by-test-or-documentation' : 'not-demonstrated',
    failurePath: excluded ? 'excluded-by-engagement-scope' : 'not-demonstrated',
    argumentValidation: excluded ? 'excluded-by-engagement-scope' : 'pending-final-evidence',
    exitCodeVerification: excluded ? 'excluded-by-engagement-scope' : 'pending-final-evidence',
    currentAutomatedCoverage: excluded ? 'excluded-by-engagement-scope' : referenced ? 'related-evidence-found' : 'not-demonstrated',
    finalResult: excluded ? 'excluded' : 'pending-final-evidence',
    evidenceLocation: []
  }
})

function packageScripts(packageFile, prefix) {
  const absolute = path.join(hermesAgentRoot, packageFile)
  const value = JSON.parse(fs.readFileSync(absolute, 'utf8'))

  return Object.entries(value.scripts ?? {}).map(([name, command]) => ({
    id: `script:npm:${prefix}:${name}`,
    repository: 'nested-hermes-agent',
    file: `source/hermes-agent/${packageFile}#scripts.${name}`,
    kind: 'npm-script',
    bytes: String(command).length,
    scope: /^audit(?::|$)/.test(name) ? 'security-excluded' : 'functional-qa',
    command,
    successPath: 'pending-final-evidence',
    failurePath: 'not-demonstrated',
    argumentValidation: 'not-applicable',
    exitCodeVerification: 'provided-by-npm',
    currentAutomatedCoverage: 'pending-final-evidence',
    finalResult: /^audit(?::|$)/.test(name) ? 'excluded' : 'pending-final-evidence',
    evidenceLocation: []
  }))
}

for (const packageFile of ['package.json', 'apps/desktop/package.json', 'web/package.json', 'ui-tui/package.json']) {
  if (fs.existsSync(path.join(hermesAgentRoot, packageFile))) {
    scriptInventory.push(...packageScripts(packageFile, packageFile.replace('/package.json', '').replace('package.json', 'root')))
  }
}

const workflows = [
  ['launcher-startup', 'Launcher startup', 'Electron boot → environment setup → managed stack start → renderer'],
  ['service-start', 'Start services', 'Home/Services Start → preload.startAction → IPC action:start → Start-Hermes-Local.ps1 → status refresh'],
  ['service-stop', 'Stop services', 'Services Stop → preload.startAction → IPC action:start → Stop-Hermes-Local.ps1 → status refresh'],
  ['service-restart', 'Restart services', 'Home/Services Restart → preload.startAction → IPC action:start → Restart-Hermes-Local.ps1 → status refresh'],
  ['action-exclusion', 'Exclusive service actions', 'Any action button → IPC task registry → one active operation → disabled peer actions'],
  ['profile-create', 'Create profile', 'Profiles New → saveProfile → IPC profile:save → user-settings.json → snapshot'],
  ['profile-rename', 'Rename profile', 'Profiles Save → original-name-aware IPC → replace profile → selected-profile migration → snapshot'],
  ['profile-delete', 'Delete profile', 'Profiles Delete → IPC profile:delete → fallback selection → snapshot'],
  ['profile-select', 'Select profile', 'Profile selector → IPC profile:select → user-settings.json → snapshot'],
  ['model-register', 'Register GGUF', 'Models Register → file picker → IPC model:register → user-settings.json → select'],
  ['model-remove', 'Remove registered model', 'Models Remove → IPC model:remove → settings update → snapshot'],
  ['model-select', 'Select model', 'Models Select → IPC model:select → settings update → snapshot'],
  ['settings-save', 'Save runtime settings', 'Settings form → IPC settings:save → validation → user-settings.json → snapshot'],
  ['logs', 'View logs', 'Logs selector/refresh → IPC logs → bounded read → redacted renderer output'],
  ['dashboard', 'Open dashboard', 'Dashboard Open → IPC dashboard:open → system browser'],
  ['tui', 'Open TUI', 'TUI panel → Electron PTY → managed Hermes CLI → stream cleanup'],
  ['sessions', 'Sessions', 'Sidebar route → Hermes session APIs → list/resume/delete feedback'],
  ['projects', 'Projects', 'Sidebar route → project APIs → list/create/open feedback'],
  ['chat', 'Desktop chat', 'Composer → Hermes API → streamed response/tool state → persisted session'],
  ['launch-at-login', 'Launch at sign-in', 'About toggle → IPC login-item:set → current-user Electron setting → refreshed state'],
  ['backup', 'Create backup', 'Backup action → Backup-Hermes-Local.ps1 → archive + manifest + checksum → task output'],
  ['restore', 'Restore backup', 'Restore-Hermes-Local.ps1 → validation → pre-restore backup → state restore → restart'],
  ['diagnostics', 'Export diagnostics', 'Diagnostics action → Export-Hermes-Diagnostics.ps1 → bounded archive → task output'],
  ['benchmark', 'Run benchmark', 'Benchmark action → Benchmark-Hermes-Local.ps1 → report → refreshed dashboard'],
  ['repair', 'Repair installation', 'Repair action → Repair-Hermes-Local.ps1 → dependency/config recovery → diagnostics'],
  ['update-check', 'Check for updates', 'Update action → Update-Hermes-Local.ps1 -Mode Check → task output'],
  ['patch-reconstruction', 'Reconstruct integration', 'Pinned upstream → ordered mail patches → dependency install → tree comparison'],
  ['package', 'Build/package launcher', 'Desktop build → electron-builder → portable executable → package smoke/E2E'],
  ['security', 'Security screen/action', 'Inventory only; security auditing and vulnerability assessment excluded']
].map(([id, name, chain]) => ({
  id: `workflow:${id}`,
  name,
  chain,
  scope: id === 'security' ? 'security-excluded' : 'functional-qa',
  successPath: 'pending-final-evidence',
  failurePath: 'pending-final-evidence',
  recoveryPath: 'pending-final-evidence',
  automatedCoverage: [],
  manualEvidence: [],
  finalResult: id === 'security' ? 'excluded' : 'pending-final-evidence',
  relatedDefectIds: []
}))

fs.mkdirSync(reportDirectory, { recursive: true })

const generatedAt = new Date().toISOString()
const metadata = {
  schemaVersion: 1,
  generatedAt,
  generator: 'scripts/qa/build-qa-inventories.mjs',
  rootCommit: execFileSync('git.exe', ['rev-parse', 'HEAD'], { cwd: repositoryRoot, encoding: 'utf8' }).trim(),
  nestedCommit: execFileSync('git.exe', ['rev-parse', 'HEAD'], { cwd: hermesAgentRoot, encoding: 'utf8' }).trim()
}

fs.writeFileSync(
  path.join(reportDirectory, 'ui-control-inventory.json'),
  `${JSON.stringify(
    {
      ...metadata,
      methodology:
        'TypeScript AST inventory of tracked production TSX. Coverage labels are conservative static evidence, not claims that a control passed.',
      summary: {
        sourceFiles: desktopSources.length,
        controls: uiControls.filter((entry) => entry.kind === 'control').length,
        states: uiControls.filter((entry) => entry.kind === 'state').length,
        labelAsserted: uiControls.filter((entry) => entry.currentAutomatedCoverage === 'label-asserted').length,
        relatedComponentSuite: uiControls.filter((entry) => entry.currentAutomatedCoverage === 'related-component-suite')
          .length,
        notDemonstrated: uiControls.filter((entry) => entry.currentAutomatedCoverage === 'not-demonstrated').length
      },
      entries: uiControls
    },
    null,
    2
  )}\n`
)
fs.writeFileSync(
  path.join(reportDirectory, 'script-test-inventory.json'),
  `${JSON.stringify(
    {
      ...metadata,
      methodology:
        'First-party executable files in the current worktree and npm entry points. Related references are discovery evidence only; final execution evidence is populated by finalize-qa-inventories.mjs.',
      summary: {
        total: scriptInventory.length,
        functionalQa: scriptInventory.filter((entry) => entry.scope === 'functional-qa').length,
        securityExcluded: scriptInventory.filter((entry) => entry.scope === 'security-excluded').length
      },
      entries: scriptInventory
    },
    null,
    2
  )}\n`
)
fs.writeFileSync(
  path.join(reportDirectory, 'workflow-test-inventory.json'),
  `${JSON.stringify({ ...metadata, entries: workflows }, null, 2)}\n`
)

console.log(
  JSON.stringify(
    {
      reportDirectory,
      ui: {
        sourceFiles: desktopSources.length,
        entries: uiControls.length
      },
      scripts: scriptInventory.length,
      workflows: workflows.length
    },
    null,
    2
  )
)
