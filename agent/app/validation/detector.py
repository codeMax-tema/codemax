from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.11+ always has tomllib.
    tomllib = None  # type: ignore[assignment]


@dataclass(frozen=True, slots=True)
class ValidationCommandCandidate:
    language: str
    ecosystem: str
    command: str
    reason: str
    evidence: tuple[str, ...]
    priority: int


def detect_validation_candidates(project_root: str | Path) -> list[ValidationCommandCandidate]:
    root = Path(project_root).expanduser()
    if not root.is_dir():
        return []

    candidates: list[ValidationCommandCandidate] = []
    candidates.extend(detect_javascript(root))
    candidates.extend(detect_deno(root))
    candidates.extend(detect_python(root))
    candidates.extend(detect_rust(root))
    candidates.extend(detect_go(root))
    candidates.extend(detect_jvm(root))
    candidates.extend(detect_dotnet(root))
    candidates.extend(detect_php(root))
    candidates.extend(detect_ruby(root))
    candidates.extend(detect_swift(root))
    candidates.extend(detect_dart(root))
    candidates.extend(detect_cpp(root))
    candidates.extend(detect_elixir(root))
    candidates.extend(detect_erlang(root))
    candidates.extend(detect_haskell(root))
    candidates.extend(detect_clojure(root))
    candidates.extend(detect_ocaml(root))
    candidates.extend(detect_lua(root))
    candidates.extend(detect_r(root))
    candidates.extend(detect_perl(root))
    candidates.extend(detect_zig(root))
    candidates.extend(detect_nim(root))
    candidates.extend(detect_julia(root))

    candidates.sort(
        key=lambda candidate: (-candidate.priority, candidate.language, candidate.command)
    )
    return dedupe_candidates(candidates)


def select_validation_command(
    project_root: str | Path,
    fallback: str = "python --version",
) -> tuple[str, list[ValidationCommandCandidate]]:
    candidates = detect_validation_candidates(project_root)
    if not candidates:
        return fallback, candidates
    return candidates[0].command, candidates


def detect_javascript(root: Path) -> list[ValidationCommandCandidate]:
    package_json = root / "package.json"
    if not package_json.is_file():
        return []

    data = read_json(package_json)
    scripts = data.get("scripts") if isinstance(data, dict) else {}
    scripts = scripts if isinstance(scripts, dict) else {}
    package_manager = detect_node_package_manager(root)
    commands = []
    for script_name, priority in [
        ("check", 100),
        ("test", 98),
        ("lint", 92),
        ("build", 88),
    ]:
        if script_name in scripts:
            commands.append(
                candidate(
                    "JavaScript/TypeScript",
                    package_manager,
                    node_run_command(package_manager, script_name),
                    f"package.json defines a {script_name} script.",
                    package_json,
                    priority,
                )
            )
    return commands


def detect_deno(root: Path) -> list[ValidationCommandCandidate]:
    for name in ["deno.json", "deno.jsonc"]:
        manifest = root / name
        if manifest.is_file():
            data = read_json(manifest)
            tasks = data.get("tasks") if isinstance(data, dict) else {}
            if isinstance(tasks, dict):
                for task_name, priority in [("check", 96), ("test", 94), ("lint", 90)]:
                    if task_name in tasks:
                        return [
                            candidate(
                                "JavaScript/TypeScript",
                                "deno",
                                f"deno task {task_name}",
                                f"{name} defines a {task_name} task.",
                                manifest,
                                priority,
                            )
                        ]
            return [
                candidate(
                    "JavaScript/TypeScript",
                    "deno",
                    "deno test",
                    f"{name} marks the repository as a Deno project.",
                    manifest,
                    82,
                )
            ]
    return []


def detect_python(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_existing(
        root,
        ["pyproject.toml", "pytest.ini", "tox.ini", "noxfile.py", "setup.cfg", "requirements.txt"],
    )
    if evidence is None:
        return []

    runner = "uv run" if (root / "uv.lock").is_file() else "python -m"
    commands = [
        candidate(
            "Python",
            "pytest",
            f"{runner} pytest",
            "Python project metadata or test configuration was found.",
            evidence,
            96,
        )
    ]
    if has_python_tool(root, "ruff") or (root / "ruff.toml").is_file():
        commands.append(
            candidate(
                "Python",
                "ruff",
                f"{runner} ruff check .",
                "Ruff configuration was found.",
                evidence,
                90,
            )
        )
    if has_python_tool(root, "mypy") or (root / "mypy.ini").is_file():
        commands.append(
            candidate(
                "Python", "mypy", f"{runner} mypy .", "Mypy configuration was found.", evidence, 86
            )
        )
    return commands


def detect_rust(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "Cargo.toml"
    if manifest.is_file():
        return [
            candidate("Rust", "cargo", "cargo test", "Cargo.toml was found.", manifest, 98),
            candidate("Rust", "cargo", "cargo check", "Cargo.toml was found.", manifest, 94),
        ]
    return []


def detect_go(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "go.mod"
    if manifest.is_file():
        return [candidate("Go", "go", "go test ./...", "go.mod was found.", manifest, 98)]
    return []


def detect_jvm(root: Path) -> list[ValidationCommandCandidate]:
    candidates: list[ValidationCommandCandidate] = []
    pom = root / "pom.xml"
    if pom.is_file():
        mvn = (
            ".\\mvnw"
            if (root / "mvnw.cmd").is_file()
            else ("./mvnw" if (root / "mvnw").is_file() else "mvn")
        )
        candidates.append(candidate("Java", "maven", f"{mvn} test", "pom.xml was found.", pom, 96))

    gradle_file = first_existing(root, ["build.gradle.kts", "build.gradle"])
    if gradle_file is not None:
        gradle = (
            ".\\gradlew"
            if (root / "gradlew.bat").is_file()
            else ("./gradlew" if (root / "gradlew").is_file() else "gradle")
        )
        language = "Kotlin" if gradle_file.name.endswith(".kts") else "Java"
        candidates.append(
            candidate(
                language,
                "gradle",
                f"{gradle} test",
                f"{gradle_file.name} was found.",
                gradle_file,
                94,
            )
        )

    sbt = root / "build.sbt"
    if sbt.is_file():
        candidates.append(candidate("Scala", "sbt", "sbt test", "build.sbt was found.", sbt, 92))
    return candidates


def detect_dotnet(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_glob(root, ["*.sln", "*.csproj", "*.fsproj", "*.vbproj"])
    if evidence is None:
        return []
    command = "dotnet test" if looks_like_dotnet_test_project(root, evidence) else "dotnet build"
    return [candidate("C#/.NET", "dotnet", command, f"{evidence.name} was found.", evidence, 94)]


def detect_php(root: Path) -> list[ValidationCommandCandidate]:
    composer = root / "composer.json"
    if not composer.is_file():
        return []
    data = read_json(composer)
    scripts = data.get("scripts") if isinstance(data, dict) else {}
    if isinstance(scripts, dict) and "test" in scripts:
        command = "composer test"
        priority = 92
        reason = "composer.json defines a test script."
    elif (root / "phpunit.xml").is_file() or (root / "phpunit.xml.dist").is_file():
        command = (
            "vendor\\bin\\phpunit"
            if (root / "vendor" / "bin" / "phpunit.bat").is_file()
            else "vendor/bin/phpunit"
        )
        priority = 88
        reason = "PHPUnit configuration was found."
    else:
        command = "composer validate --strict"
        priority = 78
        reason = "composer.json was found."
    return [candidate("PHP", "composer", command, reason, composer, priority)]


def detect_ruby(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_existing(root, ["Gemfile", ".rspec", "Rakefile"])
    if evidence is None:
        return []
    if (root / ".rspec").is_file() or (root / "spec").is_dir():
        command = "bundle exec rspec"
        reason = "RSpec files were found."
    else:
        command = "bundle exec rake test"
        reason = "Ruby project files were found."
    return [candidate("Ruby", "bundler", command, reason, evidence, 90)]


def detect_swift(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "Package.swift"
    if manifest.is_file():
        return [
            candidate("Swift", "swiftpm", "swift test", "Package.swift was found.", manifest, 90)
        ]
    return []


def detect_dart(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "pubspec.yaml"
    if not manifest.is_file():
        return []
    text = safe_read_text(manifest)
    if "flutter:" in text or "sdk: flutter" in text:
        return [
            candidate(
                "Dart/Flutter",
                "flutter",
                "flutter test",
                "Flutter pubspec was found.",
                manifest,
                90,
            )
        ]
    return [candidate("Dart", "dart", "dart test", "pubspec.yaml was found.", manifest, 88)]


def detect_cpp(root: Path) -> list[ValidationCommandCandidate]:
    build_dir = root / "build"
    cmake = root / "CMakeLists.txt"
    if cmake.is_file() and (build_dir / "CTestTestfile.cmake").is_file():
        return [
            candidate(
                "C/C++",
                "cmake",
                "ctest --test-dir build --output-on-failure",
                "CMake build test metadata was found.",
                cmake,
                88,
            )
        ]
    makefile = first_existing(root, ["Makefile", "makefile"])
    if makefile is not None:
        return [
            candidate("C/C++", "make", "make test", f"{makefile.name} was found.", makefile, 82)
        ]
    meson = root / "meson.build"
    if meson.is_file() and (root / "build").is_dir():
        return [
            candidate(
                "C/C++",
                "meson",
                "meson test -C build",
                "Meson build directory was found.",
                meson,
                82,
            )
        ]
    return []


def detect_elixir(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "mix.exs"
    if manifest.is_file():
        return [candidate("Elixir", "mix", "mix test", "mix.exs was found.", manifest, 88)]
    return []


def detect_erlang(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "rebar.config"
    if manifest.is_file():
        return [
            candidate("Erlang", "rebar3", "rebar3 eunit", "rebar.config was found.", manifest, 82)
        ]
    return []


def detect_haskell(root: Path) -> list[ValidationCommandCandidate]:
    stack = root / "stack.yaml"
    if stack.is_file():
        return [candidate("Haskell", "stack", "stack test", "stack.yaml was found.", stack, 84)]
    cabal = first_glob(root, ["*.cabal"])
    if cabal is not None:
        return [
            candidate("Haskell", "cabal", "cabal test all", f"{cabal.name} was found.", cabal, 82)
        ]
    return []


def detect_clojure(root: Path) -> list[ValidationCommandCandidate]:
    lein = root / "project.clj"
    if lein.is_file():
        return [candidate("Clojure", "leiningen", "lein test", "project.clj was found.", lein, 84)]
    deps = root / "deps.edn"
    if deps.is_file():
        return [candidate("Clojure", "clojure", "clojure -M:test", "deps.edn was found.", deps, 78)]
    return []


def detect_ocaml(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_existing(root, ["dune-project", "dune"])
    if evidence is not None:
        return [
            candidate("OCaml", "dune", "dune runtest", f"{evidence.name} was found.", evidence, 82)
        ]
    return []


def detect_lua(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_existing(root, [".busted", "busted.lua"])
    if evidence is not None or (root / "spec").is_dir():
        return [
            candidate(
                "Lua",
                "busted",
                "busted",
                "Lua busted tests were found.",
                evidence or root / "spec",
                78,
            )
        ]
    return []


def detect_r(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_existing(root, ["DESCRIPTION", "renv.lock"])
    if evidence is None:
        return []
    if (root / "tests" / "testthat").is_dir():
        command = "Rscript -e \"testthat::test_dir('tests/testthat')\""
        reason = "R testthat tests were found."
    else:
        command = "R CMD check ."
        reason = "R package metadata was found."
    return [candidate("R", "r", command, reason, evidence, 76)]


def detect_perl(root: Path) -> list[ValidationCommandCandidate]:
    evidence = first_existing(root, ["cpanfile", "Makefile.PL", "dist.ini"])
    if evidence is not None or (root / "t").is_dir():
        return [
            candidate(
                "Perl",
                "prove",
                "prove -lr t",
                "Perl project or t/ tests were found.",
                evidence or root / "t",
                76,
            )
        ]
    return []


def detect_zig(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "build.zig"
    if manifest.is_file():
        return [candidate("Zig", "zig", "zig build test", "build.zig was found.", manifest, 78)]
    return []


def detect_nim(root: Path) -> list[ValidationCommandCandidate]:
    manifest = first_glob(root, ["*.nimble"])
    if manifest is not None:
        return [
            candidate("Nim", "nimble", "nimble test", f"{manifest.name} was found.", manifest, 74)
        ]
    return []


def detect_julia(root: Path) -> list[ValidationCommandCandidate]:
    manifest = root / "Project.toml"
    if manifest.is_file() and (root / "test" / "runtests.jl").is_file():
        return [
            candidate(
                "Julia",
                "julia",
                'julia --project=. -e "using Pkg; Pkg.test()"',
                "Julia Project.toml and tests were found.",
                manifest,
                76,
            )
        ]
    return []


def candidate(
    language: str,
    ecosystem: str,
    command: str,
    reason: str,
    evidence: Path,
    priority: int,
) -> ValidationCommandCandidate:
    return ValidationCommandCandidate(
        language=language,
        ecosystem=ecosystem,
        command=command,
        reason=reason,
        evidence=(str(evidence),),
        priority=priority,
    )


def detect_node_package_manager(root: Path) -> str:
    if (root / "pnpm-lock.yaml").is_file():
        return "pnpm"
    if (root / "yarn.lock").is_file():
        return "yarn"
    if (root / "bun.lockb").is_file() or (root / "bun.lock").is_file():
        return "bun"
    return "npm"


def node_run_command(package_manager: str, script_name: str) -> str:
    if package_manager == "yarn":
        return f"yarn {script_name}"
    return f"{package_manager} run {script_name}"


def has_python_tool(root: Path, tool_name: str) -> bool:
    pyproject = root / "pyproject.toml"
    if not pyproject.is_file() or tomllib is None:
        return False
    try:
        data = tomllib.loads(pyproject.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return False
    tool = data.get("tool")
    return isinstance(tool, dict) and tool_name in tool


def looks_like_dotnet_test_project(root: Path, evidence: Path) -> bool:
    if evidence.suffix == ".sln":
        return any("test" in path.stem.lower() for path in root.glob("*.*proj"))
    return "test" in evidence.stem.lower() or (root / "tests").is_dir()


def first_existing(root: Path, names: list[str]) -> Path | None:
    for name in names:
        path = root / name
        if path.exists():
            return path
    return None


def first_glob(root: Path, patterns: list[str]) -> Path | None:
    for pattern in patterns:
        for path in root.glob(pattern):
            if path.exists():
                return path
    return None


def read_json(path: Path) -> dict[str, Any]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return data if isinstance(data, dict) else {}


def safe_read_text(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError:
        return ""


def dedupe_candidates(
    candidates: list[ValidationCommandCandidate],
) -> list[ValidationCommandCandidate]:
    seen: set[str] = set()
    unique: list[ValidationCommandCandidate] = []
    for item in candidates:
        if item.command in seen:
            continue
        seen.add(item.command)
        unique.append(item)
    return unique
