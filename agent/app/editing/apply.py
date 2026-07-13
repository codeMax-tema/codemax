from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from app.editing.models import DeleteEdit, EditingPlan


class EditSafetyError(ValueError):
    def __init__(self, message: str, *, path: str | None = None) -> None:
        super().__init__(message)
        self.path = path


@dataclass(frozen=True, slots=True)
class AppliedFileEdit:
    path: str
    operation: str
    summary: str


@dataclass(frozen=True, slots=True)
class ApplyEditResult:
    file_edits: list[AppliedFileEdit] = field(default_factory=list)
    pending_approval: list[DeleteEdit] = field(default_factory=list)

    @property
    def requires_approval(self) -> bool:
        return bool(self.pending_approval)


def apply_edit_plan(workspace: str | Path, plan: EditingPlan, *, allow_deletes: bool = False) -> ApplyEditResult:
    del workspace, plan, allow_deletes
    raise EditSafetyError("Python file editing is disabled; edits must be committed by the Rust safety service.")
