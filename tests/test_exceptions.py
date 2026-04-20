"""Tests for kafpy exception hierarchy — ERR-01 through ERR-05."""

from __future__ import annotations

import pytest


class TestExceptionHierarchy:
    """ERR-01: kafpy.exceptions exposes KafPyError, ConsumerError, HandlerError, ConfigurationError."""

    def test_exceptions_module_exports(self):
        """ERR-01: Verify all exception classes are importable from kafpy.exceptions."""
        from kafpy.exceptions import (
            KafPyError,
            ConsumerError,
            HandlerError,
            ConfigurationError,
        )

        assert KafPyError is not None
        assert ConsumerError is not None
        assert HandlerError is not None
        assert ConfigurationError is not None

    def test_kafpy_error_base(self):
        """ERR-01: KafPyError is the base exception."""
        from kafpy.exceptions import KafPyError

        err = KafPyError(message="test error")
        assert isinstance(err, Exception)
        assert str(err) == "test error"

    def test_all_in_kafpy_all(self):
        """ERR-01: All exception classes are in kafpy.exceptions.__all__."""
        from kafpy import exceptions

        for name in ("KafPyError", "ConsumerError", "HandlerError", "ConfigurationError"):
            assert name in exceptions.__all__, f"{name} not in __all__"


class TestExceptionInheritanceChain:
    """ERR-02: Framework errors inherit from KafPyError, not Exception directly."""

    def test_exception_inheritance_chain(self):
        """ERR-02: ConsumerError, HandlerError, ConfigurationError inherit from KafPyError."""
        from kafpy.exceptions import KafPyError, ConsumerError, HandlerError, ConfigurationError

        # All must inherit from KafPyError
        assert issubclass(ConsumerError, KafPyError)
        assert issubclass(HandlerError, KafPyError)
        assert issubclass(ConfigurationError, KafPyError)

        # None inherit from Exception directly (only KafPyError does)
        assert not issubclass(ConsumerError, Exception) or ConsumerError.__mro__[1] == KafPyError
        assert not issubclass(HandlerError, Exception) or HandlerError.__mro__[1] == KafPyError
        assert not issubclass(ConfigurationError, Exception) or ConfigurationError.__mro__[1] == KafPyError

    def test_consumer_error_not_direct_exception(self):
        """ERR-02: ConsumerError does not inherit directly from Exception."""
        from kafpy.exceptions import KafPyError, ConsumerError

        # ConsumerError.__mro__ should be: ConsumerError -> KafPyError -> Exception -> BaseException
        mro = ConsumerError.__mro__
        assert mro[0] == ConsumerError
        assert mro[1] == KafPyError
        assert mro[2] == Exception


class TestStructuredAttributes:
    """ERR-01/ERR-02: Structured attributes (error_code, partition, topic) on all exceptions."""

    def test_consumer_error_structured_fields(self):
        """ERR-01/ERR-02: ConsumerError carries error_code, partition, topic."""
        from kafpy.exceptions import ConsumerError

        err = ConsumerError(
            message="NOT_LEADER",
            error_code=6,
            partition=3,
            topic="test-topic",
        )
        assert err.error_code == 6
        assert err.partition == 3
        assert err.topic == "test-topic"

    def test_handler_error_structured_fields(self):
        """ERR-01/ERR-02: HandlerError carries error_code, partition, topic."""
        from kafpy.exceptions import HandlerError

        err = HandlerError(
            message="invalid key encoding",
            error_code=None,
            partition=1,
            topic="other-topic",
        )
        assert err.error_code is None
        assert err.partition == 1
        assert err.topic == "other-topic"

    def test_configuration_error_structured_fields(self):
        """ERR-01/ERR-02: ConfigurationError carries error_code, partition, topic."""
        from kafpy.exceptions import ConfigurationError

        err = ConfigurationError(
            message="invalid bootstrap_servers",
            error_code=None,
            partition=None,
            topic=None,
        )
        assert err.error_code is None
        assert err.partition is None
        assert err.topic is None


class TestConsumerErrorStrFormat:
    """D-10: ConsumerError str format includes topic@partition when both are present."""

    def test_consumer_error_str_with_topic_partition(self):
        """D-10: ConsumerError.__str__ returns 'message on topic@partition'."""
        from kafpy.exceptions import ConsumerError

        err = ConsumerError(
            message="Consumer error: NOT_LEADER (error 6)",
            error_code=6,
            partition=3,
            topic="test-topic",
        )
        assert str(err) == "Consumer error: NOT_LEADER (error 6) on test-topic@3"

    def test_consumer_error_str_without_topic_partition(self):
        """D-10: ConsumerError.__str__ returns just the message when topic/partition absent."""
        from kafpy.exceptions import ConsumerError

        err = ConsumerError(message="subscription error: unknown group")
        assert str(err) == "subscription error: unknown group"


class TestRustErrorTranslation:
    """ERR-03: Rust errors translated to Python exceptions with context-preserving messages."""

    def test_translate_rust_consumer_error(self):
        """ERR-03: Rust consumer error string maps to ConsumerError with error_code."""
        from kafpy.exceptions import translate_rust_error, ConsumerError

        # D-10 format: "Consumer error: NOT_LEADER (error 6) on topic-0@partition 3"
        result = translate_rust_error(
            module="Consumer",
            error_string="Consumer error: NOT_LEADER (error 6) on topic-0@partition 3",
        )

        assert isinstance(result, ConsumerError)
        assert result.error_code == 6
        assert result.partition == 3
        assert result.topic == "topic-0"
        assert "NOT_LEADER" in result.message

    def test_translate_rust_config_error(self):
        """ERR-03: Rust config error string maps to ConfigurationError."""
        from kafpy.exceptions import translate_rust_error, ConfigurationError

        result = translate_rust_error(
            module="Config",
            error_string="configuration error: missing required field 'bootstrap_servers'",
        )

        assert isinstance(result, ConfigurationError)
        assert result.error_code is None

    def test_translate_rust_unknown_module(self):
        """ERR-03: Unknown Rust module falls back to KafPyError."""
        from kafpy.exceptions import translate_rust_error, KafPyError, ConsumerError

        result = translate_rust_error(
            module="UnknownModule",
            error_string="some error occurred",
        )

        assert isinstance(result, KafPyError)
        assert not isinstance(result, ConsumerError)  # Should be base KafPyError


class TestKafkaMessageAccessErrors:
    """ERR-04: KafkaMessage access errors raise HandlerError, not raw Rust panics."""

    def test_kafka_message_key_none_returns_none(self):
        """D-08: Accessing .key when None returns None silently — no exception."""
        from kafpy.handlers import KafkaMessage

        msg = KafkaMessage(
            topic="test-topic",
            partition=0,
            offset=100,
            key=None,  # No key set
            payload=b"hello",
            headers=[],
            timestamp=0,
        )

        # Accessing .key should return None, not raise
        assert msg.key is None

    def test_kafka_message_wrong_type_raises_handler_error(self):
        """D-09: Wrong-type access raises HandlerError, not raw Rust panics."""
        from kafpy.handlers import KafkaMessage
        from kafpy.exceptions import HandlerError

        # key contains invalid UTF-8 bytes
        msg = KafkaMessage(
            topic="test-topic",
            partition=0,
            offset=100,
            key=b"\xff\xfe invalid",  # Not valid UTF-8
            payload=b"hello",
            headers=[],
            timestamp=0,
        )

        with pytest.raises(HandlerError) as exc_info:
            msg.get_key_as_string()

        assert "key is not valid UTF-8" in str(exc_info.value)
        assert exc_info.value.partition == 0
        assert exc_info.value.topic == "test-topic"

    def test_kafka_message_payload_wrong_type_raises_handler_error(self):
        """D-09: Wrong-type payload access raises HandlerError."""
        from kafpy.handlers import KafkaMessage
        from kafpy.exceptions import HandlerError

        msg = KafkaMessage(
            topic="test-topic",
            partition=0,
            offset=100,
            key=None,
            payload=b"\xc0\xc1 invalid utf-8",  # Not valid UTF-8
            headers=[],
            timestamp=0,
        )

        with pytest.raises(HandlerError) as exc_info:
            msg.get_payload_as_string()

        assert "payload is not valid UTF-8" in str(exc_info.value)

    def test_kafka_message_from_dict(self):
        """KafkaMessage can be constructed from a dict (as Rust passes PyDict)."""
        from kafpy.handlers import KafkaMessage

        data = {
            "topic": "my-topic",
            "partition": 5,
            "offset": 1234,
            "key": b"my-key",
            "payload": b"my-payload",
            "headers": [("content-type", b"application/json")],
            "timestamp": 1700000000000,
        }

        msg = KafkaMessage.from_dict(data)

        assert msg.topic == "my-topic"
        assert msg.partition == 5
        assert msg.offset == 1234
        assert msg.key == b"my-key"
        assert msg.payload == b"my-payload"
        assert msg.timestamp == 1700000000000


class TestPublicApiBoundaries:
    """ERR-05: kafpy.exceptions is the only public import path for exceptions."""

    def test_exceptions_module_not_leaked_from_consumer(self):
        """ERR-05: kafpy.consumer does not expose exception classes directly."""
        import kafpy.consumer as consumer_module

        # consumer module should not expose exceptions directly
        assert not hasattr(consumer_module, "KafPyError")

    def test_kafpy_public_import(self):
        """ERR-05: Exceptions are importable from the top-level kafpy module."""
        import kafpy

        # All exception types must be importable from kafpy directly
        assert kafpy.KafPyError is not None
        assert kafpy.ConsumerError is not None
        assert kafpy.HandlerError is not None
        assert kafpy.ConfigurationError is not None

    def test_exceptions_in_kafpy_all(self):
        """ERR-05: Exception types are in kafpy.__all__."""
        import kafpy

        # Check that exceptions are in __all__
        all_set = set(kafpy.__all__)
        exception_names = {"KafPyError", "ConsumerError", "HandlerError", "ConfigurationError"}
        assert exception_names.issubset(all_set), "Exception types must be in kafpy.__all__"
