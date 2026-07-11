from __future__ import annotations

import shutil
from dataclasses import dataclass, field
from pathlib import Path
from tempfile import TemporaryDirectory

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


def apply_edit_plan(
    workspace: str | Path,
    plan: EditingPlan,
    *,
    allow_deletes: bool = False,
) -> ApplyEditResult:
    root = _resolve_workspace(workspace)
    prepared = [_prepare_edit(root, edit) for edit in plan.edits]
    pending_approval = [item.edit for item in prepared if isinstance(item.edit, DeleteEdit)]

    if pending_approval and not allow_deletes:
        return ApplyEditResult(pending_approval=pending_approval)

    with TemporaryDirectory(dir=root, prefix=".codemax-edit-txn-") as transaction_dir:
        backup_root = Path(transaction_dir)
        backups = _backup_existing_files(root, prepared, backup_root)
        created_targets: list[Path] = []
        created_directories: list[Path] = []
        applied: list[AppliedFileEdit] = []
        try:
            for item in prepared:
                _revalidate_prepared_edit(root, item)
                edit = item.edit
                if isinstance(edit, CreateEdit):
                    created_targets.append(item.target)
                    created_directories.extend(_missing_directories(root, item.target.parent))
                    item.target.parent.mkdir(parents=True, exist_ok=True)
                    item.target.write_text(edit.content, encoding="utf-8")
                elif isinstance(edit, UpdateEdit):
                    item.target.write_text(edit.content, encoding="utf-8")
                elif isinstance(edit, DeleteEdit):
                    item.target.unlink()
                else:  # pragma: no cover - discriminated union is exhaustive
                    raise AssertionError(f"Unexpected edit operation: {edit.operation}")

                applied.append(
                    AppliedFileEdit(
                        path=item.relative_path,
                        operation=edit.operation,
                        summary=edit.summary,
                    )
                )
        except Exception as error:
            rollback_errors = _rollback_edits(backups, created_targets, created_directories)
            if rollback_errors:
                detail = "; ".join(rollback_errors)
                raise OSError(
                    f"Edit transaction failed and rollback was incomplete: {detail}"
                ) from error
            raise

    return ApplyEditResult(file_edits=applied)


def _backup_existing_files(
    root: Path,
    prepared: list[_PreparedEdit],
    backup_root: Path,
) -> dict[Path, Path]:
    backups: dict[Path, Path] = {}
    for index, item in enumerate(prepared):
        _revalidate_prepared_edit(root, item)
        if isinstance(item.edit, CreateEdit) or item.target in backups:
            continue
        backup = backup_root / f"{index:04d}.backup"
        shutil.copy2(item.target, backup)
        backups[item.target] = backup
    return backups


def _missing_directories(root: Path, parent: Path) -> list[Path]:
    missing: list[Path] = []
    current = parent
    while current != root and not current.exists():
        missing.append(current)
        current = current.parent
    return missing


def _rollback_edits(
    backups: dict[Path, Path],
    created_targets: list[Path],
    created_directories: list[Path],
) -> list[str]:
    errors: list[str] = []
    for target in reversed(created_targets):
        try:
            target.unlink(missing_ok=True)
        except OSError as error:
            errors.append(f"remove {target.name}: {type(error).__name__}")
    for target, backup in backups.items():
        try:
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(backup, target)
        except OSError as error:
            errors.append(f"restore {target.name}: {type(error).__name__}")
    for directory in sorted(
        set(created_directories), key=lambda item: len(item.parts), reverse=True
    ):
        try:
            directory.rmdir()
        except FileNotFoundError:
            continue
        except OSError as error:
            errors.append(f"remove directory {directory.name}: {type(error).__name__}")
    return errors


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


def _revalidate_prepared_edit(root: Path, item: _PreparedEdit) -> None:
    expected = (root / Path(item.relative_path)).resolve(strict=False)
    current = item.target.resolve(strict=False)
    _require_inside_workspace(root, expected, item.relative_path)
    _require_inside_workspace(root, current, item.relative_path)
    if current != expected:
        raise EditSafetyError(
            f"Edit target changed after workspace safety validation: {item.relative_path}",
            path=item.relative_path,
        )
    if isinstance(item.edit, CreateEdit):
        if current.exists():
            raise EditSafetyError(
                f"Create edit target changed after validation: {item.relative_path}",
                path=item.relative_path,
            )
        parent = current.parent.resolve(strict=False)
        _require_inside_workspace(root, parent, item.relative_path)
        return
    if not current.exists() or not current.is_file():
        raise EditSafetyError(
            f"Edit target changed after validation: {item.relative_path}",
            path=item.relative_path,
        )
    if isinstance(item.edit, UpdateEdit):
        _read_utf8_text(current, item.relative_path)


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
