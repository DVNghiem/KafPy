"""Embedded guide documents for KafPy.

KafPy ships with getting-started, handler-patterns, configuration,
and error-handling guides that are accessible at runtime.

Example usage::

    from importlib.resources import files
    guide = (files("kafpy.guides") / "getting-started.md").read_text()
    print(guide[:500])

"""

from __future__ import annotations

__all__ = [
    "list_guides",
]

def list_guides() -> list[str]:
    """Return names of available guide documents."""
    return [
        "getting-started",
        "handler-patterns",
        "configuration",
        "error-handling",
    ]