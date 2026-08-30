use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thiserror::Error;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const CANCEL_TIMEOUT: Duration = Duration::from_secs(10);
const RUN_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const MAX_RESULT_CHARS: usize = 8_000;
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Error)]
enum ProviderError {
    #[error("Codex provider unavailable")]
    Unavailable,
    #[error("Codex protocol error")]
    Protocol,
    #[error("Codex request timed out")]
    Timeout,
    #[error("Codex execution failed")]
    Failed,
}
impl ProviderError {
    fn telemetry_message(&self) -> &'static str {
        "Codex telemetry is temporarily unavailable. Please retry."
    }
    fn execution_message(&self) -> &'static str {
        match self {
            Self::Timeout => "Codex execution timed out and was stopped.",
            Self::Protocol => "Codex returned malformed execution data.",
            Self::Unavailable => "Codex execution is temporarily unavailable.",
            Self::Failed => "Codex execution did not complete successfully.",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderHealth {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub message: String,
    pub checked_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaWindow {
    pub id: String,
    pub label: Option<String>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub reset_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaBucket {
    pub id: String,
    pub label: Option<String>,
    pub windows: Vec<QuotaWindow>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCapability {
    pub id: String,
    pub display_name: String,
    pub is_default: Option<bool>,
    pub reasoning_efforts: Vec<String>,
    pub reasoning_effort_descriptions: BTreeMap<String, String>,
    pub default_reasoning_effort: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEvent {
    pub id: String,
    pub kind: String,
    pub message: String,
    pub at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEvent {
    pub id: String,
    pub event_type: String,
    pub at: String,
    pub project_id: Option<String>,
    pub task_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisteredProject {
    pub id: String,
    pub display_name: String,
    pub local_path: String,
    pub created_at: String,
    pub updated_at: String,
    pub archived: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Draft,
    PendingApproval,
    Approved,
    Cancelled,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalTask {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub instruction: String,
    pub status: TaskStatus,
    pub created_at: String,
    pub updated_at: String,
    pub approved_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalWorkerStatus {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub message: String,
    pub checked_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    #[serde(default)]
    pub onboarding_completed: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionStatus {
    Queued,
    Starting,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    Interrupted,
}
impl ExecutionStatus {
    fn active(&self) -> bool {
        matches!(self, Self::Queued | Self::Starting | Self::Running)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExecutionPolicy {
    ReadOnly,
    WorkspaceWrite,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRun {
    pub id: String,
    pub task_id: String,
    pub project_id: String,
    pub worker_id: String,
    pub status: ExecutionStatus,
    pub selected_model: Option<String>,
    pub selected_reasoning_effort: Option<String>,
    pub execution_policy: ExecutionPolicy,
    pub provider_thread_id: Option<String>,
    pub provider_turn_id: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub summary: Option<String>,
    pub error: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrchestrationState {
    pub version: u32,
    pub projects: Vec<RegisteredProject>,
    pub tasks: Vec<ApprovalTask>,
    pub activity: Vec<TimelineEvent>,
    pub worker: LocalWorkerStatus,
    #[serde(default)]
    pub runs: Vec<ExecutionRun>,
    #[serde(default)]
    pub settings: AppSettings,
}
impl Default for OrchestrationState {
    fn default() -> Self {
        Self {
            version: 3,
            projects: vec![],
            tasks: vec![],
            activity: vec![],
            runs: vec![],
            settings: AppSettings::default(),
            worker: LocalWorkerStatus {
                id: "codex-local".into(),
                display_name: "Codex Local".into(),
                status: "unavailable".into(),
                message: "Telemetry has not been refreshed.".into(),
                checked_at: None,
            },
        }
    }
}

#[derive(Debug, Error)]
enum StoreError {
    #[error("state unavailable")]
    Unavailable,
    #[error("state malformed")]
    Malformed,
    #[error("invalid path")]
    InvalidPath,
    #[error("duplicate project")]
    DuplicateProject,
    #[error("project missing")]
    ProjectNotFound,
    #[error("task missing")]
    TaskNotFound,
    #[error("invalid transition")]
    InvalidTransition,
    #[error("invalid task")]
    InvalidTask,
    #[error("run not allowed")]
    RunNotAllowed,
    #[error("active run")]
    ActiveRun,
    #[error("run missing")]
    RunNotFound,
    #[error("run not active")]
    RunNotActive,
    #[error("invalid selection")]
    InvalidSelection,
}
impl StoreError {
    fn frontend_message(&self) -> &'static str {
        match self {
            Self::InvalidPath => "Choose an existing local directory.",
            Self::DuplicateProject => "That local project is already registered.",
            Self::ProjectNotFound => "The selected project is no longer available.",
            Self::TaskNotFound => "The selected task is no longer available.",
            Self::InvalidTransition => "That action is no longer allowed.",
            Self::InvalidTask => "Enter a task title and instruction.",
            Self::Malformed => "Local orchestration data needs attention. It was not changed.",
            Self::RunNotAllowed => "Only an approved task in an active project can run.",
            Self::ActiveRun => "This task already has an active run.",
            Self::RunNotFound => "The execution run was not found.",
            Self::RunNotActive => "That execution run is no longer active.",
            Self::InvalidSelection => {
                "The selected execution configuration is no longer available."
            }
            Self::Unavailable => "Local orchestration data is temporarily unavailable.",
        }
    }
}

fn state_file(p: &Path) -> PathBuf {
    p.join("orchestration-state.json")
}
fn stable_id(k: &str) -> String {
    format!(
        "{k}-{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    )
}
fn now() -> String {
    format!(
        "unix:{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    )
}
fn bounded(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_RESULT_CHARS)
        .collect()
}
fn validate_state(s: &OrchestrationState) -> Result<(), StoreError> {
    if s.version != 3 {
        return Err(StoreError::Malformed);
    }
    let mut projects = std::collections::BTreeSet::new();
    let mut paths = std::collections::BTreeSet::new();
    for p in &s.projects {
        if p.id.is_empty() || !projects.insert(&p.id) || !paths.insert(p.local_path.to_lowercase())
        {
            return Err(StoreError::Malformed);
        }
    }
    let mut tasks = std::collections::BTreeSet::new();
    for t in &s.tasks {
        if t.id.is_empty() || !tasks.insert(&t.id) || !projects.contains(&t.project_id) {
            return Err(StoreError::Malformed);
        }
    }
    let mut runs = std::collections::BTreeSet::new();
    for r in &s.runs {
        if r.id.is_empty()
            || !runs.insert(&r.id)
            || !tasks.contains(&r.task_id)
            || !projects.contains(&r.project_id)
        {
            return Err(StoreError::Malformed);
        }
    }
    Ok(())
}
fn read_state_from(path: &Path) -> Result<OrchestrationState, StoreError> {
    if !path.exists() {
        return Ok(OrchestrationState::default());
    }
    let text = fs::read_to_string(path).map_err(|_| StoreError::Unavailable)?;
    let mut value: Value = serde_json::from_str(&text).map_err(|_| StoreError::Malformed)?;
    if value.get("version").and_then(Value::as_u64) == Some(1) {
        value["runs"] = json!([]);
    }
    if matches!(value.get("version").and_then(Value::as_u64), Some(1 | 2)) {
        value["version"] = json!(3);
        value["settings"] = json!({"onboardingCompleted": false});
    }
    let s: OrchestrationState = serde_json::from_value(value).map_err(|_| StoreError::Malformed)?;
    validate_state(&s)?;
    Ok(s)
}
fn write_state_to(path: &Path, s: &OrchestrationState) -> Result<(), StoreError> {
    validate_state(s)?;
    let parent = path.parent().ok_or(StoreError::Unavailable)?;
    fs::create_dir_all(parent).map_err(|_| StoreError::Unavailable)?;
    let temp = parent.join(format!(".orchestration-state-{}.tmp", stable_id("write")));
    fs::write(
        &temp,
        serde_json::to_vec_pretty(s).map_err(|_| StoreError::Unavailable)?,
    )
    .map_err(|_| StoreError::Unavailable)?;
    fs::rename(&temp, path).map_err(|_| {
        let _ = fs::remove_file(&temp);
        StoreError::Unavailable
    })
}
/// Serializes every application-owned read-mutate-write cycle, including telemetry refresh.
struct StoreGate(Mutex<()>);
impl StoreGate {
    fn new() -> Self {
        Self(Mutex::new(()))
    }
    fn read(&self, path: &Path) -> Result<OrchestrationState, StoreError> {
        let _g = self.0.lock().map_err(|_| StoreError::Unavailable)?;
        read_state_from(path)
    }
    fn mutate<F>(&self, path: &Path, f: F) -> Result<OrchestrationState, StoreError>
    where
        F: FnOnce(&mut OrchestrationState) -> Result<(), StoreError>,
    {
        let _g = self.0.lock().map_err(|_| StoreError::Unavailable)?;
        let mut s = read_state_from(path)?;
        f(&mut s)?;
        write_state_to(path, &s)?;
        Ok(s)
    }
}
fn valid_directory(p: &str) -> Result<String, StoreError> {
    let c = fs::canonicalize(p).map_err(|_| StoreError::InvalidPath)?;
    if !c.is_dir() {
        return Err(StoreError::InvalidPath);
    }
    Ok(c.to_string_lossy().into_owned())
}
fn record(
    s: &mut OrchestrationState,
    event: &str,
    project_id: Option<String>,
    task_id: Option<String>,
) {
    s.activity.push(TimelineEvent {
        id: stable_id("event"),
        event_type: event.into(),
        at: now(),
        project_id,
        task_id,
    })
}
fn set_onboarding(s: &mut OrchestrationState, completed: bool) {
    s.settings.onboarding_completed = completed;
}
fn create_project(
    s: &mut OrchestrationState,
    name: String,
    path: String,
) -> Result<(), StoreError> {
    let path = valid_directory(&path)?;
    if s.projects
        .iter()
        .any(|p| p.local_path.eq_ignore_ascii_case(&path))
    {
        return Err(StoreError::DuplicateProject);
    }
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::InvalidPath);
    }
    let t = now();
    let id = stable_id("project");
    s.projects.push(RegisteredProject {
        id: id.clone(),
        display_name: name.into(),
        local_path: path,
        created_at: t.clone(),
        updated_at: t,
        archived: false,
    });
    record(s, "project.created", Some(id), None);
    Ok(())
}
fn update_project(
    s: &mut OrchestrationState,
    id: &str,
    name: String,
    path: String,
) -> Result<(), StoreError> {
    let path = valid_directory(&path)?;
    let name = name.trim();
    if name.is_empty() {
        return Err(StoreError::InvalidPath);
    }
    if s.projects
        .iter()
        .any(|p| p.id != id && p.local_path.eq_ignore_ascii_case(&path))
    {
        return Err(StoreError::DuplicateProject);
    }
    let p = s
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(StoreError::ProjectNotFound)?;
    p.display_name = name.into();
    p.local_path = path;
    p.updated_at = now();
    record(s, "project.updated", Some(id.into()), None);
    Ok(())
}
fn archive_project(s: &mut OrchestrationState, id: &str) -> Result<(), StoreError> {
    let p = s
        .projects
        .iter_mut()
        .find(|p| p.id == id)
        .ok_or(StoreError::ProjectNotFound)?;
    p.archived = true;
    p.updated_at = now();
    record(s, "project.archived", Some(id.into()), None);
    Ok(())
}
fn create_task(
    s: &mut OrchestrationState,
    pid: String,
    title: String,
    instruction: String,
) -> Result<(), StoreError> {
    if !s.projects.iter().any(|p| p.id == pid && !p.archived) {
        return Err(StoreError::ProjectNotFound);
    }
    let title = title.trim();
    let instruction = instruction.trim();
    if title.is_empty() || instruction.is_empty() {
        return Err(StoreError::InvalidTask);
    }
    let t = now();
    let id = stable_id("task");
    s.tasks.push(ApprovalTask {
        id: id.clone(),
        project_id: pid.clone(),
        title: title.into(),
        instruction: instruction.into(),
        status: TaskStatus::Draft,
        created_at: t.clone(),
        updated_at: t,
        approved_at: None,
    });
    record(s, "task.created", Some(pid), Some(id));
    Ok(())
}
fn update_task(
    s: &mut OrchestrationState,
    id: &str,
    title: String,
    instruction: String,
) -> Result<(), StoreError> {
    let title = title.trim();
    let instruction = instruction.trim();
    if title.is_empty() || instruction.is_empty() {
        return Err(StoreError::InvalidTask);
    }
    let t = s
        .tasks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or(StoreError::TaskNotFound)?;
    if t.status != TaskStatus::Draft {
        return Err(StoreError::InvalidTransition);
    }
    t.title = title.into();
    t.instruction = instruction.into();
    t.updated_at = now();
    Ok(())
}
fn transition_task(
    s: &mut OrchestrationState,
    id: &str,
    target: TaskStatus,
) -> Result<(), StoreError> {
    let project_id = {
        let t = s
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or(StoreError::TaskNotFound)?;
        let valid = matches!(
            (&t.status, &target),
            (TaskStatus::Draft, TaskStatus::PendingApproval)
                | (TaskStatus::PendingApproval, TaskStatus::Approved)
        ) || target == TaskStatus::Cancelled;
        if !valid {
            return Err(StoreError::InvalidTransition);
        }
        t.status = target.clone();
        t.updated_at = now();
        if target == TaskStatus::Approved {
            t.approved_at = Some(t.updated_at.clone())
        }
        t.project_id.clone()
    };
    record(
        s,
        match target {
            TaskStatus::PendingApproval => "task.submitted",
            TaskStatus::Approved => "task.approved",
            TaskStatus::Cancelled => "task.cancelled",
            TaskStatus::Draft => return Err(StoreError::InvalidTransition),
        },
        Some(project_id),
        Some(id.into()),
    );
    Ok(())
}
fn selection(x: &Option<String>) -> Result<(), StoreError> {
    if x.as_ref()
        .is_some_and(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
    {
        Err(StoreError::InvalidSelection)
    } else {
        Ok(())
    }
}
fn start_run(
    s: &mut OrchestrationState,
    task_id: &str,
    model: Option<String>,
    effort: Option<String>,
    policy: ExecutionPolicy,
) -> Result<ExecutionRun, StoreError> {
    selection(&model)?;
    selection(&effort)?;
    let task = s
        .tasks
        .iter()
        .find(|t| t.id == task_id)
        .ok_or(StoreError::TaskNotFound)?;
    let p = s
        .projects
        .iter()
        .find(|p| p.id == task.project_id)
        .ok_or(StoreError::ProjectNotFound)?;
    let c = valid_directory(&p.local_path).map_err(|_| StoreError::RunNotAllowed)?;
    if p.archived || !c.eq_ignore_ascii_case(&p.local_path) || task.status != TaskStatus::Approved {
        return Err(StoreError::RunNotAllowed);
    }
    if s.runs
        .iter()
        .any(|r| r.task_id == task_id && r.status.active())
    {
        return Err(StoreError::ActiveRun);
    }
    let r = ExecutionRun {
        id: stable_id("run"),
        task_id: task.id.clone(),
        project_id: p.id.clone(),
        worker_id: "codex-local".into(),
        status: ExecutionStatus::Queued,
        selected_model: model.map(|x| x.trim().into()),
        selected_reasoning_effort: effort.map(|x| x.trim().into()),
        execution_policy: policy,
        provider_thread_id: None,
        provider_turn_id: None,
        created_at: now(),
        started_at: None,
        finished_at: None,
        summary: None,
        error: None,
    };
    record(
        s,
        "execution.queued",
        Some(r.project_id.clone()),
        Some(r.task_id.clone()),
    );
    s.runs.push(r.clone());
    Ok(r)
}
fn update_run(
    s: &mut OrchestrationState,
    id: &str,
    status: ExecutionStatus,
    summary: Option<String>,
    error: Option<String>,
) -> Result<(), StoreError> {
    let r = s
        .runs
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or(StoreError::RunNotFound)?;
    if !r.status.active() {
        return Err(StoreError::RunNotActive);
    }
    let legal = matches!(
        (&r.status, &status),
        (ExecutionStatus::Queued, ExecutionStatus::Starting)
            | (ExecutionStatus::Queued, ExecutionStatus::Failed)
            | (ExecutionStatus::Queued, ExecutionStatus::Interrupted)
            | (ExecutionStatus::Starting, ExecutionStatus::Running)
            | (ExecutionStatus::Starting, ExecutionStatus::Failed)
            | (ExecutionStatus::Starting, ExecutionStatus::Cancelled)
            | (ExecutionStatus::Starting, ExecutionStatus::Interrupted)
            | (ExecutionStatus::Running, ExecutionStatus::Succeeded)
            | (ExecutionStatus::Running, ExecutionStatus::Failed)
            | (ExecutionStatus::Running, ExecutionStatus::Cancelled)
            | (ExecutionStatus::Running, ExecutionStatus::Interrupted)
    );
    if !legal {
        return Err(StoreError::InvalidTransition);
    }
    r.status = status.clone();
    if status == ExecutionStatus::Running {
        r.started_at = Some(now())
    }
    if !status.active() {
        r.finished_at = Some(now());
        r.summary = summary.map(|x| bounded(&x));
        r.error = error.map(|x| bounded(&x))
    }
    let (pid, tid) = (r.project_id.clone(), r.task_id.clone());
    record(
        s,
        match status {
            ExecutionStatus::Starting => return Ok(()),
            ExecutionStatus::Running => "execution.started",
            ExecutionStatus::Succeeded => "execution.succeeded",
            ExecutionStatus::Failed => "execution.failed",
            ExecutionStatus::Cancelled => "execution.cancelled",
            ExecutionStatus::Interrupted => "execution.interrupted",
            ExecutionStatus::Queued => return Err(StoreError::InvalidTransition),
        },
        Some(pid),
        Some(tid),
    );
    Ok(())
}
fn set_provider_ids(
    s: &mut OrchestrationState,
    id: &str,
    thread: String,
    turn: String,
) -> Result<(), StoreError> {
    let r = s
        .runs
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or(StoreError::RunNotFound)?;
    if r.status != ExecutionStatus::Starting {
        return Err(StoreError::InvalidTransition);
    }
    r.provider_thread_id = Some(thread);
    r.provider_turn_id = Some(turn);
    Ok(())
}
fn retry_run(s: &mut OrchestrationState, id: &str) -> Result<ExecutionRun, StoreError> {
    let old = s
        .runs
        .iter()
        .find(|r| r.id == id)
        .ok_or(StoreError::RunNotFound)?
        .clone();
    if !matches!(
        old.status,
        ExecutionStatus::Failed | ExecutionStatus::Cancelled | ExecutionStatus::Interrupted
    ) {
        return Err(StoreError::InvalidTransition);
    }
    let r = start_run(
        s,
        &old.task_id,
        old.selected_model,
        old.selected_reasoning_effort,
        old.execution_policy,
    )?;
    record(
        s,
        "execution.retried",
        Some(r.project_id.clone()),
        Some(r.task_id.clone()),
    );
    Ok(r)
}
fn reconcile_runs(s: &mut OrchestrationState) {
    for id in s
        .runs
        .iter()
        .filter(|r| r.status.active())
        .map(|r| r.id.clone())
        .collect::<Vec<_>>()
    {
        let _ = update_run(
            s,
            &id,
            ExecutionStatus::Interrupted,
            None,
            Some("The desktop app restarted before this run completed.".into()),
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TelemetrySnapshot {
    pub health: ProviderHealth,
    pub quotas: Vec<QuotaBucket>,
    pub models: Vec<ModelCapability>,
    pub activity: Vec<ActivityEvent>,
    pub provenance: String,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub version: String,
    pub codex_version: Option<String>,
    pub data_location: String,
}

fn local_codex_version() -> Option<String> {
    let mut child = Command::new("codex")
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().ok()? {
            break status;
        }
        if started.elapsed() >= Duration::from_secs(2) {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    if !status.success() {
        return None;
    }
    let mut bytes = vec![];
    child
        .stdout
        .take()?
        .take(256)
        .read_to_end(&mut bytes)
        .ok()?;
    let value = String::from_utf8_lossy(&bytes);
    let line = value.lines().next()?.trim();
    (!line.is_empty()).then(|| bounded(line))
}
struct RpcProcess {
    child: Child,
    lines: mpsc::Receiver<Result<String, ()>>,
    next_id: u64,
    notices: VecDeque<Value>,
}
impl RpcProcess {
    fn start() -> Result<Self, ProviderError> {
        Self::start_with("codex", &["app-server"])
    }
    fn start_with(command: &str, args: &[&str]) -> Result<Self, ProviderError> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| ProviderError::Unavailable)?;
        let stdout = child.stdout.take().ok_or(ProviderError::Unavailable)?;
        let (tx, lines) = mpsc::channel();
        std::thread::spawn(move || {
            let mut r = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match r.read_line(&mut line) {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx.send(Ok(line.clone())).is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        let _ = tx.send(Err(()));
                        break;
                    }
                }
            }
        });
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || {
                let mut r = BufReader::new(stderr);
                let mut line = String::new();
                let mut total = 0;
                while total < 65536 {
                    line.clear();
                    match r.read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => total += n,
                    }
                }
            });
        }
        Ok(Self {
            child,
            lines,
            next_id: 1,
            notices: VecDeque::new(),
        })
    }
    fn terminate(&mut self) {
        let _ = self.child.stdin.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
    fn next_value(&mut self, t: Duration) -> Result<Value, ProviderError> {
        let deadline = Instant::now() + t;
        let mut bad = 0;
        loop {
            let remain = deadline.saturating_duration_since(Instant::now());
            if remain.is_zero() {
                return Err(ProviderError::Timeout);
            }
            match self.lines.recv_timeout(remain) {
                Ok(Ok(line)) => match serde_json::from_str(&line) {
                    Ok(v) => return Ok(v),
                    Err(_) => {
                        bad += 1;
                        if bad > 8 {
                            return Err(ProviderError::Protocol);
                        }
                    }
                },
                Ok(Err(())) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(ProviderError::Unavailable)
                }
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(ProviderError::Timeout),
            }
        }
    }
    fn request(
        &mut self,
        method: &str,
        params: Value,
        t: Duration,
    ) -> Result<Value, ProviderError> {
        let id = self.next_id;
        self.next_id += 1;
        let body = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params});
        self.child
            .stdin
            .as_mut()
            .ok_or(ProviderError::Unavailable)?
            .write_all(format!("{body}\n").as_bytes())
            .map_err(|_| ProviderError::Unavailable)?;
        let deadline = Instant::now() + t;
        loop {
            let v = self.next_value(deadline.saturating_duration_since(Instant::now()))?;
            if v.get("method").is_some() {
                self.notices.push_back(v);
                continue;
            }
            if v.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if v.get("error").is_some() {
                return Err(ProviderError::Failed);
            }
            return v.get("result").cloned().ok_or(ProviderError::Protocol);
        }
    }
    fn notification(&mut self, t: Duration) -> Result<Option<Value>, ProviderError> {
        if let Some(n) = self.notices.pop_front() {
            return Ok(Some(n));
        }
        let v = self.next_value(t)?;
        if v.get("method").is_some() {
            Ok(Some(v))
        } else {
            Ok(None)
        }
    }
    fn respond(&mut self, id: &Value, result: Value) -> Result<(), ProviderError> {
        let body = json!({"jsonrpc":"2.0","id":id,"result":result});
        self.child
            .stdin
            .as_mut()
            .ok_or(ProviderError::Unavailable)?
            .write_all(format!("{body}\n").as_bytes())
            .map_err(|_| ProviderError::Unavailable)
    }
}
impl Drop for RpcProcess {
    fn drop(&mut self) {
        self.terminate()
    }
}
fn string(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str).map(str::to_owned))
}
fn normalize_quotas(raw: &Value) -> Vec<QuotaBucket> {
    let buckets = raw
        .pointer("/rateLimits/buckets")
        .or_else(|| raw.get("buckets"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    buckets
        .into_iter()
        .enumerate()
        .map(|(i, b)| {
            let id = string(&b, &["id", "limitId"]).unwrap_or_else(|| format!("bucket-{i}"));
            let wins = b
                .get("windows")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_else(|| {
                    ["primary", "secondary"]
                        .iter()
                        .filter_map(|k| b.get(*k).cloned())
                        .collect()
                });
            QuotaBucket {
                id: id.clone(),
                label: string(&b, &["label", "name"]),
                windows: wins
                    .into_iter()
                    .enumerate()
                    .map(|(n, w)| QuotaWindow {
                        id: string(&w, &["id", "name"]).unwrap_or_else(|| format!("{id}-{n}")),
                        label: string(&w, &["label", "window"]),
                        used: w
                            .get("used")
                            .or_else(|| w.get("usedPercent"))
                            .and_then(Value::as_f64),
                        limit: w
                            .get("limit")
                            .and_then(Value::as_f64)
                            .or_else(|| w.get("usedPercent").map(|_| 100.0)),
                        reset_at: w.get("resetAt").or_else(|| w.get("resetsAt")).map(|x| {
                            x.as_i64()
                                .map(|n| format!("unix:{n}"))
                                .unwrap_or_else(|| x.as_str().unwrap_or_default().into())
                        }),
                    })
                    .collect(),
            }
        })
        .collect()
}
pub fn normalize_models(raw: &Value) -> Vec<ModelCapability> {
    raw.get("models")
        .or_else(|| raw.get("data"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|m| {
            let id = string(&m, &["id", "model"])?;
            let mut desc = BTreeMap::new();
            let efforts = m
                .get("reasoningEfforts")
                .or_else(|| m.get("supportedReasoningEfforts"))
                .and_then(Value::as_array)
                .map(|xs| {
                    xs.iter()
                        .filter_map(|x| {
                            if let Some(v) = x.as_str() {
                                Some(v.into())
                            } else {
                                let v = x.get("reasoningEffort")?.as_str()?.to_owned();
                                if let Some(d) = x.get("description").and_then(Value::as_str) {
                                    desc.insert(v.clone(), d.into());
                                }
                                Some(v)
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(ModelCapability {
                id: id.clone(),
                display_name: string(&m, &["displayName", "name"]).unwrap_or(id),
                is_default: m.get("isDefault").and_then(Value::as_bool),
                reasoning_efforts: efforts,
                reasoning_effort_descriptions: desc,
                default_reasoning_effort: string(&m, &["defaultReasoningEffort"]),
            })
        })
        .collect()
}
pub fn read_live_telemetry() -> Result<TelemetrySnapshot, String> {
    let t = now();
    let mut rpc = RpcProcess::start().map_err(|e| e.telemetry_message().to_owned())?;
    rpc.request(
        "initialize",
        json!({"clientInfo":{"name":"project-orchestrator","version":"0.3.0"},"capabilities":{}}),
        REQUEST_TIMEOUT,
    )
    .map_err(|e| e.telemetry_message().to_owned())?;
    let q = normalize_quotas(
        &rpc.request("account/rateLimits/read", json!({}), REQUEST_TIMEOUT)
            .map_err(|e| e.telemetry_message().to_owned())?,
    );
    let m = normalize_models(
        &rpc.request("model/list", json!({}), REQUEST_TIMEOUT)
            .map_err(|e| e.telemetry_message().to_owned())?,
    );
    Ok(TelemetrySnapshot {
        health: ProviderHealth {
            id: "codex-local".into(),
            display_name: "Codex Local".into(),
            status: "connected".into(),
            message: "Read-only app-server telemetry connected".into(),
            checked_at: Some(t.clone()),
        },
        quotas: q,
        models: m,
        activity: vec![ActivityEvent {
            id: "refresh".into(),
            kind: "refresh".into(),
            message: "Telemetry refreshed".into(),
            at: t,
        }],
        provenance: "live".into(),
    })
}

struct RunControls(Mutex<HashMap<String, mpsc::Sender<()>>>);
impl RunControls {
    fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}
fn visible_result(turn: &Value) -> String {
    let mut p = Vec::new();
    if let Some(items) = turn.get("items").and_then(Value::as_array) {
        for i in items {
            if i.get("type").and_then(Value::as_str) == Some("agentMessage") {
                if let Some(t) = i.get("text").and_then(Value::as_str) {
                    p.push(t.to_owned())
                }
            }
        }
    }
    let v = bounded(&p.join("\n"));
    if v.is_empty() {
        "Codex completed the task.".into()
    } else {
        v
    }
}
fn payload(run: &ExecutionRun, cwd: &str, instruction: &str) -> (Value, Value) {
    let sandbox = match run.execution_policy {
        ExecutionPolicy::ReadOnly => "read-only",
        ExecutionPolicy::WorkspaceWrite => "workspace-write",
    };
    let thread = json!({"cwd":cwd,"sandbox":sandbox,"approvalPolicy":"untrusted","model":run.selected_model});
    let policy = match run.execution_policy {
        ExecutionPolicy::ReadOnly => json!({"type":"readOnly","networkAccess":false}),
        ExecutionPolicy::WorkspaceWrite => {
            json!({"type":"workspaceWrite","networkAccess":false,"writableRoots":[cwd]})
        }
    };
    let turn = json!({"threadId":"","input":[{"type":"text","text":instruction}],"cwd":cwd,"model":run.selected_model,"effort":run.selected_reasoning_effort,"approvalPolicy":"untrusted","sandboxPolicy":policy});
    (thread, turn)
}
fn provider_execute(
    run: &ExecutionRun,
    cwd: &str,
    instruction: &str,
    cancel: &mpsc::Receiver<()>,
    on_started: impl FnOnce(String, String),
) -> Result<(ExecutionStatus, String), ProviderError> {
    let mut rpc = RpcProcess::start()?;
    let outcome = (|| {
        rpc.request("initialize",json!({"clientInfo":{"name":"project-orchestrator","version":"0.3.0"},"capabilities":{}}),REQUEST_TIMEOUT)?;
        if let Some(model) = &run.selected_model {
            let catalog =
                normalize_models(&rpc.request("model/list", json!({}), REQUEST_TIMEOUT)?);
            let m = catalog
                .iter()
                .find(|m| &m.id == model)
                .ok_or(ProviderError::Failed)?;
            if run
                .selected_reasoning_effort
                .as_ref()
                .is_some_and(|e| !m.reasoning_efforts.iter().any(|x| x == e))
            {
                return Err(ProviderError::Failed);
            }
        }
        let (thread_params, mut turn_params) = payload(run, cwd, instruction);
        let thread = rpc.request("thread/start", thread_params, REQUEST_TIMEOUT)?;
        let tid = thread
            .pointer("/thread/id")
            .and_then(Value::as_str)
            .ok_or(ProviderError::Protocol)?
            .to_owned();
        turn_params["threadId"] = json!(tid.clone());
        let turn = rpc.request("turn/start", turn_params, REQUEST_TIMEOUT)?;
        let turn_id = turn
            .pointer("/turn/id")
            .and_then(Value::as_str)
            .ok_or(ProviderError::Protocol)?
            .to_owned();
        on_started(tid.clone(), turn_id.clone());
        let began = Instant::now();
        let mut cancel_requested = false;
        loop {
            if began.elapsed() > RUN_TIMEOUT {
                return Err(ProviderError::Timeout);
            }
            if !cancel_requested && cancel.try_recv().is_ok() {
                cancel_requested = true;
                let _ = rpc.request(
                    "turn/interrupt",
                    json!({"threadId":tid,"turnId":turn_id}),
                    CANCEL_TIMEOUT,
                );
            }
            match rpc.notification(Duration::from_millis(250)) {
                Ok(Some(n))
                    if n.get("id").is_some()
                        && n.get("method").and_then(Value::as_str)
                            == Some("item/commandExecution/requestApproval") =>
                {
                    // The user already selected this bounded sandbox at Run time. A command
                    // approval cannot broaden its cwd, writable roots, or disabled network.
                    rpc.respond(n.get("id").unwrap(), json!({"decision":"accept"}))?;
                }
                Ok(Some(n))
                    if n.get("id").is_some()
                        && n.get("method").and_then(Value::as_str)
                            == Some("item/fileChange/requestApproval") =>
                {
                    let decision = if run.execution_policy == ExecutionPolicy::WorkspaceWrite {
                        "accept"
                    } else {
                        "decline"
                    };
                    rpc.respond(n.get("id").unwrap(), json!({"decision":decision}))?;
                }
                Ok(Some(n))
                    if n.get("id").is_some()
                        && n.get("method").and_then(Value::as_str)
                            == Some("item/permissions/requestApproval") =>
                {
                    rpc.respond(n.get("id").unwrap(), json!({"permissions":{"fileSystem":{"entries":[]},"network":{"enabled":false}}}))?;
                }
                Ok(Some(n))
                    if n.get("id").is_some()
                        && n.get("method").and_then(Value::as_str)
                            == Some("execCommandApproval") =>
                {
                    rpc.respond(n.get("id").unwrap(), json!({"decision":"approved"}))?;
                }
                Ok(Some(n))
                    if n.get("id").is_some()
                        && n.get("method").and_then(Value::as_str)
                            == Some("applyPatchApproval") =>
                {
                    let decision = if run.execution_policy == ExecutionPolicy::WorkspaceWrite {
                        "approved"
                    } else {
                        "abort"
                    };
                    rpc.respond(n.get("id").unwrap(), json!({"decision":decision}))?;
                }
                Ok(Some(n))
                    if n.get("method").and_then(Value::as_str) == Some("turn/completed") =>
                {
                    let p = n.get("params").unwrap_or(&Value::Null);
                    if p.get("threadId").and_then(Value::as_str) != Some(&tid)
                        || p.pointer("/turn/id").and_then(Value::as_str) != Some(&turn_id)
                    {
                        continue;
                    }
                    let t = p.get("turn").ok_or(ProviderError::Protocol)?;
                    return Ok(match t.get("status").and_then(Value::as_str) {
                        Some("completed") => (ExecutionStatus::Succeeded, visible_result(t)),
                        Some("interrupted") if cancel_requested => (
                            ExecutionStatus::Cancelled,
                            "Execution cancelled by the user.".into(),
                        ),
                        Some("interrupted") => (
                            ExecutionStatus::Interrupted,
                            "Codex interrupted the execution.".into(),
                        ),
                        Some("failed") => (
                            ExecutionStatus::Failed,
                            "Codex reported that execution failed.".into(),
                        ),
                        _ => (
                            ExecutionStatus::Failed,
                            "Codex returned an unsupported terminal status.".into(),
                        ),
                    });
                }
                Ok(_) => {}
                Err(ProviderError::Timeout) => {}
                Err(e) => return Err(e),
            }
        }
    })();
    rpc.terminate();
    outcome
}

mod commands {
    use super::*;
    use tauri::{Manager, State};
    fn data_root(app: &tauri::AppHandle) -> Result<PathBuf, StoreError> {
        if let Some(configured) = std::env::var_os("PROJECT_ORCHESTRATOR_DATA_DIR") {
            let configured = PathBuf::from(configured);
            if !configured.is_absolute() {
                return Err(StoreError::Unavailable);
            }
            let root = fs::canonicalize(configured).map_err(|_| StoreError::Unavailable)?;
            if !root.is_dir() {
                return Err(StoreError::Unavailable);
            }
            return Ok(root);
        }
        app.path()
            .app_data_dir()
            .map_err(|_| StoreError::Unavailable)
    }
    fn path(app: &tauri::AppHandle) -> Result<PathBuf, StoreError> {
        Ok(state_file(&data_root(app)?))
    }
    fn gate<'a>(app: &'a tauri::AppHandle) -> State<'a, StoreGate> {
        app.state()
    }
    fn read(app: &tauri::AppHandle) -> Result<OrchestrationState, String> {
        let p = path(app).map_err(|e| e.frontend_message())?;
        gate(app).read(&p).map_err(|e| e.frontend_message().into())
    }
    #[tauri::command(rename = "get_release_info")]
    pub fn release_info(app: tauri::AppHandle) -> Result<ReleaseInfo, String> {
        let data_location =
            data_root(&app).map_err(|_| "Application data location is unavailable.")?;
        Ok(ReleaseInfo {
            version: env!("CARGO_PKG_VERSION").into(),
            codex_version: local_codex_version(),
            data_location: data_location.to_string_lossy().into_owned(),
        })
    }
    #[tauri::command(rename = "complete_onboarding")]
    pub fn complete_onboarding(app: tauri::AppHandle) -> Result<OrchestrationState, String> {
        mutate(&app, |s| {
            set_onboarding(s, true);
            Ok(())
        })
    }
    #[tauri::command(rename = "reset_onboarding")]
    pub fn reset_onboarding(app: tauri::AppHandle) -> Result<OrchestrationState, String> {
        mutate(&app, |s| {
            set_onboarding(s, false);
            Ok(())
        })
    }
    fn mutate<F>(app: &tauri::AppHandle, f: F) -> Result<OrchestrationState, String>
    where
        F: FnOnce(&mut OrchestrationState) -> Result<(), StoreError>,
    {
        let p = path(app).map_err(|e| e.frontend_message())?;
        gate(app)
            .mutate(&p, f)
            .map_err(|e| e.frontend_message().into())
    }
    fn persist_worker(app: &tauri::AppHandle, h: &ProviderHealth) {
        let _ = mutate(app, |s| {
            let changed = s.worker.status != h.status;
            s.worker = LocalWorkerStatus {
                id: h.id.clone(),
                display_name: h.display_name.clone(),
                status: h.status.clone(),
                message: h.message.clone(),
                checked_at: h.checked_at.clone(),
            };
            if changed {
                record(s, "worker.health_changed", None, None)
            }
            Ok(())
        });
    }
    fn finish(app: &tauri::AppHandle, id: &str, status: ExecutionStatus, msg: String) {
        let succeeded = status == ExecutionStatus::Succeeded;
        let failed = status == ExecutionStatus::Failed;
        let _ = mutate(app, |s| {
            update_run(
                s,
                id,
                status,
                if succeeded { Some(msg.clone()) } else { None },
                if failed { Some(msg) } else { None },
            )
        });
    }
    fn spawn(app: tauri::AppHandle, run: ExecutionRun, receiver: mpsc::Receiver<()>) {
        tauri::async_runtime::spawn_blocking(move || {
            let _ = mutate(&app, |s| {
                update_run(s, &run.id, ExecutionStatus::Starting, None, None)
            });
            let snapshot = read(&app);
            let (cwd, instruction) = match snapshot.and_then(|s| {
                let p = s
                    .projects
                    .iter()
                    .find(|p| p.id == run.project_id)
                    .ok_or_else(|| "missing".to_owned())?;
                let t = s
                    .tasks
                    .iter()
                    .find(|t| t.id == run.task_id)
                    .ok_or_else(|| "missing".to_owned())?;
                let c = valid_directory(&p.local_path).map_err(|_| "missing".to_owned())?;
                if p.archived
                    || !c.eq_ignore_ascii_case(&p.local_path)
                    || t.status != TaskStatus::Approved
                {
                    return Err("missing".to_owned());
                }
                Ok((c, t.instruction.clone()))
            }) {
                Ok(v) => v,
                Err(_) => {
                    finish(
                        &app,
                        &run.id,
                        ExecutionStatus::Failed,
                        "The registered project is no longer available.".into(),
                    );
                    return;
                }
            };
            let aid = app.clone();
            let rid = run.id.clone();
            let res = provider_execute(&run, &cwd, &instruction, &receiver, move |thread, turn| {
                let _ = mutate(&aid, |s| {
                    set_provider_ids(s, &rid, thread, turn)?;
                    update_run(s, &rid, ExecutionStatus::Running, None, None)
                });
            });
            match res {
                Ok((status, msg)) => finish(&app, &run.id, status, msg),
                Err(e) => finish(
                    &app,
                    &run.id,
                    if receiver.try_recv().is_ok() {
                        ExecutionStatus::Cancelled
                    } else {
                        ExecutionStatus::Failed
                    },
                    e.execution_message().into(),
                ),
            }
            if let Ok(mut controls) = app.state::<RunControls>().0.lock() {
                controls.remove(&run.id);
            }
        });
    }
    #[tauri::command(rename = "get_orchestration_state")]
    pub fn get(app: tauri::AppHandle) -> Result<OrchestrationState, String> {
        read(&app)
    }
    #[tauri::command(rename = "create_project")]
    pub fn create_project_command(
        app: tauri::AppHandle,
        display_name: String,
        local_path: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| create_project(s, display_name, local_path))
    }
    #[tauri::command(rename = "update_project")]
    pub fn update_project_command(
        app: tauri::AppHandle,
        id: String,
        display_name: String,
        local_path: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| update_project(s, &id, display_name, local_path))
    }
    #[tauri::command(rename = "archive_project")]
    pub fn archive_project_command(
        app: tauri::AppHandle,
        id: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| archive_project(s, &id))
    }
    #[tauri::command(rename = "create_task")]
    pub fn create_task_command(
        app: tauri::AppHandle,
        project_id: String,
        title: String,
        instruction: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| create_task(s, project_id, title, instruction))
    }
    #[tauri::command(rename = "update_task")]
    pub fn update_task_command(
        app: tauri::AppHandle,
        id: String,
        title: String,
        instruction: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| update_task(s, &id, title, instruction))
    }
    #[tauri::command(rename = "submit_task")]
    pub fn submit_task_command(
        app: tauri::AppHandle,
        id: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| {
            transition_task(s, &id, TaskStatus::PendingApproval)
        })
    }
    #[tauri::command(rename = "approve_task")]
    pub fn approve_task_command(
        app: tauri::AppHandle,
        id: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| transition_task(s, &id, TaskStatus::Approved))
    }
    #[tauri::command(rename = "cancel_task")]
    pub fn cancel_task_command(
        app: tauri::AppHandle,
        id: String,
    ) -> Result<OrchestrationState, String> {
        mutate(&app, |s| transition_task(s, &id, TaskStatus::Cancelled))
    }
    #[tauri::command(rename = "run_task")]
    pub fn run_task(
        app: tauri::AppHandle,
        task_id: String,
        model: Option<String>,
        reasoning_effort: Option<String>,
        policy: ExecutionPolicy,
    ) -> Result<OrchestrationState, String> {
        let mut created = None;
        let state = mutate(&app, |s| {
            created = Some(start_run(s, &task_id, model, reasoning_effort, policy)?);
            Ok(())
        })?;
        if let Some(run) = created {
            let (tx, rx) = mpsc::channel();
            app.state::<RunControls>()
                .0
                .lock()
                .map_err(|_| "Execution control is temporarily unavailable.")?
                .insert(run.id.clone(), tx);
            spawn(app, run, rx);
        }
        Ok(state)
    }
    #[tauri::command(rename = "retry_execution")]
    pub fn retry(app: tauri::AppHandle, run_id: String) -> Result<OrchestrationState, String> {
        let mut created = None;
        let state = mutate(&app, |s| {
            created = Some(retry_run(s, &run_id)?);
            Ok(())
        })?;
        if let Some(run) = created {
            let (tx, rx) = mpsc::channel();
            app.state::<RunControls>()
                .0
                .lock()
                .map_err(|_| "Execution control is temporarily unavailable.")?
                .insert(run.id.clone(), tx);
            spawn(app, run, rx);
        }
        Ok(state)
    }
    #[tauri::command(rename = "cancel_execution")]
    pub fn cancel(app: tauri::AppHandle, run_id: String) -> Result<OrchestrationState, String> {
        let state = read(&app)?;
        if !state
            .runs
            .iter()
            .any(|r| r.id == run_id && r.status.active())
        {
            return Err("That execution run is no longer active.".into());
        }
        let tx = app
            .state::<RunControls>()
            .0
            .lock()
            .map_err(|_| "Execution control is temporarily unavailable.")?
            .get(&run_id)
            .cloned()
            .ok_or_else(|| "The active execution is no longer attached.".to_owned())?;
        let _ = tx.send(());
        Ok(state)
    }
    #[tauri::command(rename = "refresh_telemetry")]
    pub async fn refresh(app: tauri::AppHandle) -> TelemetrySnapshot {
        let t = now();
        let s = match tauri::async_runtime::spawn_blocking(read_live_telemetry).await {
            Ok(Ok(s)) => s,
            _ => TelemetrySnapshot {
                health: ProviderHealth {
                    id: "codex-local".into(),
                    display_name: "Codex Local".into(),
                    status: "error".into(),
                    message: "Codex telemetry is temporarily unavailable. Please retry.".into(),
                    checked_at: Some(t.clone()),
                },
                quotas: vec![],
                models: vec![],
                activity: vec![ActivityEvent {
                    id: "error".into(),
                    kind: "error".into(),
                    message: "Codex telemetry could not be refreshed".into(),
                    at: t,
                }],
                provenance: "live".into(),
            },
        };
        persist_worker(&app, &s.health);
        s
    }
    pub fn reconcile(app: &tauri::AppHandle) {
        let _ = mutate(app, |s| {
            reconcile_runs(s);
            Ok(())
        });
    }
}
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(StoreGate::new())
        .manage(RunControls::new())
        .setup(|a| {
            commands::reconcile(&a.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::refresh,
            commands::get,
            commands::release_info,
            commands::complete_onboarding,
            commands::reset_onboarding,
            commands::create_project_command,
            commands::update_project_command,
            commands::archive_project_command,
            commands::create_task_command,
            commands::update_task_command,
            commands::submit_task_command,
            commands::approve_task_command,
            commands::cancel_task_command,
            commands::run_task,
            commands::retry,
            commands::cancel
        ])
        .run(tauri::generate_context!())
        .expect("error while running project orchestrator");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn approved(root: &Path) -> OrchestrationState {
        fs::create_dir_all(root).unwrap();
        let mut s = OrchestrationState::default();
        create_project(&mut s, "P".into(), root.to_string_lossy().into_owned()).unwrap();
        let p = s.projects[0].id.clone();
        create_task(&mut s, p, "T".into(), "Return READY".into()).unwrap();
        let t = s.tasks[0].id.clone();
        transition_task(&mut s, &t, TaskStatus::PendingApproval).unwrap();
        transition_task(&mut s, &t, TaskStatus::Approved).unwrap();
        s
    }
    #[test]
    fn unicode_and_error_sanitization() {
        assert_eq!(bounded("→ tiếng Việt"), "→ tiếng Việt");
        assert!(!ProviderError::Unavailable
            .execution_message()
            .contains("token"));
    }
    #[test]
    fn prior_state_migrates_with_onboarding_incomplete_and_data_preserved() {
        let root = std::env::temp_dir().join(stable_id("test"));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("s.json");
        fs::write(&file,r#"{"version":1,"projects":[],"tasks":[],"activity":[],"worker":{"id":"c","displayName":"c","status":"ok","message":"ok","checkedAt":null}}"#).unwrap();
        let migrated = read_state_from(&file).unwrap();
        assert_eq!(migrated.version, 3);
        assert!(!migrated.settings.onboarding_completed);
        fs::write(&file,r#"{"version":2,"projects":[],"tasks":[],"activity":[],"runs":[],"worker":{"id":"c","displayName":"c","status":"ok","message":"ok","checkedAt":null}}"#).unwrap();
        let migrated = read_state_from(&file).unwrap();
        assert_eq!(migrated.version, 3);
        assert!(
            migrated.projects.is_empty() && migrated.tasks.is_empty() && migrated.runs.is_empty()
        );
        assert!(!migrated.settings.onboarding_completed);
        fs::write(&file, "bad").unwrap();
        assert!(matches!(read_state_from(&file), Err(StoreError::Malformed)));
        assert_eq!(fs::read_to_string(&file).unwrap(), "bad");
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn onboarding_completion_and_replay_preserve_orchestration_data() {
        let root = std::env::temp_dir().join(stable_id("test"));
        let mut s = approved(&root);
        let project_count = s.projects.len();
        let task_count = s.tasks.len();
        set_onboarding(&mut s, true);
        assert!(s.settings.onboarding_completed);
        set_onboarding(&mut s, false);
        assert!(!s.settings.onboarding_completed);
        assert_eq!(s.projects.len(), project_count);
        assert_eq!(s.tasks.len(), task_count);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn concurrent_mutation_preserves_both_events() {
        let root = std::env::temp_dir().join(stable_id("test"));
        fs::create_dir_all(&root).unwrap();
        let file = root.join("s.json");
        let gate = Arc::new(StoreGate::new());
        let mut js = vec![];
        for kind in ["one", "two"] {
            let g = gate.clone();
            let f = file.clone();
            js.push(std::thread::spawn(move || {
                g.mutate(&f, |s| {
                    record(s, kind, None, None);
                    Ok(())
                })
                .unwrap()
            }));
        }
        for j in js {
            j.join().unwrap();
        }
        assert_eq!(read_state_from(&file).unwrap().activity.len(), 2);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn run_authorization_and_path_are_enforced() {
        let root = std::env::temp_dir().join(stable_id("test"));
        let mut s = approved(&root);
        let id = s.tasks[0].id.clone();
        s.tasks[0].status = TaskStatus::Draft;
        assert!(matches!(
            start_run(&mut s, &id, None, None, ExecutionPolicy::ReadOnly),
            Err(StoreError::RunNotAllowed)
        ));
        s.tasks[0].status = TaskStatus::Approved;
        s.projects[0].local_path = "C:\\missing".into();
        assert!(matches!(
            start_run(&mut s, &id, None, None, ExecutionPolicy::ReadOnly),
            Err(StoreError::RunNotAllowed)
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn lifecycle_retry_and_terminal_rules() {
        let root = std::env::temp_dir().join(stable_id("test"));
        let mut s = approved(&root);
        let id = s.tasks[0].id.clone();
        let first = start_run(&mut s, &id, None, None, ExecutionPolicy::ReadOnly).unwrap();
        assert!(matches!(
            start_run(&mut s, &id, None, None, ExecutionPolicy::ReadOnly),
            Err(StoreError::ActiveRun)
        ));
        update_run(&mut s, &first.id, ExecutionStatus::Starting, None, None).unwrap();
        assert!(s.runs[0].started_at.is_none());
        update_run(&mut s, &first.id, ExecutionStatus::Running, None, None).unwrap();
        assert!(s.runs[0].started_at.is_some());
        update_run(
            &mut s,
            &first.id,
            ExecutionStatus::Failed,
            None,
            Some("x".into()),
        )
        .unwrap();
        let second = retry_run(&mut s, &first.id).unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(s.runs.len(), 2);
        assert_eq!(
            s.activity
                .iter()
                .filter(|e| e.event_type == "execution.started")
                .count(),
            1
        );
        assert_eq!(
            s.activity
                .iter()
                .filter(|e| e.event_type == "execution.failed")
                .count(),
            1
        );
        assert_eq!(
            s.activity
                .iter()
                .filter(|e| e.event_type == "execution.retried")
                .count(),
            1
        );
        assert!(matches!(
            update_run(
                &mut s,
                &first.id,
                ExecutionStatus::Succeeded,
                Some("x".into()),
                None
            ),
            Err(StoreError::RunNotActive)
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn cancellation_lifecycle_is_terminal_and_preserves_attempt() {
        let root = std::env::temp_dir().join(stable_id("test"));
        let mut s = approved(&root);
        let task = s.tasks[0].id.clone();
        let run = start_run(&mut s, &task, None, None, ExecutionPolicy::ReadOnly).unwrap();
        update_run(&mut s, &run.id, ExecutionStatus::Starting, None, None).unwrap();
        update_run(&mut s, &run.id, ExecutionStatus::Cancelled, None, None).unwrap();
        assert_eq!(s.runs[0].status, ExecutionStatus::Cancelled);
        assert!(matches!(
            update_run(&mut s, &run.id, ExecutionStatus::Running, None, None),
            Err(StoreError::RunNotActive)
        ));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn restart_reconciles_once() {
        let root = std::env::temp_dir().join(stable_id("test"));
        let mut s = approved(&root);
        let id = s.tasks[0].id.clone();
        start_run(&mut s, &id, None, None, ExecutionPolicy::ReadOnly).unwrap();
        reconcile_runs(&mut s);
        assert_eq!(s.runs[0].status, ExecutionStatus::Interrupted);
        let n = s.activity.len();
        reconcile_runs(&mut s);
        assert_eq!(s.activity.len(), n);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn safe_payload_has_no_network_or_dangerous_policy() {
        let root = std::env::temp_dir().join(stable_id("test"));
        let s = approved(&root);
        let r = ExecutionRun {
            id: "r".into(),
            task_id: s.tasks[0].id.clone(),
            project_id: s.projects[0].id.clone(),
            worker_id: "c".into(),
            status: ExecutionStatus::Queued,
            selected_model: None,
            selected_reasoning_effort: None,
            execution_policy: ExecutionPolicy::WorkspaceWrite,
            provider_thread_id: None,
            provider_turn_id: None,
            created_at: now(),
            started_at: None,
            finished_at: None,
            summary: None,
            error: None,
        };
        let (a, b) = payload(&r, &s.projects[0].local_path, "x");
        assert_eq!(a["sandbox"], "workspace-write");
        assert_eq!(
            b.pointer("/sandboxPolicy/networkAccess"),
            Some(&json!(false))
        );
        assert!(!a.to_string().contains("danger"));
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn protocol_data_is_correlated_and_bounded() {
        let n = json!({"jsonrpc":"2.0","method":"turn/completed","params":{"threadId":"t","turn":{"id":"u","status":"completed","items":[{"type":"agentMessage","text":"READY"}]}}});
        assert_eq!(n["params"]["turn"]["id"], "u");
        assert_eq!(visible_result(&n["params"]["turn"]), "READY");
    }
    #[test]
    fn provider_visible_output_is_bounded_and_malformed_json_does_not_persist() {
        let large = "x".repeat(MAX_RESULT_CHARS + 20);
        assert_eq!(bounded(&large).chars().count(), MAX_RESULT_CHARS);
        assert!(serde_json::from_str::<Value>("not-json").is_err());
        assert!(!ProviderError::Protocol
            .execution_message()
            .contains("not-json"));
    }
}
