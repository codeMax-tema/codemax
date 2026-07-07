"""Project validation command detection."""

from app.validation.detector import (
    ValidationCommandCandidate,
    detect_validation_candidates,
    select_validation_command,
)

__all__ = [
    "ValidationCommandCandidate",
    "detect_validation_candidates",
    "select_validation_command",
]
