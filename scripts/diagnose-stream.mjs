import fs from 'node:fs/promises';
import path from 'node:path';

const directory = path.join(process.env.APPDATA, 'FeishuLocStudio');
const config = JSON.parse(await fs.readFile(path.join(directory, 'config.json'), 'utf8'));
const workflow = JSON.parse(await fs.readFile(path.join(directory, 'workflow.json'), 'utf8'));
const failed = new Set(workflow.summary?.failedRowIds ?? []);
const targets = workflow.sheetState.columns.filter(column => workflow.targetColumns.includes(column.id) && !workflow.lockedColumns.includes(column.id));
const rows = workflow.sheetState.rows.filter(row => failed.has(row.rowId)).slice(0, 2);
if (!rows.length || !targets.length) throw new Error('No failed rows or targets available');
const items = rows.map(row => ({
  rowId: row.rowId,
  source: row.values[workflow.sourceColumnId],
  missingTargets: targets.filter(column => workflow.writeMode !== 'fill_empty' || !row.values[column.id]?.trim()).map(column => ({ id: column.id, name: column.name })),
}));
const body = {
  model: config.openaiModel,
  messages: [
    { role: 'system', content: 'Translate each source into its missingTargets languages. Preserve placeholders and tags. Return JSON items with rowId and translations keyed by target id.' },
    { role: 'user', content: items.map(item => JSON.stringify(item)).join('\n') },
  ],
  response_format: { type: 'json_schema', json_schema: { name: 'translation_batch_multi', strict: true, schema: {
    type: 'object', properties: { items: { type: 'array', items: { type: 'object', properties: {
      rowId: { type: 'string' }, translations: { type: 'object', properties: Object.fromEntries(targets.map(column => [column.id, { type: 'string' }])), additionalProperties: false },
    }, required: ['rowId', 'translations'], additionalProperties: false } } }, required: ['items'], additionalProperties: false,
  } } },
  stream: true, stream_options: { include_usage: true },
};
if (config.openaiModel === 'gemini-3.8-flash') body.reasoning_effort = 'high';
const response = await fetch(`${config.openaiBaseUrl.replace(/\/$/, '')}/chat/completions`, {
  method: 'POST', headers: { Authorization: `Bearer ${config.openaiApiKey.trim()}`, 'Content-Type': 'application/json' },
  body: JSON.stringify(body), signal: AbortSignal.timeout(180000),
});
const raw = await response.text();
const outputDirectory = path.join(directory, 'diagnostics');
await fs.mkdir(outputDirectory, { recursive: true });
const output = path.join(outputDirectory, `probe-${Date.now()}.sse`);
await fs.writeFile(output, raw);
const frames = raw.split('\n').map(line => line.trim()).filter(line => line.startsWith('data:')).map(line => line.slice(5).trim());
const events = frames.filter(frame => frame && frame !== '[DONE]').map(frame => { try { return JSON.parse(frame); } catch { return { invalidJson: true }; } });
const content = events.map(event => event.choices?.[0]?.delta?.content ?? '').join('');
let parsed;
try { parsed = JSON.parse(content); } catch {}
console.log(JSON.stringify({ status: response.status, contentType: response.headers.get('content-type'), model: config.openaiModel,
  rows: items.length, frames: frames.length, done: frames.includes('[DONE]'),
  finishReasons: events.flatMap(event => (event.choices ?? []).map(choice => choice.finish_reason).filter(reason => reason != null)),
  contentBytes: Buffer.byteLength(content), validJson: !!parsed,
  completeTargets: !!parsed && items.every(item => item.missingTargets.every(target => parsed.items?.some(result => result.rowId === item.rowId && typeof result.translations?.[target.id] === 'string' && result.translations[target.id].trim()))),
  errorPresent: events.some(event => event.error), rawResponse: output,
}, null, 2));
