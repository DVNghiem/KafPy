"""Fan-out builder for KafPy."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Callable

__all__ = ["FanOutBuilder", "FanOutRegistration"]


@dataclass(frozen=True)
class FanOutRegistration:
    """Result of a fan-out registration.

    Attributes:
        group_name: The identifier for this fan-out group.
        fan_out_id: Unique ID for this fan-out dispatch (generated at registration time).
        sink_topics: The list of sink topics.
    """

    group_name: str
    fan_out_id: int
    sink_topics: list[str]


@dataclass(frozen=True)
class FanOutBuilder:
    """Builder for configuring fan-out handler registration.

    Use `consumer.register_fanout(...)` to obtain an instance,
    then call `.max_fan_out(n)` to set the max degree, then `.register()`.

    Example::

        builder = consumer.register_fanout(
            group_name="enrichment",
            sink_topics=["topic-a", "topic-b"],
            handler=my_handler,
        )
        registration = builder.max_fan_out(8).register()
    """

    _consumer: object
    _group_name: str
    _sink_topics: list[str]
    _handler: Callable
    _max_fan_out: int | None
    _timeout_ms: int | None

    def max_fan_out(self, n: int) -> "FanOutBuilder":
        """Set the maximum fan-out degree.

        Args:
            n: Maximum concurrent sink branches (capped at 64).

        Returns:
            A new FanOutBuilder with max_fan_out set.
        """
        return FanOutBuilder(
            _consumer=self._consumer,
            _group_name=self._group_name,
            _sink_topics=self._sink_topics,
            _handler=self._handler,
            _max_fan_out=n,
            _timeout_ms=self._timeout_ms,
        )

    def register(self) -> "FanOutRegistration":
        """Register the fan-out group with the consumer.

        Returns:
            FanOutRegistration with group_name, fan_out_id, sink_topics.
        """
        import kafpy._kafpy as _kafpy

        result = self._consumer._consumer.register_fanout(
            self._group_name,
            self._sink_topics,
            self._handler,
            self._max_fan_out,
            self._timeout_ms,
        )
        return FanOutRegistration(
            group_name=result.group_name,
            fan_out_id=result.fan_out_id,
            sink_topics=result.sink_topics,
        )
