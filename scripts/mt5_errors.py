"""Read availability is distinct from malformed data or failed persistence."""


class HistoryUnavailable(ValueError):
    """The requested closed window returned no history; never implies completeness."""
