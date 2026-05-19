"""Tests for handler registration API.

Verifies that:
- handler() only registers single-message handlers
- batch_handler() is the sole path for batch registration
- async functions are rejected by both decorators
"""

from __future__ import annotations

import pytest
from unittest.mock import MagicMock


class TestHandlerAsyncRejection:
    """async functions must be rejected by handler() and batch_handler()."""

    def test_async_handler_raises_typeerror(self):
        """handler() must reject async functions."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        with pytest.raises(TypeError, match="async handlers are not supported"):
            @app.handler(topic="test")
            async def handle(msg, ctx):
                return None

    def test_async_batch_handler_raises_typeerror(self):
        """batch_handler() must reject async functions."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        with pytest.raises(TypeError, match="async batch handlers are not supported"):
            @app.batch_handler(topic="test")
            async def handle(messages, ctx):
                return None


class TestHandlerRegistration:
    """Sync handler and batch_handler registration smoke tests."""

    def test_sync_handler_registers_as_sync(self):
        """handler() registers a sync function with type='sync'."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        @app.handler(topic="test")
        def handle(msg, ctx):
            return None

        assert "test" in app._handlers
        assert app._handlers["test"]["type"] == "sync"

    def test_batch_handler_registers_as_batch_sync(self):
        """batch_handler() registers a sync function with type='batch_sync'."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        @app.batch_handler(topic="test")
        def handle_batch(messages, ctx):
            return None

        assert "test" in app._handlers
        assert app._handlers["test"]["type"] == "batch_sync"

    def test_handler_does_not_accept_batch_params(self):
        """handler() must not accept batch, batch_max_size, or batch_max_wait_ms."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        with pytest.raises(TypeError):
            @app.handler(topic="test", batch=True)  # type: ignore[call-arg]
            def handle(msg, ctx):
                return None
