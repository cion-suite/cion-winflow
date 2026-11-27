//! Импорт закладок Remanga/ExManga в библиотеку текущего пользователя.
//!
//! Импорт долгий: на каждый тайтл уходят запросы в источник, поэтому он
//! выполняется фоновым заданием, а профиль опрашивает его состояние.
//! Задание живёт в памяти процесса: это self-host с одним инстансом, а
//! перезапуск сервера просто прерывает импорт.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex as SyncMutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use serde::Serialize;
use sqlx::SqlitePool;
use tokio::{sync::Semaphore, task::JoinHandle};

use crate::{
    auth::session::{new_secret, now_unix},
    bookmarks::parse::{self, BookmarkItem, BookmarkStatus},
    catalog::{
        chapters::{self, TitleChapterDto},
        selection,
    },
    error::AppError,
    library::service::MAX_TABS,
    source_admin::service::{self as source_admin, ImportInput, ImportResponse},
    source_auth::{SourceAuth, service as source_auth_service},
    source_discovery,
    sources::{Capability, SourceKind, remanga},
    state::AppState,
};

/// Тело multipart чуть больше файла: запас на обёртку.
pub const BODY_LIMIT: usize = 4 * 1024 * 1024;
pub const MAX_FILE_BYTES: usize = 3 * 1024 * 1024;

/// Три одновременных импорта: источник не должен получать лавину запросов,
/// но и ждать по одному тайтлу на 800 закладок слишком долго.
const IMPORT_CONCURRENCY: usize = 3;
/// Бюджет поиска на других источниках для одной пропавшей закладки: даже
/// при двух медленных источниках он завершится и не заблокирует слот.
const FALLBACK_TIMEOUT_SECS: u64 = 45;
/// Ошибки храним в задании только до этого числа: файл может быть большим.
const MAX_ERRORS: usize = 200;
const MAX_ERROR_CHARS: usize = 300;

/// Порядковый номер задания: `started_at` в секундах не различает два
/// импорта, запущенных подряд, а «последнее задание» должно быть точным.
static JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportState {
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportErrorDto {
    pub name: String,
    pub message: String,
    /// Машинный код ошибки API для локализации в UI.
    pub code: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ImportJobDto {
    pub id: String,
    pub state: ImportState,
    pub total: usize,
    pub processed: usize,
    pub succeeded: usize,
    pub failed: usize,
    /// Сколько новых тайтлов появилось на сервере (не «уже были»).
    pub titles_imported: usize,
    pub library_created: usize,
    pub library_updated: usize,
    /// Сколько новых отметок прочитанного записано.
    pub read_marked: i64,
    pub errors: Vec<ImportErrorDto>,
    pub errors_truncated: bool,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

struct ImportJob {
    id: String,
    /// Монотонный порядок запуска: по нему `latest` выбирает задание.
    sequence: u64,
    user_id: i64,
    state: ImportState,
    total: usize,
    processed: usize,
    succeeded: usize,
    failed: usize,
    titles_imported: usize,
    library_created: usize,
    library_updated: usize,
    read_marked: i64,
    errors: Vec<ImportErrorDto>,
    errors_truncated: bool,
    started_at: i64,
    finished_at: Option<i64>,
}

impl ImportJob {
    fn dto(&self) -> ImportJobDto {
        ImportJobDto {
            id: self.id.clone(),
            state: self.state,
            total: self.total,
            processed: self.processed,
            succeeded: self.succeeded,
            failed: self.failed,
            titles_imported: self.titles_imported,
            library_created: self.library_created,
            library_updated: self.library_updated,
            read_marked: self.read_marked,
            errors: self.errors.clone(),
            errors_truncated: self.errors_truncated,
            started_at: self.started_at,
            finished_at: self.finished_at,
        }
    }

    fn record_failure(&mut self, name: &str, message: String, code: String) {
        self.failed += 1;
        self.processed += 1;
        if self.errors.len() < MAX_ERRORS {
            self.errors.push(ImportErrorDto {
                // Название тоже из файла: не храним мегабайты в задании.
                name: name.chars().take(MAX_ERROR_CHARS).collect(),
                message: message.chars().take(MAX_ERROR_CHARS).collect(),
                code,
            });
        } else {
            self.errors_truncated = true;
        }
    }

    fn finish(&mut self, state: ImportState) {
        self.state = state;
        self.finished_at = Some(now_unix());
    }
}

/// Задания импорта в памяти процесса. Ключ — случайный id; пользователь
/// видит последнее своё задание, поэтому отдельный маршрут по id не нужен.
#[derive(Default)]
pub struct ImportJobs {
    jobs: SyncMutex<HashMap<String, Arc<SyncMutex<ImportJob>>>>,
}

impl ImportJobs {
    pub fn new() -> Self {
        Self::default()
    }

    fn insert(&self, job: Arc<SyncMutex<ImportJob>>) -> Result<(), AppError> {
        let (job_id, job_user_id) = {
            let job = lock(&job);
            (job.id.clone(), job.user_id)
        };
        let mut jobs = lock(&self.jobs);
        let now = now_unix();

        // Одно активное задание на пользователя: два параллельных импорта
        // писали бы одни и те же записи и удваивали нагрузку на источник.
        for existing in jobs.values() {
            let existing = lock(existing);
            if existing.user_id == job_user_id && existing.state == ImportState::Running {
                return Err(AppError::conflict(
                    "import_in_progress",
                    "another bookmarks import is already running",
                ));
            }
        }

        // Завершённые задания старше суток не нужны; заодно не даём карте
        // расти при частых повторных импортах.
        jobs.retain(|_, existing| {
            let existing = lock(existing);
            existing.state == ImportState::Running
                || existing.finished_at.is_none_or(|at| now - at < 86_400)
        });
        if jobs.len() >= 64 {
            let mut finished: Vec<(i64, String)> = jobs
                .iter()
                .filter_map(|(id, existing)| {
                    let existing = lock(existing);
                    (existing.state != ImportState::Running)
                        .then(|| (existing.started_at, id.clone()))
                })
                .collect();
            finished.sort();
            for (_, id) in finished.into_iter().take(jobs.len().saturating_sub(63)) {
                jobs.remove(&id);
            }
        }

        jobs.insert(job_id, job);
        Ok(())
    }

    /// Есть ли у пользователя выполняющееся задание. Проверка нужна до
    /// разбора файла, чтобы повторная загрузка не тратила силы впустую.
    pub fn has_running(&self, user_id: i64) -> bool {
        let jobs = lock(&self.jobs);
        jobs.values().any(|job| {
            let job = lock(job);
            job.user_id == user_id && job.state == ImportState::Running
        })
    }

    /// Последнее задание пользователя: по нему профиль показывает прогресс
    /// и результат. `None` — импортов ещё не было.
    pub fn latest(&self, user_id: i64) -> Option<ImportJobDto> {
        let mut jobs = lock(&self.jobs);
        let now = now_unix();
        // Чистим при чтении: иначе залежавшееся задание жило бы до
        // следующего импорта, а не сутки.
        jobs.retain(|_, job| {
            let job = lock(job);
            job.state == ImportState::Running || job.finished_at.is_none_or(|at| now - at < 86_400)
        });
        jobs.values()
            .map(|job| lock(job))
            .filter(|job| job.user_id == user_id)
            .max_by_key(|job| job.sequence)
            .map(|job| job.dto())
    }
}

fn lock<T>(mutex: &SyncMutex<T>) -> MutexGuard<'_, T> {
    // Ошибка импорта не должна превращаться в панику фонового задания.
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Clone)]
struct ImportPlan {
    user_id: i64,
    source_code: String,
    /// id строки `sources`; по нему ищем уже импортированный entry.
    source_id: i64,
    adapter: SourceKind,
    /// Учётка инициатора: фоновое задание не имеет сессии, поэтому
    /// резолвим её один раз при старте.
    auth: Option<SourceAuth>,
    custom_tab_ids: HashMap<i64, i64>,
    /// Языки источников для группировки глав страницы тайтла.
    source_langs: HashMap<String, &'static str>,
}

struct ItemOutcome {
    title_imported: bool,
    library_created: bool,
    library_updated: bool,
    read_marked: i64,
}

/// Разбирает файл, создаёт задание и запускает его в фоне. Возвращает
/// снимок задания и `JoinHandle`; обработчик HTTP ручку не ждёт, а тесты
/// могут дождаться завершения детерминированно.
pub async fn start(
    state: &AppState,
    user_id: i64,
    bytes: &[u8],
    source_code: &str,
) -> Result<(ImportJobDto, JoinHandle<()>), AppError> {
    // До разбора multipart: отклонённая повторная загрузка не должна
    // создавать закладки и тратить CPU на JSON.
    if state.bookmark_imports.has_running(user_id) {
        return Err(AppError::conflict(
            "import_in_progress",
            "another bookmarks import is already running",
        ));
    }

    let outcome = parse::parse(bytes)?;

    let (source_id, adapter, source_langs) = {
        let registry = state.sources.read().await;
        let runtime = registry
            .runtime(source_code)
            .ok_or_else(|| AppError::not_found("source_not_found", "unknown source"))?;
        let adapter = runtime.adapter()?.clone();
        let source_langs = registry
            .entries()
            .map(|runtime| (runtime.code.clone(), runtime.descriptor.lang))
            .collect();
        (runtime.id, adapter, source_langs)
    };

    let now = now_unix();

    // Учётка инициатора: импорт идёт в фоне, без сессии.
    let auth = source_auth_service::resolve(state, user_id, source_id).await?;

    let mut job = ImportJob {
        id: new_secret()?,
        sequence: JOB_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        user_id,
        state: ImportState::Running,
        total: outcome.items.len() + outcome.invalid.len(),
        processed: 0,
        succeeded: 0,
        failed: 0,
        titles_imported: 0,
        library_created: 0,
        library_updated: 0,
        read_marked: 0,
        errors: Vec::new(),
        errors_truncated: false,
        started_at: now,
        finished_at: None,
    };
    // Строки, которые не разобрались, сразу считаются ошибками задания.
    for message in outcome.invalid {
        job.record_failure("", message, "bookmarks_invalid".to_owned());
    }

    let job = Arc::new(SyncMutex::new(job));
    // insert атомарно проверяет, что у пользователя нет running-задания:
    // отклонённая повторная загрузка не успеет создать закладки.
    state.bookmark_imports.insert(job.clone())?;

    let custom_tab_ids =
        match ensure_custom_tabs(&state.pool, user_id, source_code, &outcome.items, now).await {
            Ok(ids) => ids,
            Err(error) => {
                let mut job = lock(&job);
                job.record_failure("", error.to_string(), error.code().to_owned());
                job.finish(ImportState::Failed);
                return Err(error);
            }
        };
    let dto = lock(&job).dto();

    let plan = ImportPlan {
        user_id,
        source_code: source_code.to_owned(),
        source_id,
        adapter,
        auth,
        custom_tab_ids,
        source_langs,
    };
    let handle = spawn_import(state.clone(), job, outcome.items, plan);

    Ok((dto, handle))
}

fn spawn_import(
    state: AppState,
    job: Arc<SyncMutex<ImportJob>>,
    items: Vec<BookmarkItem>,
    plan: ImportPlan,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let semaphore = Arc::new(Semaphore::new(IMPORT_CONCURRENCY));
        let mut tasks = tokio::task::JoinSet::new();

        for item in items {
            // Permit берём до spawn: одновременно живёт не больше
            // IMPORT_CONCURRENCY задач, остальные ждут здесь.
            let Ok(permit) = semaphore.clone().acquire_owned().await else {
                break;
            };
            let state = state.clone();
            let plan = plan.clone();
            let job = job.clone();
            tasks.spawn(async move {
                let _permit = permit;
                let name = item.name.clone();
                match process_item(&state, &plan, &item).await {
                    Ok(outcome) => {
                        let mut job = lock(&job);
                        job.processed += 1;
                        job.succeeded += 1;
                        if outcome.title_imported {
                            job.titles_imported += 1;
                        }
                        if outcome.library_created {
                            job.library_created += 1;
                        }
                        if outcome.library_updated {
                            job.library_updated += 1;
                        }
                        job.read_marked += outcome.read_marked;
                    }
                    Err(error) => {
                        tracing::warn!(title = %name, %error, "bookmark import item failed");
                        lock(&job).record_failure(
                            &name,
                            error.to_string(),
                            error.code().to_owned(),
                        );
                    }
                }
            });
        }

        while let Some(result) = tasks.join_next().await {
            if let Err(error) = result {
                tracing::error!(%error, "bookmark import task failed");
                lock(&job).record_failure(
                    "",
                    "import task failed".to_owned(),
                    "internal_error".to_owned(),
                );
            }
        }

        lock(&job).finish(ImportState::Completed);
    })
}

async fn process_item(
    state: &AppState,
    plan: &ImportPlan,
    item: &BookmarkItem,
) -> Result<ItemOutcome, AppError> {
    let (title_id, title_imported) = import_title(state, plan, item).await?;

    let current_reads = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM chapter_reads WHERE user_id = ? AND title_id = ?",
    )
    .bind(plan.user_id)
    .bind(title_id)
    .fetch_one(&state.pool)
    .await?;

    // Своя закладка перекрывает встроенный статус; если закладку создать
    // не удалось (лимит), остаётся встроенный fallback.
    let (status, custom_tab_id) = match item.status {
        BookmarkStatus::Builtin(status) => (status, None),
        BookmarkStatus::Custom(type_id) => (
            item.fallback_status(),
            plan.custom_tab_ids.get(&type_id).copied(),
        ),
    };

    let now = now_unix();
    // High-water монотонен: число из файла может быть больше доступных глав,
    // поэтому оно хранится отдельно от журнала chapter_reads.
    let imported_read_count = item.progress.max(0);
    let existing_imported = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(imported_read_count, 0) FROM library \
         WHERE user_id = ? AND title_id = ?",
    )
    .bind(plan.user_id)
    .bind(title_id)
    .fetch_optional(&state.pool)
    .await?;
    // Статус перезаписываем по эффективному прогрессу: импортированные
    // 930 глав не должны выглядеть как «ещё не читал».
    let effective_reads = current_reads.max(existing_imported.unwrap_or(0));

    let (library_created, library_updated) = match existing_imported {
        None => {
            sqlx::query(
                "INSERT INTO library (user_id, title_id, status, score, custom_tab_id, \
                                      added_at, updated_at, imported_read_count) \
                 VALUES (?, ?, ?, NULL, ?, ?, ?, ?)",
            )
            .bind(plan.user_id)
            .bind(title_id)
            .bind(status)
            .bind(custom_tab_id)
            .bind(now)
            .bind(now)
            .bind(imported_read_count)
            .execute(&state.pool)
            .await?;
            (true, false)
        }
        Some(_) => {
            // Отметка только растёт: ручное снятие глав её не уменьшает.
            sqlx::query(
                "UPDATE library SET imported_read_count = MAX(COALESCE(imported_read_count, 0), ?) \
                 WHERE user_id = ? AND title_id = ?",
            )
            .bind(imported_read_count)
            .bind(plan.user_id)
            .bind(title_id)
            .execute(&state.pool)
            .await?;

            if effective_reads < imported_read_count {
                // Перезаписываем статус, только если в файле прогресс больше
                // уже достигнутого: ручные изменения не откатываются.
                sqlx::query(
                    "UPDATE library SET status = ?, custom_tab_id = ?, updated_at = ? \
                     WHERE user_id = ? AND title_id = ?",
                )
                .bind(status)
                .bind(custom_tab_id)
                .bind(now)
                .bind(plan.user_id)
                .bind(title_id)
                .execute(&state.pool)
                .await?;
                (false, true)
            } else {
                (false, false)
            }
        }
    };

    let read_marked = mark_progress(
        &state.pool,
        plan.user_id,
        title_id,
        item.progress,
        current_reads,
        &plan.source_langs,
    )
    .await?;

    Ok(ItemOutcome {
        title_imported,
        library_created,
        library_updated,
        read_marked,
    })
}

/// Находит или импортирует тайтл. Если `dir` устарел, пробует поиск по
/// названию: это явно разрешено для переноса закладок.
async fn import_title(
    state: &AppState,
    plan: &ImportPlan,
    item: &BookmarkItem,
) -> Result<(i64, bool), AppError> {
    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT title_id FROM source_entries WHERE source_id = ? AND external_id = ?",
    )
    .bind(plan.source_id)
    .bind(&item.dir)
    .fetch_optional(&state.pool)
    .await?;
    if let Some(title_id) = existing {
        return Ok((title_id, false));
    }

    match import_via(state, &plan.source_code, &item.dir, plan.auth.as_ref()).await {
        Ok(response) => Ok((response.title_id, response.title_created)),
        Err(first) if is_absent_at_source(&first) => {
            // Сначала тот же источник: у Remanga `dir` мог устареть, а
            // название — остаться.
            if let Some(external_id) =
                source_discovery::find_external_id(&plan.adapter, &item.name, plan.auth.as_ref())
                    .await
                    .ok()
                    .flatten()
            {
                match import_via(
                    state,
                    &plan.source_code,
                    external_id.as_str(),
                    plan.auth.as_ref(),
                )
                .await
                {
                    Ok(response) => return Ok((response.title_id, response.title_created)),
                    Err(error) => tracing::warn!(
                        title = %item.name,
                        source = %plan.source_code,
                        %error,
                        "bookmark title not imported by name"
                    ),
                }
            }

            // Произведение исчезло с исходного источника, но может быть на
            // другом включённом: закладка всё равно должна попасть в
            // библиотеку со своим статусом и прогрессом. Общий таймаут не
            // даёт зависшему источнику держать один из трёх слотов импорта.
            match tokio::time::timeout(
                Duration::from_secs(FALLBACK_TIMEOUT_SECS),
                import_from_other_sources(state, plan, item),
            )
            .await
            {
                Ok(Ok(Some((title_id, created)))) => return Ok((title_id, created)),
                Ok(Ok(None)) => {}
                Ok(Err(error)) => return Err(error),
                Err(_) => tracing::warn!(
                    title = %item.name,
                    "bookmark fallback timed out"
                ),
            }

            // Ничего не нашли: возвращаем исходную ошибку, чтобы UI сохранил
            // её код (например, «возможно, нужен аккаунт» вместо 404).
            Err(first)
        }
        Err(error) => Err(error),
    }
}

/// Пробует импортировать закладку из другого включённого источника. Ошибки
/// поиска и импорта не отменяют остальные источники; принимается только
/// точное совпадение названия, чтобы не привязать чужое произведение.
/// `Ok(None)` — источникам нечего предложить; `Err` — содержательная ошибка
/// (rate limit, сеть, auth), которую нельзя выдавать за «глава исчезла».
async fn import_from_other_sources(
    state: &AppState,
    plan: &ImportPlan,
    item: &BookmarkItem,
) -> Result<Option<(i64, bool)>, AppError> {
    let mut first_error = None;

    for candidate in fallback_sources(state, plan.source_id).await {
        let auth =
            match source_auth_service::resolve(state, plan.user_id, candidate.source_id).await {
                Ok(auth) => auth,
                Err(error) => {
                    tracing::warn!(
                        title = %item.name,
                        source = %candidate.code,
                        %error,
                        "cannot resolve fallback source credentials"
                    );
                    first_error.get_or_insert(error);
                    continue;
                }
            };

        let external_id =
            match source_discovery::find_external_id(&candidate.adapter, &item.name, auth.as_ref())
                .await
            {
                Ok(Some(external_id)) => external_id,
                Ok(None) => continue,
                Err(error) => {
                    let mapped = error.into_app(auth.is_some());
                    tracing::warn!(
                        title = %item.name,
                        source = %candidate.code,
                        %mapped,
                        "fallback source search failed"
                    );
                    first_error.get_or_insert(mapped);
                    continue;
                }
            };

        match import_via(state, &candidate.code, external_id.as_str(), auth.as_ref()).await {
            Ok(response) => {
                tracing::info!(
                    title = %item.name,
                    source = %candidate.code,
                    title_id = response.title_id,
                    "imported bookmark from another source"
                );
                return Ok(Some((response.title_id, response.title_created)));
            }
            Err(error) => {
                tracing::warn!(
                    title = %item.name,
                    source = %candidate.code,
                    %error,
                    "fallback source import failed"
                );
                first_error.get_or_insert(error);
            }
        }
    }

    match first_error {
        Some(error) => Err(error),
        None => Ok(None),
    }
}

/// Включённый источник, на котором можно поискать исчезнувший тайтл.
struct FallbackSource {
    code: String,
    source_id: i64,
    adapter: SourceKind,
}

/// Остальные включённые источники с поиском и главами в порядке приоритета
/// их языка. Реестр читается без network-await; приоритеты — уже после.
async fn fallback_sources(state: &AppState, exclude_source_id: i64) -> Vec<FallbackSource> {
    let mut candidates: Vec<(FallbackSource, &'static str)> = {
        let registry = state.sources.read().await;
        registry
            .entries()
            .filter(|runtime| {
                runtime.enabled
                    && runtime.id != exclude_source_id
                    && runtime.descriptor.has(Capability::Search)
                    && runtime.descriptor.has(Capability::Chapters)
            })
            .filter_map(|runtime| {
                runtime.adapter().ok().map(|adapter| {
                    (
                        FallbackSource {
                            code: runtime.code.clone(),
                            source_id: runtime.id,
                            adapter: adapter.clone(),
                        },
                        runtime.descriptor.lang,
                    )
                })
            })
            .collect()
    };

    let mut ranks: HashMap<&'static str, HashMap<i64, i64>> = HashMap::new();
    for (_, lang) in &candidates {
        if !ranks.contains_key(lang) {
            ranks.insert(
                *lang,
                selection::load_priorities(&state.pool, Some(lang))
                    .await
                    .unwrap_or_default(),
            );
        }
    }
    candidates.sort_by_key(|(source, lang)| {
        let rank = ranks.get(*lang).map_or(i64::MAX, |ranked| {
            selection::rank_of(ranked, source.source_id)
        });
        (rank, source.code.clone())
    });

    candidates.into_iter().map(|(source, _)| source).collect()
}

async fn import_via(
    state: &AppState,
    code: &str,
    external_id: &str,
    auth: Option<&SourceAuth>,
) -> Result<ImportResponse, AppError> {
    // SQLx открывает транзакцию через отложенный BEGIN: между чтением
    // source_entries и записью параллельный импорт может успеть закоммитить,
    // и SQLite ответит SQLITE_BUSY_SNAPSHOT (busy_timeout его не ждёт).
    // Повторяем операцию целиком: она идемпотентна.
    let mut attempt = 0_u32;
    loop {
        match import_via_once(state, code, external_id, auth).await {
            Err(error) if is_busy(&error) && attempt < 2 => {
                attempt += 1;
                tokio::time::sleep(Duration::from_millis(150 * u64::from(attempt))).await;
            }
            other => return other,
        }
    }
}

async fn import_via_once(
    state: &AppState,
    code: &str,
    external_id: &str,
    auth: Option<&SourceAuth>,
) -> Result<ImportResponse, AppError> {
    let input = |auto_link| ImportInput {
        external_id: external_id.to_owned(),
        title_id: None,
        auto_link,
    };

    match source_admin::import(state, code, input(true), auth).await {
        // Одинаковое название у разных произведений: автослияние нашло
        // тайтл, к которому уже привязан другой entry этого источника.
        // Создаём отдельный тайтл, не сливая произведения.
        Err(AppError::Conflict {
            code: "source_already_attached",
            ..
        }) => source_admin::import(state, code, input(false), auth).await,
        // Та же ситуация, но второй импорт успел вставить entry первым:
        // SQLite сообщает о нарушении уникальности вместо 409.
        Err(error) if is_unique_violation(&error) => {
            source_admin::import(state, code, input(false), auth).await
        }
        // Один пакетный импорт легко упирается в лимит источника. Ждём
        // столько, сколько просит источник, и повторяем один раз.
        Err(AppError::ServiceUnavailable {
            code: "source_rate_limited",
            retry_after_secs,
            ..
        }) => {
            let wait = retry_after_secs.unwrap_or(5).min(120);
            tokio::time::sleep(Duration::from_secs(wait)).await;
            source_admin::import(state, code, input(true), auth).await
        }
        other => other,
    }
}

fn is_unique_violation(error: &AppError) -> bool {
    matches!(
        error,
        AppError::Database(sqlx::Error::Database(db)) if db.is_unique_violation()
    )
}

fn is_busy(error: &AppError) -> bool {
    let AppError::Database(sqlx::Error::Database(db)) = error else {
        return false;
    };
    let coded = db.code();
    let code = coded.as_deref().unwrap_or_default();
    // Основной код SQLITE_BUSY — 5; расширенные (BUSY_RECOVERY, BUSY_SNAPSHOT,
    // BUSY_TIMEOUT) дают тот же остаток по модулю 256.
    let busy_code = code.parse::<i64>().is_ok_and(|value| value % 256 == 5);
    let message = db.message().to_ascii_lowercase();
    busy_code || message.contains("locked") || message.contains("busy")
}

/// Произведения нет на исходном источнике (или оно скрыто без учётки):
/// тогда имеет смысл искать его на других включённых источниках.
fn is_absent_at_source(error: &AppError) -> bool {
    matches!(
        error,
        AppError::NotFound {
            code: "source_not_found",
            ..
        } | AppError::BadRequest {
            code: "source_invalid_external_id",
            ..
        } | AppError::Forbidden {
            code: "source_auth_maybe_required",
            ..
        }
    )
}

/// Отмечает первые `target` доступных глав прочитанными и ставит позицию
/// на последнюю из них: в файле `progress` означает «столько глав read».
/// Закрытые/пропавшие главы не считаем — их не видно и на странице тайтла.
async fn mark_progress(
    pool: &SqlitePool,
    user_id: i64,
    title_id: i64,
    target: i64,
    current_reads: i64,
    source_langs: &HashMap<String, &'static str>,
) -> Result<i64, AppError> {
    if target <= 0 {
        return Ok(0);
    }

    let title = selection::load_title_info(pool, title_id).await?;
    let chapters = chapters::title_chapters(pool, &title, source_langs, None, user_id).await?;
    let mut available: Vec<&TitleChapterDto> = chapters
        .chapters
        .iter()
        .filter(|chapter| chapter.availability != "locked" && chapter.availability != "missing")
        .collect();
    available.sort_by(|left, right| {
        left.volume
            .unwrap_or(0)
            .cmp(&right.volume.unwrap_or(0))
            .then_with(|| {
                left.number_milli
                    .unwrap_or(i64::MAX)
                    .cmp(&right.number_milli.unwrap_or(i64::MAX))
            })
            .then_with(|| left.chapter_key.cmp(&right.chapter_key))
    });

    let wanted = usize::try_from(target).unwrap_or(usize::MAX);
    let count = wanted.min(available.len());
    if count == 0 {
        return Ok(0);
    }

    let now = now_unix();
    let mut tx = pool.begin().await?;
    let mut marked = 0_i64;
    for chapter in &available[..count] {
        let result = sqlx::query(
            "INSERT INTO chapter_reads (user_id, title_id, chapter_key, read_at) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT (user_id, title_id, chapter_key) DO NOTHING",
        )
        .bind(user_id)
        .bind(title_id)
        .bind(&chapter.chapter_key)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        marked += i64::try_from(result.rows_affected()).unwrap_or(0);
    }

    if current_reads < target {
        // Не откатываем позицию назад: пока импорт шёл, пользователь мог
        // читать в другой вкладке и сохранить более позднюю главу. SELECT
        // идёт после первой записи, поэтому транзакция уже write-locked.
        let existing_key: Option<String> = sqlx::query_scalar(
            "SELECT chapter_key FROM progress WHERE user_id = ? AND title_id = ?",
        )
        .bind(user_id)
        .bind(title_id)
        .fetch_optional(&mut *tx)
        .await?;
        let target_index = count - 1;
        let existing_index = existing_key.as_deref().and_then(|key| {
            available
                .iter()
                .position(|chapter| chapter.chapter_key == key)
        });
        if existing_index.is_none_or(|index| index < target_index) {
            // Позиция без page/seconds/scroll: тип контента ещё может быть
            // неизвестен, а читалка сама откроет главу с начала.
            sqlx::query(
                "INSERT INTO progress (user_id, title_id, chapter_key, page, seconds, scroll, updated_at) \
                 VALUES (?, ?, ?, NULL, NULL, NULL, ?) \
                 ON CONFLICT (user_id, title_id) DO UPDATE SET \
                     chapter_key = excluded.chapter_key, page = excluded.page, \
                     seconds = excluded.seconds, scroll = excluded.scroll, \
                     updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(title_id)
            .bind(&available[count - 1].chapter_key)
            .bind(now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    Ok(marked)
}

async fn ensure_custom_tabs(
    pool: &SqlitePool,
    user_id: i64,
    source_code: &str,
    items: &[BookmarkItem],
    now: i64,
) -> Result<HashMap<i64, i64>, AppError> {
    let mut type_ids = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        if let BookmarkStatus::Custom(type_id) = item.status
            && seen.insert(type_id)
        {
            type_ids.push(type_id);
        }
    }

    let mut tabs = HashMap::new();
    for type_id in type_ids {
        match ensure_custom_tab(pool, user_id, source_code, type_id, now).await? {
            Some(tab_id) => {
                tabs.insert(type_id, tab_id);
            }
            None => {
                tracing::warn!(
                    type_id,
                    user_id,
                    "bookmark import: custom tab limit reached"
                );
            }
        }
    }
    Ok(tabs)
}

/// Своя закладка для внешнего статуса. Имя `new1`/`new2`… выбирается
/// свободное, а привязка к источнику хранится в `source_ref`: пользователь
/// может переименовать закладку, повторный импорт её всё равно найдёт.
async fn ensure_custom_tab(
    pool: &SqlitePool,
    user_id: i64,
    source_code: &str,
    type_id: i64,
    now: i64,
) -> Result<Option<i64>, AppError> {
    let source_ref = custom_source_ref(source_code, type_id);
    if let Some(id) = sqlx::query_scalar::<_, i64>(
        "SELECT id FROM library_tabs WHERE user_id = ? AND source_ref = ?",
    )
    .bind(user_id)
    .bind(&source_ref)
    .fetch_optional(pool)
    .await?
    {
        return Ok(Some(id));
    }

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM library_tabs WHERE user_id = ?")
        .bind(user_id)
        .fetch_one(pool)
        .await?;
    if count >= MAX_TABS {
        return Ok(None);
    }

    let names: HashSet<String> =
        sqlx::query_scalar::<_, String>("SELECT name FROM library_tabs WHERE user_id = ?")
            .bind(user_id)
            .fetch_all(pool)
            .await?
            .into_iter()
            .collect();

    let mut number = 1_i64;
    let name = loop {
        let candidate = format!("new{number}");
        if !names.contains(&candidate) {
            break candidate;
        }
        number += 1;
        if number > 1000 {
            return Ok(None);
        }
    };

    let position = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(position) + 1, 0) FROM library_tabs WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await?;

    // ON CONFLICT DO NOTHING страхует от гонки двух стартов импорта:
    // вторая попытка просто перечитает созданную закладку.
    sqlx::query(
        "INSERT INTO library_tabs (user_id, name, position, created_at, source_ref) \
         VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT DO NOTHING",
    )
    .bind(user_id)
    .bind(&name)
    .bind(position)
    .bind(now)
    .bind(&source_ref)
    .execute(pool)
    .await?;

    Ok(sqlx::query_scalar::<_, i64>(
        "SELECT id FROM library_tabs WHERE user_id = ? AND source_ref = ?",
    )
    .bind(user_id)
    .bind(&source_ref)
    .fetch_optional(pool)
    .await?)
}

fn custom_source_ref(source_code: &str, type_id: i64) -> String {
    format!("{source_code}:bookmark:{type_id}")
}

/// Код источника по умолчанию для HTTP-импорта: экспорт Remanga.
pub const SOURCE_CODE: &str = remanga::CODE;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_source_ref_is_stable() {
        assert_eq!(
            custom_source_ref(SOURCE_CODE, 34_956),
            "remanga:bookmark:34956"
        );
    }
}
