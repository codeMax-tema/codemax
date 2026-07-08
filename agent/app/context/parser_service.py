from __future__ import annotations

import re
from dataclasses import dataclass, field
from pathlib import Path

from app.context.language_registry import language_for_path


@dataclass(frozen=True, slots=True)
class ParsedCode:
    path: str
    language: str
    parser_mode: str
    imports: list[str] = field(default_factory=list)
    functions: list[str] = field(default_factory=list)
    classes: list[str] = field(default_factory=list)
    symbols: list[str] = field(default_factory=list)


class CodeParserService:
    def __init__(self) -> None:
        self._tree_sitter_available = self._detect_tree_sitter()

    def parse_file(self, path: str | Path) -> ParsedCode:
        file_path = Path(path)
        return self.parse_text(str(file_path), file_path.read_text(encoding="utf-8", errors="ignore"))

    def parse_text(self, path: str | Path, source: str) -> ParsedCode:
        language = language_for_path(path)
        parser_mode = "tree-sitter" if self._tree_sitter_available else "fallback"
        imports = sorted(_unique(self._extract_imports(language.language_id, source)))
        functions = sorted(_unique(self._extract_functions(language.language_id, source)))
        classes = sorted(_unique(self._extract_classes(language.language_id, source)))
        symbols = sorted(_unique([*imports, *functions, *classes]))
        return ParsedCode(
            path=str(path),
            language=language.language_id,
            parser_mode=parser_mode,
            imports=imports,
            functions=functions,
            classes=classes,
            symbols=symbols,
        )

    def _detect_tree_sitter(self) -> bool:
        try:
            import tree_sitter  # noqa: F401
        except Exception:
            return False
        return True

    def _extract_imports(self, language: str, source: str) -> list[str]:
        patterns = {
            "python": [
                r"^\s*import\s+([A-Za-z_][\w.]*)",
                r"^\s*from\s+([A-Za-z_][\w.]*)\s+import\s+",
            ],
            "typescript": [r"from\s+['\"]([^'\"]+)['\"]", r"import\s+['\"]([^'\"]+)['\"]"],
            "javascript": [r"from\s+['\"]([^'\"]+)['\"]", r"require\(['\"]([^'\"]+)['\"]\)"],
            "java": [r"^\s*import\s+([\w.*]+);"],
            "go": [r"^\s*import\s+(?:\(\s*)?\"([^\"]+)\""],
            "rust": [r"^\s*use\s+([\w:]+)"],
            "csharp": [r"^\s*using\s+([\w.]+);"],
            "php": [r"^\s*use\s+([\w\\]+);"],
            "ruby": [r"^\s*require\s+['\"]([^'\"]+)['\"]"],
            "lua": [r"^\s*local\s+\w+\s*=\s*require\s*\(?['\"]([^'\"]+)['\"]\)?"],
            "r": [r"^\s*library\(([^)]+)\)", r"^\s*require\(([^)]+)\)"],
            "perl": [r"^\s*use\s+([\w:]+)", r"^\s*require\s+['\"]?([^'\";]+)"],
            "elixir": [r"^\s*import\s+([\w.]+)", r"^\s*alias\s+([\w.]+)"],
            "erlang": [r"^-include\(['\"]([^'\"]+)['\"]\)"],
            "haskell": [r"^\s*import\s+(?:qualified\s+)?([\w.]+)"],
            "julia": [r"^\s*using\s+([\w.]+)", r"^\s*import\s+([\w.]+)"],
            "clojure": [r"\(:require\s+\[?([\w.\-]+)"],
            "ocaml": [r"^\s*open\s+([\w.]+)"],
            "fsharp": [r"^\s*open\s+([\w.]+)"],
            "groovy": [r"^\s*import\s+([\w.*]+)"],
        }
        return _matches(patterns.get(language, []), source)

    def _extract_functions(self, language: str, source: str) -> list[str]:
        if language == "python":
            return _matches([r"^\s*def\s+([A-Za-z_]\w*)\s*\("], source)
        if language in {"elixir", "ruby"}:
            return _matches([r"^\s*def\s+([A-Za-z_]\w*[!?=]?)"], source)
        if language == "erlang":
            return _matches([r"^\s*([a-z][\w@]*)\s*\("], source)
        if language in {"typescript", "javascript"}:
            return _matches(
                [
                    r"\bfunction\s+([A-Za-z_$][\w$]*)\s*\(",
                    r"\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:async\s*)?\(",
                    r"^\s*([A-Za-z_$][\w$]*)\s*\([^)]*\)\s*\{",
                ],
                source,
            )
        return _matches(
            [
                r"\b(?:fn|func|fun|def|function|sub|void|int|string|bool|public|private|static)\s+([A-Za-z_]\w*)\s*\(",
                r"^\s*([A-Za-z_]\w*)\s*<-\s*function\s*\(",
            ],
            source,
        )

    def _extract_classes(self, language: str, source: str) -> list[str]:
        if language in {"go", "rust", "c"}:
            return _matches([r"\b(?:struct|enum|trait|interface)\s+([A-Za-z_]\w*)"], source)
        if language == "elixir":
            return _matches([r"^\s*defmodule\s+([\w.]+)\s+do"], source)
        if language == "erlang":
            return _matches([r"^-module\(([\w@]+)\)"], source)
        return _matches([r"\b(?:class|interface|struct|enum|trait|object|module|contract|protocol)\s+([A-Za-z_]\w*)"], source)


def _matches(patterns: list[str], source: str) -> list[str]:
    values: list[str] = []
    for pattern in patterns:
        values.extend(match.group(1) for match in re.finditer(pattern, source, re.MULTILINE))
    return values


def _unique(values: list[str]) -> list[str]:
    seen: set[str] = set()
    result: list[str] = []
    for value in values:
        clean = value.strip()
        if clean and clean not in seen:
            seen.add(clean)
            result.append(clean)
    return result
