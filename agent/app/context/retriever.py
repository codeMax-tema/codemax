from __future__ import annotations

import os
import subprocess
from dataclasses import dataclass, field
from pathlib import Path

from app.context.language_registry import language_for_path
from app.context.parser_service import CodeParserService, ParsedCode

_SKIP_DIRS = {
    ".git",
    ".venv",
    ".worktrees",
    "__pycache__",
    "dist",
    "node_modules",
    "target",
}


@dataclass(frozen=True, slots=True)
class RetrievedContextItem:
    path: str
    score: int
    parsed: ParsedCode
    snippet: str


@dataclass(frozen=True, slots=True)
class RetrievedContext:
    items: list[RetrievedContextItem] = field(default_factory=list)
    scanned_file_count: int = 0
    truncated: bool = False


class ContextRetriever:
    def __init__(
        self,
        max_files: int = 12,
        max_file_bytes: int = 64 * 1024,
        max_scan_files: int | None = None,
    ) -> None:
        if max_files < 1:
            raise ValueError("max_files must be at least 1")
        self.max_files = max_files
        self.max_file_bytes = max_file_bytes
        self.max_scan_files = max_scan_files or max(max_files * 50, 50)
        self._parser = CodeParserService()

    def retrieve(self, repository_path: str | Path, query: str) -> RetrievedContext:
        root = Path(repository_path)
        query_terms = _terms(query)
        candidates: list[RetrievedContextItem] = []
        scanned = 0

        scan_truncated = False
        for path in self._iter_source_files(root):
            if scanned >= self.max_scan_files:
                scan_truncated = True
                break
            scanned += 1
            relative = path.relative_to(root).as_posix()
            text = path.read_text(encoding="utf-8", errors="ignore")
            parsed = self._parser.parse_text(relative, text)
            score = _score(relative, parsed, query_terms)
            if score <= 0:
                continue
            candidates.append(
                RetrievedContextItem(
                    path=relative,
                    score=score,
                    parsed=parsed,
                    snippet=text[:1200],
                )
            )

        candidates.sort(key=lambda item: (-item.score, item.path))
        selected = candidates[: self.max_files]
        return RetrievedContext(
            items=selected,
            scanned_file_count=scanned,
            truncated=scan_truncated or len(candidates) > len(selected),
        )

    def _iter_source_files(self, root: Path):
        yield from self._git_tracked_source_files(root)
        if (root / ".git").exists():
            return

        for current_root, dirnames, filenames in os.walk(root):
            dirnames[:] = sorted(
                dirname for dirname in dirnames if dirname not in _SKIP_DIRS
            )
            for filename in sorted(filenames):
                path = Path(current_root) / filename
                if self._is_source_file(path):
                    yield path

    def _git_tracked_source_files(self, root: Path):
        try:
            completed = subprocess.run(
                ["git", "-C", str(root), "ls-files"],
                check=False,
                capture_output=True,
                text=True,
                timeout=3,
            )
        except (OSError, subprocess.SubprocessError):
            return

        if completed.returncode != 0:
            return

        for line in completed.stdout.splitlines():
            relative = line.strip()
            if not relative:
                continue
            path = root / relative
            if self._is_source_file(path):
                yield path

    def _is_source_file(self, path: Path) -> bool:
        try:
            relative_parts = path.parts
        except OSError:
            return False
        if any(part in _SKIP_DIRS for part in relative_parts):
            return False
        if not path.is_file():
            return False
        if language_for_path(path).language_id == "plaintext":
            return False
        try:
            return path.stat().st_size <= self.max_file_bytes
        except OSError:
            return False


def _terms(query: str) -> set[str]:
    return {term.lower() for term in query.replace("_", " ").replace("-", " ").split() if term}


def _score(path: str, parsed: ParsedCode, query_terms: set[str]) -> int:
    haystack = " ".join([path, *parsed.symbols]).lower()
    score = sum(3 for term in query_terms if term in path.lower())
    score += sum(2 for term in query_terms if term in haystack)
    score += len(parsed.functions) + len(parsed.classes)
    return score
