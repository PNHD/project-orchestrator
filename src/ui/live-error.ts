import type { TelemetrySnapshot } from './domain';

export function liveErrorSnapshot(checkedAt: string): TelemetrySnapshot {
  return {
    provenance: 'live',
    health: {
      id: 'codex-local',
      displayName: 'Codex Local',
      status: 'error',
      message: 'Codex telemetry is temporarily unavailable. Please retry.',
      checkedAt,
    },
    quotas: [],
    models: [],
    activity: [],
  };
}
