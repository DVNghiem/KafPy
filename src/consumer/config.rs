use crate::retry::RetryPolicy;
use crate::routing::config::RoutingRule;
use rdkafka::config::ClientConfig;
use std::time::Duration;

/// Kafka consumer configuration.
///
/// All fields have sensible defaults. Build with [`ConsumerConfigBuilder`].
///
/// # Example
///
/// ```
/// let config = ConsumerConfigBuilder::new()
///     .brokers("localhost:9092")
///     .group_id("my-consumer-group")
///     .topics(["events", "transactions"])
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ConsumerConfig {
    pub brokers: String,
    pub group_id: String,
    pub topics: Vec<String>,
    pub auto_offset_reset: AutoOffsetReset,
    pub enable_auto_commit: bool,
    pub enable_auto_offset_store: bool,
    pub session_timeout_ms: u32,
    pub heartbeat_interval_ms: u32,
    pub max_poll_interval_ms: u32,
    pub security_protocol: Option<String>,
    pub sasl_mechanism: Option<String>,
    pub sasl_username: Option<String>,
    pub sasl_password: Option<String>,
    pub fetch_min_bytes: i32,
    pub max_partition_fetch_bytes: i32,
    pub partition_assignment_strategy: PartitionAssignmentStrategy,
    pub retry_backoff_ms: u32,
    pub default_retry_policy: RetryPolicy,
    pub dlq_topic_prefix: String,
    pub drain_timeout_secs: u64,
    pub routing_rules: Vec<RoutingRule>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum AutoOffsetReset {
    Earliest,
    #[default]
    Latest,
}

impl AutoOffsetReset {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Earliest => "earliest",
            Self::Latest => "latest",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum PartitionAssignmentStrategy {
    #[default]
    RoundRobin,
    Range,
    CooperativeSticky,
}

impl PartitionAssignmentStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RoundRobin => "roundrobin",
            Self::Range => "range",
            Self::CooperativeSticky => "cooperative-sticky",
        }
    }
}

#[derive(Debug, Default)]
pub struct ConsumerConfigBuilder {
    brokers: Option<String>,
    group_id: Option<String>,
    topics: Vec<String>,
    auto_offset_reset: AutoOffsetReset,
    enable_auto_commit: bool,
    enable_auto_offset_store: bool,
    session_timeout_ms: u32,
    heartbeat_interval_ms: u32,
    max_poll_interval_ms: u32,
    security_protocol: Option<String>,
    sasl_mechanism: Option<String>,
    sasl_username: Option<String>,
    sasl_password: Option<String>,
    fetch_min_bytes: i32,
    max_partition_fetch_bytes: i32,
    partition_assignment_strategy: PartitionAssignmentStrategy,
    retry_backoff_ms: u32,
    default_retry_policy: RetryPolicy,
    dlq_topic_prefix: String,
    drain_timeout_secs: u64,
    routing_rules: Vec<RoutingRule>,
}

impl ConsumerConfigBuilder {
    pub fn new() -> Self {
        Self {
            topics: Vec::new(),
            auto_offset_reset: Default::default(),
            enable_auto_commit: false,
            enable_auto_offset_store: false,
            session_timeout_ms: 45000,
            heartbeat_interval_ms: 3000,
            max_poll_interval_ms: 300000,
            fetch_min_bytes: 1048576,
            max_partition_fetch_bytes: 10485760,
            partition_assignment_strategy: PartitionAssignmentStrategy::CooperativeSticky,
            retry_backoff_ms: 100,
            default_retry_policy: RetryPolicy::default(),
            dlq_topic_prefix: "dlq.".to_string(),
            drain_timeout_secs: 30,
            routing_rules: Vec::new(),
            ..Default::default()
        }
    }

    pub fn brokers(mut self, brokers: impl Into<String>) -> Self {
        self.brokers = Some(brokers.into());
        self
    }

    pub fn group_id(mut self, group_id: impl Into<String>) -> Self {
        self.group_id = Some(group_id.into());
        self
    }

    pub fn topics(mut self, topics: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.topics = topics.into_iter().map(|t| t.into()).collect();
        self
    }

    pub fn add_topic(mut self, topic: impl Into<String>) -> Self {
        self.topics.push(topic.into());
        self
    }

    pub fn auto_offset_reset(mut self, reset: AutoOffsetReset) -> Self {
        self.auto_offset_reset = reset;
        self
    }

    pub fn enable_auto_commit(mut self, enabled: bool) -> Self {
        self.enable_auto_commit = enabled;
        self
    }

    pub fn enable_auto_offset_store(mut self, enabled: bool) -> Self {
        self.enable_auto_offset_store = enabled;
        self
    }

    pub fn session_timeout(mut self, timeout: Duration) -> Self {
        self.session_timeout_ms = timeout.as_millis() as u32;
        self
    }

    pub fn heartbeat_interval(mut self, interval: Duration) -> Self {
        self.heartbeat_interval_ms = interval.as_millis() as u32;
        self
    }

    pub fn max_poll_interval(mut self, interval: Duration) -> Self {
        self.max_poll_interval_ms = interval.as_millis() as u32;
        self
    }

    pub fn security_protocol(mut self, protocol: impl Into<String>) -> Self {
        self.security_protocol = Some(protocol.into());
        self
    }

    pub fn sasl_mechanism(
        mut self,
        mechanism: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        self.sasl_mechanism = Some(mechanism.into());
        self.sasl_username = Some(username.into());
        self.sasl_password = Some(password.into());
        self
    }

    pub fn fetch_min_bytes(mut self, bytes: i32) -> Self {
        self.fetch_min_bytes = bytes;
        self
    }

    pub fn max_partition_fetch_bytes(mut self, bytes: i32) -> Self {
        self.max_partition_fetch_bytes = bytes;
        self
    }

    pub fn partition_assignment_strategy(mut self, strategy: PartitionAssignmentStrategy) -> Self {
        self.partition_assignment_strategy = strategy;
        self
    }

    pub fn retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff_ms = backoff.as_millis() as u32;
        self
    }

    pub fn default_retry_policy(mut self, policy: RetryPolicy) -> Self {
        self.default_retry_policy = policy;
        self
    }

    pub fn dlq_topic_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.dlq_topic_prefix = prefix.into();
        self
    }

    pub fn drain_timeout(mut self, secs: u64) -> Self {
        self.drain_timeout_secs = secs;
        self
    }

    /// Adds a routing rule. Rules are evaluated in priority order (lower first).
    ///
    /// # Example
    /// ```ignore
    /// let config = ConsumerConfigBuilder::new()
    ///     .brokers("localhost:9092")
    ///     .group_id("my-group")
    ///     .topics(["events"])
    ///     .routing_rule(
    ///         RoutingRuleBuilder::new()
    ///             .topic_pattern("events.*", PatternType::Glob)
    ///             .to_handler("events-handler")
    ///             .priority(1)
    ///     )
    ///     .build();
    /// ```
    pub fn routing_rule(mut self, rule: RoutingRule) -> Self {
        self.routing_rules.push(rule);
        self
    }

    pub fn build(self) -> Result<ConsumerConfig, BuildError> {
        let brokers = self.brokers.ok_or(BuildError::MissingField("brokers"))?;
        let group_id = self.group_id.ok_or(BuildError::MissingField("group_id"))?;

        if self.topics.is_empty() {
            return Err(BuildError::NoTopics);
        }

        Ok(ConsumerConfig {
            brokers,
            group_id,
            topics: self.topics,
            auto_offset_reset: self.auto_offset_reset,
            enable_auto_commit: self.enable_auto_commit,
            enable_auto_offset_store: self.enable_auto_offset_store,
            session_timeout_ms: self.session_timeout_ms,
            heartbeat_interval_ms: self.heartbeat_interval_ms,
            max_poll_interval_ms: self.max_poll_interval_ms,
            security_protocol: self.security_protocol,
            sasl_mechanism: self.sasl_mechanism,
            sasl_username: self.sasl_username,
            sasl_password: self.sasl_password,
            fetch_min_bytes: self.fetch_min_bytes,
            max_partition_fetch_bytes: self.max_partition_fetch_bytes,
            partition_assignment_strategy: self.partition_assignment_strategy,
            retry_backoff_ms: self.retry_backoff_ms,
            default_retry_policy: self.default_retry_policy,
            dlq_topic_prefix: self.dlq_topic_prefix,
            drain_timeout_secs: self.drain_timeout_secs,
            routing_rules: self.routing_rules,
        })
    }
}

impl ConsumerConfig {
    /// Creates a [`rdkafka::config::ClientConfig`] with all fields applied.
    pub fn into_rdkafka_config(self) -> ClientConfig {
        let mut cfg = ClientConfig::new();

        cfg.set("bootstrap.servers", &self.brokers)
            .set("group.id", &self.group_id)
            .set("auto.offset.reset", self.auto_offset_reset.as_str())
            .set("enable.auto.commit", self.enable_auto_commit.to_string())
            .set(
                "enable.auto.offset.store",
                self.enable_auto_offset_store.to_string(),
            )
            .set("session.timeout.ms", self.session_timeout_ms.to_string())
            .set(
                "heartbeat.interval.ms",
                self.heartbeat_interval_ms.to_string(),
            )
            .set(
                "max.poll.interval.ms",
                self.max_poll_interval_ms.to_string(),
            )
            .set("fetch.min.bytes", self.fetch_min_bytes.to_string())
            .set("enable.partition.eof", "false")
            .set(
                "max.partition.fetch.bytes",
                self.max_partition_fetch_bytes.to_string(),
            )
            .set("fetch.message.max.bytes", self.fetch_min_bytes.to_string())
            .set("queued.min.messages", "100000")
            .set("queued.max.messages.kbytes", "65536")
            .set("fetch.wait.max.ms", "500")
            .set("fetch.error.backoff.ms", "500")
            .set("reconnect.backoff.ms", "100")
            .set("retry.backoff.ms", self.retry_backoff_ms.to_string())
            .set("reconnect.backoff.max.ms", "10000")
            .set(
                "partition.assignment.strategy",
                self.partition_assignment_strategy.as_str(),
            );

        if let Some(ref proto) = self.security_protocol {
            cfg.set("security.protocol", proto);
        }
        if let Some(ref mechanism) = self.sasl_mechanism {
            cfg.set("sasl.mechanism", mechanism);
        }
        if let Some(ref username) = self.sasl_username {
            cfg.set("sasl.username", username);
        }
        if let Some(ref password) = self.sasl_password {
            cfg.set("sasl.password", password);
        }

        cfg
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("missing required field: {0}")]
    MissingField(&'static str),
    #[error("at least one topic must be specified")]
    NoTopics,
}
