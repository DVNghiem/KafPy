use std::cmp;
use std::collections::HashMap;
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;

use arc_swap::ArcSwap;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

// Re-export log macros for centralized logging imports
pub use log::{debug, error, info, trace, warn};

/// Capacity of the async log channel. Records are dropped (not blocked) when full.
const LOG_CHANNEL_CAPACITY: usize = 4096;

#[derive(Clone, Debug)]
pub struct ResetHandle(Arc<ArcSwap<CacheNode>>);

impl ResetHandle {
    /// Reset the internal logger caches.
    pub fn reset(&self) {
        // Overwrite whatever is in the cache directly. This must win in case of any collisions
        self.0.store(Default::default());
    }
}

/// What the [`Logger`] can cache.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
#[derive(Default)]
pub enum Caching {
    /// Disables caching.
    /// the message shall be logged.
    Nothing,

    /// Caches the Python `Logger` objects.
    Loggers,

    /// Caches both the Python `Logger` and their respective effective log levels.
    #[default]
    LoggersAndLevels,
}

#[derive(Debug)]
struct CacheEntry {
    filter: LevelFilter,
    logger: Py<PyAny>,
}

impl CacheEntry {
    fn clone_ref(&self, py: Python<'_>) -> Self {
        CacheEntry {
            filter: self.filter,
            logger: self.logger.clone_ref(py),
        }
    }
}

#[derive(Debug, Default)]
struct CacheNode {
    local: Option<CacheEntry>,
    children: HashMap<String, Arc<CacheNode>>,
}

impl CacheNode {
    fn store_to_cache_recursive<'a, P>(
        &self,
        py: Python<'_>,
        mut path: P,
        entry: CacheEntry,
    ) -> Arc<Self>
    where
        P: Iterator<Item = &'a str>,
    {
        let mut me = CacheNode {
            children: self.children.clone(),
            local: self.local.as_ref().map(|e| e.clone_ref(py)),
        };
        match path.next() {
            Some(segment) => {
                let child = me.children.entry(segment.to_owned()).or_default();
                *child = child.store_to_cache_recursive(py, path, entry);
            }
            None => me.local = Some(entry),
        }
        Arc::new(me)
    }
}

/// Owned log record sent over the async channel.
struct OwnedRecord {
    level: Level,
    target: String,
    message: String,
    file: Option<String>,
    line: Option<u32>,
}

/// Shared state accessed by both the [`Logger`] builder and the background worker.
struct LoggerState {
    top_filter: LevelFilter,
    filters: HashMap<String, LevelFilter>,
    prefix: Option<String>,
    logging: Py<PyModule>,
    caching: Caching,
    cache: Arc<ArcSwap<CacheNode>>,
}

impl LoggerState {
    fn lookup(&self, target: &str) -> Option<Arc<CacheNode>> {
        if self.caching == Caching::Nothing {
            return None;
        }
        let root = self.cache.load();
        let mut node: &Arc<CacheNode> = &root;
        for segment in target.split("::") {
            match node.children.get(segment) {
                Some(sub) => node = sub,
                None => return None,
            }
        }
        Some(Arc::clone(node))
    }

    fn filter_for(&self, target: &str) -> LevelFilter {
        let mut start = 0;
        let mut filter = self.top_filter;
        while let Some(end) = target[start..].find("::") {
            if let Some(f) = self.filters.get(&target[..start + end]) {
                filter = *f;
            }
            start += end + 2;
        }
        if let Some(f) = self.filters.get(target) {
            filter = *f;
        }
        filter
    }

    fn enabled_inner(&self, metadata: &Metadata, cache: &Option<Arc<CacheNode>>) -> bool {
        let cache_filter = cache
            .as_ref()
            .and_then(|node| node.local.as_ref())
            .map(|local| local.filter)
            .unwrap_or_else(LevelFilter::max);
        metadata.level() <= cache_filter && metadata.level() <= self.filter_for(metadata.target())
    }

    fn store_to_cache(&self, py: Python<'_>, target: &str, entry: CacheEntry) {
        let path = target.split("::");
        let orig = self.cache.load();
        let new = orig.store_to_cache_recursive(py, path, entry);
        self.cache.compare_and_swap(orig, new);
    }

    /// Core Python logging call. Returns a logger object to cache when appropriate.
    fn log_inner(
        &self,
        py: Python<'_>,
        record: &OwnedRecord,
        cache: &Option<Arc<CacheNode>>,
    ) -> PyResult<Option<Py<PyAny>>> {
        let log_level = map_level(record.level);
        let mut target = record.target.replace("::", ".");
        target = match &self.prefix {
            Some(prefix) => format!("{}.{}", prefix, target),
            None => target,
        };

        let cached_logger = cache
            .as_ref()
            .and_then(|node| node.local.as_ref())
            .map(|local| &local.logger);

        let (logger, cached) = match cached_logger {
            Some(cached) => (cached.bind(py).clone(), true),
            None => (
                self.logging
                    .bind(py)
                    .getattr("getLogger")?
                    .call1((&target,))?,
                false,
            ),
        };

        if is_enabled_for(&logger, record.level)? {
            let none = py.None();
            #[allow(unused_mut)]
            let mut extra = py.None().into_bound(py);

            let log_record = logger.call_method1(
                "makeRecord",
                (
                    &target,
                    log_level,
                    record.file.as_deref(),
                    record.line.unwrap_or_default(),
                    &record.message,
                    PyTuple::empty(py), // args
                    &none,              // exc_info
                    &none,              // func
                    extra,              // extra
                ),
            )?;
            logger.call_method1("handle", (log_record,))?;
        }

        let cache_logger = if !cached && self.caching != Caching::Nothing {
            Some(logger.into())
        } else {
            None
        };
        Ok(cache_logger)
    }

    /// Process one record on the background worker thread (called with the GIL held).
    fn process_record(&self, py: Python<'_>, record: OwnedRecord) {
        let cache = self.lookup(&record.target);

        // Preserve any exception that was already set before we entered Python.
        let maybe_existing_exception = PyErr::take(py);

        match self.log_inner(py, &record, &cache) {
            Ok(Some(logger)) => {
                let filter = match self.caching {
                    Caching::Nothing => unreachable!(),
                    Caching::Loggers => LevelFilter::max(),
                    Caching::LoggersAndLevels => {
                        extract_max_level(logger.bind(py)).unwrap_or_else(|e| {
                            e.restore(py);
                            LevelFilter::max()
                        })
                    }
                };
                let entry = CacheEntry { filter, logger };
                self.store_to_cache(py, &record.target, entry);
            }
            Ok(None) => {}
            Err(e) => e.restore(py),
        }

        if let Some(e) = maybe_existing_exception {
            e.restore(py);
        }
    }
}

/// Builder for the async logger.
#[derive(Debug)]
pub struct Logger {
    top_filter: LevelFilter,
    filters: HashMap<String, LevelFilter>,
    prefix: Option<String>,
    logging: Py<PyModule>,
    caching: Caching,
    cache: Arc<ArcSwap<CacheNode>>,
}

impl Logger {
    /// Creates a new logger builder.
    ///
    /// Defaults to [`LevelFilter::Debug`].
    pub fn new(py: Python<'_>, caching: Caching) -> PyResult<Self> {
        let logging = py.import("logging")?;
        Ok(Self {
            top_filter: LevelFilter::Debug,
            filters: HashMap::new(),
            prefix: None,
            logging: logging.into(),
            caching,
            cache: Default::default(),
        })
    }

    /// Installs this logger as the global one.
    pub fn install(self) -> Result<ResetHandle, SetLoggerError> {
        let state = Arc::new(LoggerState {
            top_filter: self.top_filter,
            filters: self.filters,
            prefix: self.prefix,
            logging: self.logging,
            caching: self.caching,
            cache: Arc::clone(&self.cache),
        });

        let level = cmp::max(
            state.top_filter,
            state
                .filters
                .values()
                .copied()
                .max()
                .unwrap_or(LevelFilter::Off),
        );

        let (sender, receiver) = mpsc::sync_channel::<OwnedRecord>(LOG_CHANNEL_CAPACITY);
        let worker_state = Arc::clone(&state);

        std::thread::Builder::new()
            .name("kafpy-log-worker".to_owned())
            .spawn(move || {
                while let Ok(record) = receiver.recv() {
                    Python::attach(|py| worker_state.process_record(py, record));
                }
            })
            .expect("Failed to spawn logging worker thread");

        let handle = ResetHandle(Arc::clone(&state.cache));
        log::set_boxed_logger(Box::new(AsyncLogger { state, sender }))?;
        log::set_max_level(level);
        Ok(handle)
    }

    /// Returns a reset handle without consuming the builder.
    ///
    /// Useful when passing the logger to a composite logging system before installation.
    pub fn reset_handle(&self) -> ResetHandle {
        ResetHandle(Arc::clone(&self.cache))
    }

    /// Sets the default log level filter.
    pub fn filter(mut self, filter: LevelFilter) -> Self {
        self.top_filter = filter;
        self
    }

    pub fn filter_target(mut self, target: String, filter: LevelFilter) -> Self {
        self.filters.insert(target, filter);
        self
    }

    /// Prepends `prefix` to all log target names.
    pub fn set_prefix(mut self, prefix: &str) -> Self {
        self.prefix = Some(prefix.replace("::", "."));
        self
    }
}

impl Default for Logger {
    fn default() -> Self {
        Python::attach(|py| {
            Self::new(py, Caching::LoggersAndLevels).expect("Failed to initialize python logging")
        })
    }
}

/// The installed logger — forwards records to the background worker via a bounded channel.
struct AsyncLogger {
    state: Arc<LoggerState>,
    sender: SyncSender<OwnedRecord>,
}

impl Log for AsyncLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let cache = self.state.lookup(metadata.target());
        self.state.enabled_inner(metadata, &cache)
    }

    fn log(&self, record: &Record) {
        let cache = self.state.lookup(record.target());
        if self.state.enabled_inner(record.metadata(), &cache) {
            let owned = OwnedRecord {
                level: record.level(),
                target: record.target().to_owned(),
                message: format!("{}", record.args()),
                file: record.file().map(str::to_owned),
                line: record.line(),
            };
            // Non-blocking: silently drop the record if the channel is full
            // rather than stalling the calling thread.
            let _ = self.sender.try_send(owned);
        }
    }

    fn flush(&self) {}
}

fn map_level(level: Level) -> usize {
    match level {
        Level::Error => 40,
        Level::Warn => 30,
        Level::Info => 20,
        Level::Debug => 10,
        Level::Trace => 5,
    }
}

fn is_enabled_for(logger: &Bound<'_, PyAny>, level: Level) -> PyResult<bool> {
    let level = map_level(level);
    logger.call_method1("isEnabledFor", (level,))?.is_truthy()
}

fn extract_max_level(logger: &Bound<'_, PyAny>) -> PyResult<LevelFilter> {
    use Level::*;
    for l in &[Trace, Debug, Info, Warn, Error] {
        if is_enabled_for(logger, *l)? {
            return Ok(l.to_level_filter());
        }
    }

    Ok(LevelFilter::Off)
}

/// Installs a default instance of the logger.
pub fn try_init() -> Result<ResetHandle, SetLoggerError> {
    Logger::default().install()
}

/// Similar to [`try_init`], but panics if there's a previous logger already installed.
pub fn init() -> ResetHandle {
    try_init().unwrap()
}
