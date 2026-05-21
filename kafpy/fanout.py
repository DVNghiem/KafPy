"""Fan-out builder for KafPy."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Callable

from ._kafpy import Consumer

__all__ = ["FanOutBuilder", "FanOutRegistration", "FanInRegistration"]


@dataclass(frozen=True)
class FanInRegistration:
    """Result of a fan-in handler registration.

    Attributes:
        handler_key: The unique identifier for this fan-in handler.
        fan_in_id: Unique numeric ID generated at registration time.
        sources: The list of source topics that feed this handler.
    """

    handler_key: str
    fan_in_id: int
    sources: list[str]


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

    timeout_ms: int | None = None
    consumer: Consumer = field(default=None)
    group_name: str = ""
    sink_topics: list[str] = field(default_factory=list)
    handler: Callable = field(default=None)
    max_fan_out: int | None = None

    def register(self) -> "FanOutRegistration":
        """Register the fan-out group with the consumer.

        Returns:
            FanOutRegistration with group_name, fan_out_id, sink_topics.
        """
        result = self.consumer.register_fanout(
            self.group_name,
            self.sink_topics,
            self.handler,
            self.max_fan_out,
            self.timeout_ms,
        )
        return FanOutRegistration(
            group_name=result.group_name,
            fan_out_id=result.fan_out_id,
            sink_topics=result.sink_topics,
        )
