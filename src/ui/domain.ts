export type ProviderStatus = 'connected' | 'disconnected' | 'error' | 'loading';
export type ReasoningEffort = string;

export interface ProviderHealth { id: string; displayName: string; status: ProviderStatus; message: string; checkedAt: string | null; }
export interface QuotaWindow { id: string; label: string | null; used: number | null; limit: number | null; resetAt: string | null; }
export interface QuotaBucket { id: string; label: string | null; windows: QuotaWindow[]; }
export interface ModelCapability { id: string; displayName: string; isDefault: boolean | null; reasoningEfforts: ReasoningEffort[]; reasoningEffortDescriptions: Record<string, string>; defaultReasoningEffort: ReasoningEffort | null; }
export interface ActivityEvent { id: string; kind: 'connection' | 'refresh' | 'error'; message: string; at: string; }
export interface TelemetrySnapshot { health: ProviderHealth; quotas: QuotaBucket[]; models: ModelCapability[]; activity: ActivityEvent[]; provenance: 'live' | 'demo'; }
export type TaskStatus = 'DRAFT' | 'PENDING_APPROVAL' | 'APPROVED' | 'CANCELLED';
export interface RegisteredProject { id: string; displayName: string; localPath: string; createdAt: string; updatedAt: string; archived: boolean; }
export interface ApprovalTask { id: string; projectId: string; title: string; instruction: string; status: TaskStatus; createdAt: string; updatedAt: string; approvedAt: string | null; }
export interface TimelineEvent { id: string; eventType: string; at: string; projectId: string | null; taskId: string | null; }
export interface LocalWorkerStatus { id: string; displayName: string; status: ProviderStatus; message: string; checkedAt: string | null; }
export interface AppSettings { onboardingCompleted: boolean; }
export interface ReleaseInfo { version: string; codexVersion: string | null; dataLocation: string; }
export type ExecutionStatus = 'QUEUED' | 'STARTING' | 'RUNNING' | 'SUCCEEDED' | 'FAILED' | 'CANCELLED' | 'INTERRUPTED';
export type ExecutionPolicy = 'READ_ONLY' | 'WORKSPACE_WRITE';
export interface ExecutionRun { id: string; taskId: string; projectId: string; workerId: string; status: ExecutionStatus; selectedModel: string | null; selectedReasoningEffort: string | null; executionPolicy: ExecutionPolicy; providerThreadId: string | null; providerTurnId: string | null; createdAt: string; startedAt: string | null; finishedAt: string | null; summary: string | null; error: string | null; }
export interface OrchestrationState { version: number; projects: RegisteredProject[]; tasks: ApprovalTask[]; activity: TimelineEvent[]; worker: LocalWorkerStatus; runs: ExecutionRun[]; settings: AppSettings; }

export function usedPercent(window: QuotaWindow): number | null {
  if (window.used === null || window.limit === null || window.limit <= 0) return null;
  return Math.min(100, Math.max(0, (window.used / window.limit) * 100));
}
