import json

import pytest
from app.validation import detect_validation_candidates, select_validation_command


@pytest.mark.parametrize(
    ("files", "expected_command"),
    [
        (
            {"package.json": {"scripts": {"check": "tsc --noEmit"}}, "pnpm-lock.yaml": ""},
            "pnpm run check",
        ),
        ({"deno.json": {"tasks": {"test": "deno test"}}}, "deno task test"),
        ({"pyproject.toml": "[tool.pytest.ini_options]\n"}, "python -m pytest"),
        ({"Cargo.toml": "[package]\nname='demo'\n"}, "cargo test"),
        ({"go.mod": "module example.com/demo\n"}, "go test ./..."),
        ({"pom.xml": "<project />"}, "mvn test"),
        ({"build.gradle.kts": 'plugins { kotlin("jvm") }'}, "gradle test"),
        ({"demo.Tests.csproj": "<Project />"}, "dotnet test"),
        ({"composer.json": {"scripts": {"test": "phpunit"}}}, "composer test"),
        ({"Gemfile": "source 'https://rubygems.org'\n", ".rspec": ""}, "bundle exec rspec"),
        ({"Package.swift": "// swift-tools-version: 5.9\n"}, "swift test"),
        ({"pubspec.yaml": "name: demo\n"}, "dart test"),
        ({"mix.exs": "defmodule Demo.MixProject do end\n"}, "mix test"),
        ({"build.sbt": 'scalaVersion := "3.4.0"\n'}, "sbt test"),
        ({"stack.yaml": "resolver: lts\n"}, "stack test"),
        ({"project.clj": '(defproject demo "0.1.0")\n'}, "lein test"),
        ({"dune-project": "(lang dune 3.0)\n"}, "dune runtest"),
        ({".busted": ""}, "busted"),
        (
            {"DESCRIPTION": "Package: demo\n", "tests/testthat/.keep": ""},
            "Rscript -e \"testthat::test_dir('tests/testthat')\"",
        ),
        ({"cpanfile": "", "t/.keep": ""}, "prove -lr t"),
        ({"build.zig": 'const std = @import("std");\n'}, "zig build test"),
        ({"demo.nimble": 'version = "0.1.0"\n'}, "nimble test"),
        (
            {"Project.toml": 'name = "Demo"\n', "test/runtests.jl": ""},
            'julia --project=. -e "using Pkg; Pkg.test()"',
        ),
    ],
)
def test_detects_mainstream_validation_commands(tmp_path, files, expected_command):
    for name, content in files.items():
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(content, dict):
            path.write_text(json.dumps(content), encoding="utf-8")
        else:
            path.write_text(content, encoding="utf-8")

    command, candidates = select_validation_command(tmp_path)

    assert command == expected_command
    assert candidates


def test_detects_multiple_language_candidates_without_duplicate_commands(tmp_path):
    (tmp_path / "package.json").write_text(
        json.dumps({"scripts": {"test": "vitest", "build": "vite build"}}),
        encoding="utf-8",
    )
    (tmp_path / "Cargo.toml").write_text("[package]\nname='demo'\n", encoding="utf-8")

    candidates = detect_validation_candidates(tmp_path)
    commands = [candidate.command for candidate in candidates]

    assert "cargo test" in commands
    assert "npm run test" in commands
    assert len(commands) == len(set(commands))
