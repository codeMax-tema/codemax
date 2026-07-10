from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from app.editing.models import CreateEdit, DeleteEdit, EditingPlan, FileEdit, UpdateEdit


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


@dataclass(frozen=True, slots=True)
class _PreparedEdit:
    edit: FileEdit
    target: Path
    relative_path: str


def apply_edit_plan(workspace: str | Path, plan: EditingPlan) -> ApplyEditResult:
    root = _resolve_workspace(workspace)
    prepared = [_prepare_edit(root, edit) for edit in plan.edits]
    pending_approval = [item.edit for item in prepared if isinstance(item.edit, DeleteEdit)]

    if pending_approval:
        return ApplyEditResult(pending_approval=pending_approval)

    applied: list[AppliedFileEdit] = []
    for item in prepared:
        edit = item.edit
        if isinstance(edit, CreateEdit):
            item.target.parent.mkdir(parents=True, exist_ok=True)
            item.target.write_text(edit.content, encoding="utf-8")
        elif isinstance(edit, UpdateEdit):
            item.target.write_text(edit.content, encoding="utf-8")
        else:  # pragma: no cover - delete edits return for approval above
            raise AssertionError(f"Unexpected edit operation: {edit.operation}")

        applied.append(
            AppliedFileEdit(
                path=item.relative_path,
                operation=edit.operation,
                summary=edit.summary,
            )
        )

    return ApplyEditResult(file_edits=applied)


def _resolve_workspace(workspace: str | Path) -> Path:
    root = Path(workspace).expanduser()
    try:
        resolved = root.resolve(strict=True)
    except OSError as error:
        raise EditSafetyError(f"Workspace is unavailable: {root}") from error
    if not resolved.is_dir():
        raise EditSafetyError(f"Workspace is not a directory: {resolved}")
    return resolved


def _prepare_edit(root: Path, edit: FileEdit) -> _PreparedEdit:
    relative = _validate_relative_path(edit.path)
    target = (root / relative).resolve(strict=False)
    _require_inside_workspace(root, target, edit.path)
    _validate_text_content(edit)

    if isinstance(edit, CreateEdit):
        if target.exists():
            raise EditSafetyError(
                f"Create edit refuses to overwrite an existing path: {edit.path}",
                path=edit.path,
            )
        if target.parent.exists() and not target.parent.is_dir():
            raise EditSafetyError(
                f"Create edit parent is not a directory: {edit.path}",
                path=edit.path,
            )
    else:
        if not target.exists() or not target.is_file():
            raise EditSafetyError(
                f"Edit target is not an existing file: {edit.path}",
                path=edit.path,
            )
        if isinstance(edit, UpdateEdit):
            _read_utf8_text(target, edit.path)

    return _PreparedEdit(edit=edit, target=target, relative_path=relative.as_posix())


def _validate_relative_path(raw_path: str) -> Path:
    candidate = Path(raw_path)
    if candidate.is_absolute() or candidate.anchor:
        raise EditSafetyError(
            f"Edit path must be a workspace-relative path: {raw_path}",
            path=raw_path,
        )
    if any(part == ".." for part in candidate.parts):
        raise EditSafetyError(
            f"Edit path must stay inside the workspace: {raw_path}",
            path=raw_path,
        )
    if not candidate.parts or candidate == Path("."):
        raise EditSafetyError("Edit path must name a file.", path=raw_path)
    return candidate


def _require_inside_workspace(root: Path, target: Path, raw_path: str) -> None:
    try:
        target.relative_to(root)
    except ValueError as error:
        raise EditSafetyError(
            f"Edit path resolves outside the workspace: {raw_path}",
            path=raw_path,
        ) from error


def _validate_text_content(edit: FileEdit) -> None:
    if not isinstance(edit, (CreateEdit, UpdateEdit)):
        return
    try:
        edit.content.encode("utf-8", errors="strict")
    except UnicodeEncodeError as error:
        raise EditSafetyError(
            f"Edit content is not valid UTF-8 text: {edit.path}",
            path=edit.path,
        ) from error
    if "\x00" in edit.content:
        raise EditSafetyError(
            f"Edit content appears binary and was refused: {edit.path}",
            path=edit.path,
        )


def _read_utf8_text(target: Path, raw_path: str) -> str:
    try:
        content = target.read_bytes().decode("utf-8", errors="strict")
    except (OSError, UnicodeDecodeError) as error:
        raise EditSafetyError(
            f"Existing file is binary or cannot be decoded as UTF-8: {raw_path}",
            path=raw_path,
        ) from error
    if "\x00" in content:
        raise EditSafetyError(
            f"Existing file appears binary and was refused: {raw_path}",
            path=raw_path,
        )
    return content
