from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class LanguageInfo:
    language_id: str
    tree_sitter_name: str
    extensions: tuple[str, ...]


_LANGUAGES = [
    LanguageInfo("typescript", "typescript", (".ts", ".tsx")),
    LanguageInfo("javascript", "javascript", (".js", ".jsx", ".mjs", ".cjs")),
    LanguageInfo("python", "python", (".py", ".pyw")),
    LanguageInfo("java", "java", (".java",)),
    LanguageInfo("go", "go", (".go",)),
    LanguageInfo("rust", "rust", (".rs",)),
    LanguageInfo("c", "c", (".c", ".h")),
    LanguageInfo("cpp", "cpp", (".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx")),
    LanguageInfo("csharp", "c_sharp", (".cs",)),
    LanguageInfo("php", "php", (".php",)),
    LanguageInfo("ruby", "ruby", (".rb",)),
    LanguageInfo("kotlin", "kotlin", (".kt", ".kts")),
    LanguageInfo("swift", "swift", (".swift",)),
    LanguageInfo("dart", "dart", (".dart",)),
    LanguageInfo("scala", "scala", (".scala", ".sc")),
    LanguageInfo("objectivec", "objc", (".m", ".mm")),
    LanguageInfo("lua", "lua", (".lua",)),
    LanguageInfo("r", "r", (".r", ".R")),
    LanguageInfo("perl", "perl", (".pl", ".pm")),
    LanguageInfo("elixir", "elixir", (".ex", ".exs")),
    LanguageInfo("erlang", "erlang", (".erl", ".hrl")),
    LanguageInfo("haskell", "haskell", (".hs", ".lhs")),
    LanguageInfo("zig", "zig", (".zig",)),
    LanguageInfo("solidity", "solidity", (".sol",)),
    LanguageInfo("julia", "julia", (".jl",)),
    LanguageInfo("clojure", "clojure", (".clj", ".cljs", ".cljc")),
    LanguageInfo("ocaml", "ocaml", (".ml", ".mli")),
    LanguageInfo("fsharp", "fsharp", (".fs", ".fsx", ".fsi")),
    LanguageInfo("visualbasic", "vbnet", (".vb",)),
    LanguageInfo("groovy", "groovy", (".groovy", ".gradle")),
    LanguageInfo("shell", "bash", (".sh", ".bash", ".zsh", ".ps1")),
    LanguageInfo("sql", "sql", (".sql",)),
    LanguageInfo("html", "html", (".html", ".htm")),
    LanguageInfo("css", "css", (".css", ".scss", ".sass", ".less")),
    LanguageInfo("vue", "vue", (".vue",)),
    LanguageInfo("svelte", "svelte", (".svelte",)),
    LanguageInfo("yaml", "yaml", (".yml", ".yaml")),
    LanguageInfo("toml", "toml", (".toml",)),
    LanguageInfo("json", "json", (".json", ".jsonc")),
    LanguageInfo("markdown", "markdown", (".md", ".mdx")),
]

_BY_EXTENSION = {
    extension: language
    for language in _LANGUAGES
    for extension in language.extensions
}
_UNKNOWN = LanguageInfo("plaintext", "plaintext", ())


def language_for_path(path: str | Path) -> LanguageInfo:
    suffix = Path(path).suffix.lower()
    return _BY_EXTENSION.get(suffix, _UNKNOWN)


def supported_languages() -> list[LanguageInfo]:
    return list(_LANGUAGES)
