import fs from 'node:fs/promises';
import path from 'node:path';

const directory = path.join(process.env.APPDATA, 'FeishuLocStudio');
const before = JSON.parse(await fs.readFile(path.join(directory, 'diagnostics/selfcheck-before.json'), 'utf8'));
const after = JSON.parse(await fs.readFile(path.join(directory, 'workflow.json'), 'utf8'));
const config = JSON.parse(await fs.readFile(path.join(directory, 'config.json'), 'utf8'));
const changes = [];
const structuralIssues = [];
const tokens = text => (text.match(/<[^>]+>|\{[^{}]+\}/g) ?? []).sort().join('\n');
for (const row of after.sheetState.rows) {
  const previous = before.sheetState.rows.find(original => original.sheetId === row.sheetId && original.rowNumber === row.rowNumber);
  if (!previous || previous.rowId !== row.rowId) throw new Error('Row identity changed');
  for (const [column, value] of Object.entries(row.values)) {
    if (value === (previous.values[column] ?? '')) continue;
    if (!before.targetColumns.includes(column) || before.lockedColumns.includes(column) || (previous.values[column] ?? '').trim()) throw new Error('Unexpected overwrite or locked column change');
    if (tokens(value) !== tokens(previous.values[before.sourceColumnId] ?? '')) structuralIssues.push({ rowId: row.rowId, column });
    changes.push({ sheetId: row.sheetId, rowNumber: row.rowNumber, columnIndex: row.availableColumnIds.indexOf(column), rowId: row.rowId, column, value });
  }
}
if (!changes.length) throw new Error('No saved changes to verify');
const root = 'https://open.feishu.cn/open-apis';
const authResponse = await fetch(`${root}/auth/v3/tenant_access_token/internal`, {
  method: 'POST', headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ app_id: config.feishuAppId, app_secret: config.feishuAppSecret }),
});
const auth = await authResponse.json();
if (auth.code !== 0 || !auth.tenant_access_token) throw new Error(`Feishu auth failed: ${auth.code}`);
const mismatches = [];
let checked = 0;
const snapshots = {};
const unexpectedLiveChanges = [];
let unchangedCellsVerified = 0;
for (const sheetId of new Set(changes.map(change => change.sheetId))) {
  const response = await fetch(`${root}/sheets/v2/spreadsheets/${after.sheetState.spreadsheetId}/values/${encodeURIComponent(sheetId)}`, {
    headers: { Authorization: `Bearer ${auth.tenant_access_token}` },
  });
  const result = await response.json();
  if (result.code !== 0) throw new Error(`Feishu read failed: ${result.code}`);
  snapshots[sheetId] = result.data;
  const values = result.data.valueRange.values;
  for (const change of changes.filter(change => change.sheetId === sheetId)) {
    if (values[change.rowNumber - 1]?.[change.columnIndex] !== change.value) mismatches.push({ rowId: change.rowId, column: change.column });
    checked++;
  }
  for (const row of before.sheetState.rows.filter(row => row.sheetId === sheetId)) {
    for (const [columnIndex, column] of row.availableColumnIds.entries()) {
      if (changes.some(change => change.sheetId === sheetId && change.rowNumber === row.rowNumber && change.column === column)) continue;
      const actual = values[row.rowNumber - 1]?.[columnIndex];
      const actualText = actual == null ? '' : typeof actual === 'object' ? JSON.stringify(actual) : String(actual);
      if (actualText !== (row.values[column] ?? '')) unexpectedLiveChanges.push({ rowId: row.rowId, column });
      else unchangedCellsVerified++;
    }
  }
}
const report = { status: after.summary?.status, changedCells: changes.length, changedRows: new Set(changes.map(change => `${change.sheetId}:${change.rowNumber}`)).size,
  failedRows: after.summary?.failedRows?.length, independentlyVerified: checked, mismatches, structuralIssues,
  unchangedCellsVerified, unexpectedLiveChanges };
await fs.writeFile(path.join(directory, 'diagnostics/selfcheck-live-readback.json'), JSON.stringify(snapshots, null, 2));
await fs.writeFile(path.join(directory, 'diagnostics/selfcheck-report.json'), JSON.stringify(report, null, 2));
console.log(JSON.stringify(report, null, 2));
