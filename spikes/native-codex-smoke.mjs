import { createHash } from 'node:crypto';
import { mkdir, readdir, readFile, rm, stat, writeFile } from 'node:fs/promises';
import path from 'node:path';
import process from 'node:process';
import { CodexAppServerClient } from '../src/codex-app-server-client.mjs';

const root = 'C:/Users/phamn/PROJECT-ORCH-NATIVE';
const runtimeRoot = 'C:/Users/phamn/PROJECT-ORCH-NATIVE-RUNTIME';
const fixture = path.join(runtimeRoot, 'fixture');
const evidence = 'C:/Users/phamn/PROJECT-ORCH-NATIVE-EVIDENCE/N1';
const prompt = 'Reply exactly READY.\nUnicode transport probe: → tiếng Việt.\nDo not edit files and do not run repository commands.';

function hash(data) { return createHash('sha256').update(data).digest('hex'); }
async function fileHash(file) { return hash(await readFile(file)); }
async function inventory(directory) {
  const output = [];
  async function walk(current, relative = '') {
    for (const entry of (await readdir(current, { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
      const rel = path.join(relative, entry.name).replaceAll('\\', '/');
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) { output.push(`D ${rel}`); await walk(full, rel); }
      else if (entry.isFile()) output.push(`F ${rel} ${await fileHash(full)}`);
      else output.push(`O ${rel}`);
    }
  }
  await walk(directory);
  return `${output.join('\n')}${output.length ? '\n' : ''}`;
}
function redact(value, key = '') {
  const sensitive = /(?:token|secret|credential|authorization|password|email|account.*(?:id|name)|user.*(?:id|name))/i.test(key);
  if (sensitive) return '[REDACTED]';
  if (Array.isArray(value)) return value.map((entry) => redact(entry));
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, redact(v, k)]));
  return value;
}
function pretty(value) { return `${JSON.stringify(redact(value), null, 2)}\n`; }
function findAgentText(value) {
  if (!value || typeof value !== 'object') return null;
  if (value.type === 'agentMessage' && typeof value.text === 'string') return value.text;
  for (const child of Object.values(value)) { const found = findAgentText(child); if (found !== null) return found; }
  return null;
}
async function write(name, content) { await writeFile(path.join(evidence, name), content, 'utf8'); }

await mkdir(fixture, { recursive: true });
const existing = await inventory(fixture);
if (existing) throw new Error(`Fixture is not empty; refusing to use it:\n${existing}`);
await mkdir(evidence, { recursive: true });
const codexHome = process.env.CODEX_HOME || path.join(process.env.USERPROFILE, '.codex');
const configPath = path.join(codexHome, 'config.toml');
const configBefore = await fileHash(configPath);
const fixtureBefore = await inventory(fixture);
await write('environment.txt', `node=${process.version}\nplatform=${process.platform}\narch=${process.arch}\ncodexHome=${codexHome}\nconfigPath=${configPath}\n`);
await write('codex-config-hash-before.txt', `${configBefore}  ${configPath}\n`);
await write('fixture-before.txt', fixtureBefore);

const events = [];
const client = new CodexAppServerClient().start();
let finalText = null;
let started = false;
let completed = null;
let expectedTurnId = null;
let settle;
const completedPromise = new Promise((resolve, reject) => { settle = { resolve, reject }; });
const unsubscribe = client.onNotification((event) => {
  events.push(event);
  const text = findAgentText(event.params);
  if (text !== null) finalText = text;
  if (event.method === 'turn/started' && (!expectedTurnId || event.params?.turn?.id === expectedTurnId || event.params?.turnId === expectedTurnId)) started = true;
  if (event.method === 'turn/completed' && (!expectedTurnId || event.params?.turn?.id === expectedTurnId || event.params?.turnId === expectedTurnId)) { completed = event.params; settle.resolve(); }
});

let init; let quota; let models; let thread; let turn;
try {
  init = await client.initialize();
  quota = await client.readRateLimits();
  models = await client.listModels();
  const defaultModel = models.data?.find((model) => model.isDefault === true);
  thread = await client.startThread({ cwd: fixture, model: defaultModel?.id });
  const threadId = thread.thread?.id ?? thread.threadId;
  if (!threadId) throw new Error('thread/start did not return a thread ID');
  turn = await client.startTurn(threadId, prompt);
  expectedTurnId = turn.turn?.id ?? turn.turnId;
  if (!expectedTurnId) throw new Error('turn/start did not return a turn ID');
  await Promise.race([completedPromise, new Promise((_, reject) => setTimeout(() => reject(new Error('turn/completed was not received within 120000ms')), 120_000))]);
} catch (error) {
  settle?.reject?.(error);
  throw error;
} finally {
  unsubscribe();
  await client.close();
  await write('appserver-init.sanitized.json', pretty(init ?? { unavailable: true }));
  await write('quota.sanitized.json', pretty(quota ?? { unavailable: true }));
  await write('models.sanitized.json', pretty(models ?? { unavailable: true }));
  await write('thread-start.sanitized.json', pretty(thread ?? { unavailable: true }));
  await write('turn-events.sanitized.jsonl', events.map((event) => JSON.stringify(redact(event))).join('\n') + (events.length ? '\n' : ''));
}

const configAfter = await fileHash(configPath);
const fixtureAfter = await inventory(fixture);
await write('codex-config-hash-after.txt', `${configAfter}  ${configPath}\n`);
await write('fixture-after.txt', fixtureAfter);
const checks = {
  initialize: Boolean(init), quota: Boolean(quota), models: Array.isArray(models?.data), threadId: Boolean(thread?.thread?.id ?? thread?.threadId),
  turnId: Boolean(expectedTurnId), turnStarted: started, turnCompleted: completed?.turn?.status === 'completed' || completed?.status === 'completed',
  finalReplyExactlyReady: finalText === 'READY', fixtureUnchanged: fixtureBefore === fixtureAfter, configUnchanged: configBefore === configAfter,
};
await write('smoke-result.txt', `${Object.entries(checks).map(([key, value]) => `${key}=${value}`).join('\n')}\nfinalReply=${JSON.stringify(finalText)}\n`);
if (Object.values(checks).some((value) => value !== true)) throw new Error(`Smoke acceptance failed: ${JSON.stringify(checks)}`);
