import { invoke } from '@tauri-apps/api/core';
import type { TelemetrySnapshot } from './domain';

export const demoSnapshot: TelemetrySnapshot = {
  provenance: 'demo',
  health: { id: 'codex-local', displayName: 'Codex Local', status: 'connected', message: 'Sanitized frontend preview', checkedAt: '2026-08-28T08:00:00Z' },
  quotas: [{ id: 'session', label: 'Current account', windows: [{ id: 'rolling', label: 'Rolling window', used: 31, limit: 100, resetAt: '2026-08-28T14:00:00Z' }] }],
  models: [{ id: 'gpt-5.4', displayName: 'GPT-5.4', isDefault: true, reasoningEfforts: ['low', 'medium', 'high'], reasoningEffortDescriptions: {}, defaultReasoningEffort: 'medium' }, { id: 'gpt-5.4-mini', displayName: 'GPT-5.4 mini', isDefault: false, reasoningEfforts: ['minimal', 'low', 'medium'], reasoningEffortDescriptions: {}, defaultReasoningEffort: 'low' }],
  activity: [{ id: 'demo-1', kind: 'connection', message: 'Demo telemetry loaded', at: '2026-08-28T08:00:00Z' }]
};

export async function fetchTelemetry(): Promise<TelemetrySnapshot> {
  if (!('__TAURI_INTERNALS__' in window)) return demoSnapshot;
  return invoke<TelemetrySnapshot>('refresh_telemetry');
}
