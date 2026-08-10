import { readFile, writeFile } from 'node:fs/promises'
import { fileURLToPath } from 'node:url'
import ts from 'typescript'

const controlUrl = new URL('../electron/hermes-local-control.ts', import.meta.url)
const controlPath = fileURLToPath(controlUrl)
const source = await readFile(controlPath, 'utf8')
const newline = source.includes('\r\n') ? '\r\n' : '\n'
const lines = source.split(/\r?\n/)

function findSingleLine(description, predicate) {
  const matches = []

  for (let index = 0; index < lines.length; index += 1) {
    if (predicate(lines[index])) {
      matches.push(index)
    }
  }

  if (matches.length !== 1) {
    throw new Error(
      `Expected exactly one ${description} line in ${controlPath}; found ${matches.length}.`,
    )
  }

  return matches[0]
}

const privatePathLine = findSingleLine(
  'private-path redaction',
  (line) =>
    line.includes('safe = safe.replaceAll(privatePath') &&
    line.includes('privatePath.replaceAll('),
)
const privatePathIndent = lines[privatePathLine].match(/^\s*/)?.[0] ?? ''
const privateMarker = JSON.stringify('[PRIVATE-PATH]')
const backslashLiteral = JSON.stringify('\\')
const slashLiteral = JSON.stringify('/')
lines[privatePathLine] =
  `${privatePathIndent}safe = safe.replaceAll(privatePath, ${privateMarker}).replaceAll(` +
  `privatePath.replaceAll(${backslashLiteral}, ${slashLiteral}), ${privateMarker})`

const markerBufferLine = findSingleLine(
  'Desktop update marker buffer',
  (line) =>
    line.includes('task.desktopUpdateMarkerBuffer =') &&
    line.includes('rawText') &&
    line.includes('.slice(-64 * 1024)'),
)
const markerBufferIndent = lines[markerBufferLine].match(/^\s*/)?.[0] ?? ''
const emptyStringLiteral = JSON.stringify('')
lines[markerBufferLine] =
  `${markerBufferIndent}task.desktopUpdateMarkerBuffer = (` +
  `(task.desktopUpdateMarkerBuffer || ${emptyStringLiteral}) + rawText` +
  `).slice(-64 * 1024)`

const updated = lines.join(newline)
await writeFile(controlPath, updated, 'utf8')

const parsed = ts.createSourceFile(
  controlPath,
  updated,
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TS,
)

if (parsed.parseDiagnostics.length > 0) {
  const details = parsed.parseDiagnostics.slice(0, 5).map((diagnostic) => {
    const start = diagnostic.start ?? 0
    const position = parsed.getLineAndCharacterOfPosition(start)
    const message = ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')
    const line = lines[position.line] ?? ''
    return `${position.line + 1}:${position.character + 1} ${message}; line=${JSON.stringify(line)}`
  })
  throw new Error(
    `Desktop control source still has TypeScript parse errors after repair:\n${details.join('\n')}`,
  )
}

const verified = await readFile(controlPath, 'utf8')
const verifiedLines = verified.split(/\r?\n/)
if (
  verifiedLines[privatePathLine] !== lines[privatePathLine] ||
  verifiedLines[markerBufferLine] !== lines[markerBufferLine]
) {
  throw new Error(`Desktop control source repairs did not persist in ${controlPath}.`)
}

const statusbarUrl = new URL('../src/app/shell/hooks/use-statusbar-items.tsx', import.meta.url)
const statusbarPath = fileURLToPath(statusbarUrl)
let statusbar = await readFile(statusbarPath, 'utf8')
const statusbarNewline = statusbar.includes('\r\n') ? '\r\n' : '\n'
const managedStart = statusbar.indexOf('  const managedLocalConnection =')
const clientStart = statusbar.indexOf(
  '  const clientVersionItem = useMemo<StatusbarItem>(() => {',
  managedStart,
)

if (managedStart < 0 || clientStart <= managedStart) {
  throw new Error(
    `Could not locate the generated managed-workstation footer declaration in ${statusbarPath}.`,
  )
}

const managedDeclaration = [
  "  const managedLocalHost = connection?.remoteHost?.trim().toLowerCase() ?? ''",
  '  const managedLocalConnection =',
  "    connection?.mode === 'remote' &&",
  "    (managedLocalHost === 'localhost' ||",
  "      managedLocalHost.startsWith('localhost:') ||",
  "      managedLocalHost.startsWith('127.') ||",
  "      managedLocalHost.startsWith('http://localhost') ||",
  "      managedLocalHost.startsWith('https://localhost') ||",
  "      managedLocalHost.startsWith('http://127.') ||",
  "      managedLocalHost.startsWith('https://127.') ||",
  "      managedLocalHost === '::1' ||",
  "      managedLocalHost.startsWith('[::1]') ||",
  "      managedLocalHost.startsWith('http://[::1]') ||",
  "      managedLocalHost.startsWith('https://[::1]'))",
  '',
  '',
].join(statusbarNewline)

statusbar =
  statusbar.slice(0, managedStart) +
  managedDeclaration +
  statusbar.slice(clientStart)
await writeFile(statusbarPath, statusbar, 'utf8')

const parsedStatusbar = ts.createSourceFile(
  statusbarPath,
  statusbar,
  ts.ScriptTarget.Latest,
  true,
  ts.ScriptKind.TSX,
)

if (parsedStatusbar.parseDiagnostics.length > 0) {
  const details = parsedStatusbar.parseDiagnostics.slice(0, 5).map((diagnostic) => {
    const start = diagnostic.start ?? 0
    const position = parsedStatusbar.getLineAndCharacterOfPosition(start)
    const message = ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')
    return `${position.line + 1}:${position.character + 1} ${message}`
  })
  throw new Error(
    `Desktop statusbar source has TypeScript parse errors after repair:\n${details.join('\n')}`,
  )
}

process.stdout.write(
  `Rewrote and parsed generated Desktop control and footer statements before typecheck: ${controlPath}; ${statusbarPath}\n`,
)
