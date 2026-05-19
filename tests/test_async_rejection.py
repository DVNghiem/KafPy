"""Tests for async rejection with batch=True — reproduces bug where async functions
used with @app.handler(topic="...", batch=True) do not raise TypeError with correct message.

Bug location: kafpy/runtime.py:137-139
Expected: TypeError("async batch handlers are not supported")
Actual: TypeError("handler_mode must be 'sync' or 'batch_sync', got 'batch_async'")
         (raised in register_handler instead of in handler() where it should be caught early)
"""

from __future__ import annotations

import pytest
from unittest.mock import MagicMock


class TestAsyncBatchHandlerRejection:
    """Async functions with batch=True must raise TypeError with 'async batch handlers are not supported'."""

    def test_async_handler_with_batch_true_raises_correct_typeerror_message(self):
        """Async function used with @app.handler(topic=..., batch=True) should raise
        TypeError with message 'async batch handlers are not supported'.

        Bug: When batch=True and fn is async, the code sets handler_mode="batch_async"
        and passes it to register_handler, which raises a generic error about handler_mode
        being wrong, instead of the specific 'async batch handlers are not supported' error
        that batch_handler raises (runtime.py:199).
        """
        from kafpy import KafPy

        # Create a mock consumer that won't actually connect to Kafka
        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        # Define an async function - this should NOT be allowed with batch=True
        async def async_batch_handler(messages, ctx):
            return None

        # The decorator is applied at definition time when we use @
        # We wrap the decoration in a function to catch the exception
        def try_register():
            @app.handler(topic="test", batch=True)
            async def inner_handler(messages, ctx):
                return None

        # Should raise TypeError with the CORRECT message: "async batch handlers are not supported"
        # This matches what batch_handler raises (runtime.py:199)
        with pytest.raises(TypeError) as exc_info:
            try_register()

        # The bug: currently raises "handler_mode must be 'sync' or 'batch_sync', got 'batch_async'"
        # Expected: "async batch handlers are not supported"
        assert "async batch handlers are not supported" in str(exc_info.value)

    def test_async_batch_handler_raises_typeerror_not_handler_mode_error(self):
        """Verify the error is specifically about async batch handlers, not handler_mode values."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        async def async_batch_handler(messages, ctx):
            return None

        def try_register():
            @app.handler(topic="test", batch=True)
            async def inner_handler(messages, ctx):
                return None

        with pytest.raises(TypeError) as exc_info:
            try_register()

        error_message = str(exc_info.value)

        # The correct error message should mention "async batch handlers"
        assert "async batch handlers" in error_message
        # The bug produces an error about handler_mode being wrong - that's the wrong message
        assert "handler_mode must" not in error_message, (
            f"Error should be 'async batch handlers are not supported', "
            f"not a generic handler_mode error. Got: {error_message}"
        )


class TestSyncBatchHandlerWorks:
    """Sync functions with batch=True should work fine."""

    def test_sync_batch_handler_works(self):
        """Sync function used with @app.handler(topic=..., batch=True) should work."""
        from kafpy import KafPy

        mock_consumer = MagicMock()
        app = KafPy(consumer=mock_consumer)

        # This should NOT raise
        @app.handler(topic="test", batch=True)
        def inner_handler(messages, ctx):
            return None

        # Verify it was registered correctly
        assert "test" in app._handlers
        assert app._handlers["test"]["type"] == "batch_sync"
