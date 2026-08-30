import { invoke } from '@tauri-apps/api/core';
import type { ExecutionPolicy, OrchestrationState, ReleaseInfo } from './domain';

export const getOrchestrationState = () => invoke<OrchestrationState>('get_orchestration_state');
export const getReleaseInfo = () => invoke<ReleaseInfo>('get_release_info');
export const completeOnboarding = () => invoke<OrchestrationState>('complete_onboarding');
export const resetOnboarding = () => invoke<OrchestrationState>('reset_onboarding');
export const createProject = (displayName: string, localPath: string) => invoke<OrchestrationState>('create_project', { displayName, localPath });
export const updateProject = (id: string, displayName: string, localPath: string) => invoke<OrchestrationState>('update_project', { id, displayName, localPath });
export const archiveProject = (id: string) => invoke<OrchestrationState>('archive_project', { id });
export const createTask = (projectId: string, title: string, instruction: string) => invoke<OrchestrationState>('create_task', { projectId, title, instruction });
export const updateTask = (id: string, title: string, instruction: string) => invoke<OrchestrationState>('update_task', { id, title, instruction });
export const submitTask = (id: string) => invoke<OrchestrationState>('submit_task', { id });
export const approveTask = (id: string) => invoke<OrchestrationState>('approve_task', { id });
export const cancelTask = (id: string) => invoke<OrchestrationState>('cancel_task', { id });
export const runTask = (taskId: string, model: string | null, reasoningEffort: string | null, policy: ExecutionPolicy) => invoke<OrchestrationState>('run_task', { taskId, model, reasoningEffort, policy });
export const retryExecution = (runId: string) => invoke<OrchestrationState>('retry_execution', { runId });
export const cancelExecution = (runId: string) => invoke<OrchestrationState>('cancel_execution', { runId });
