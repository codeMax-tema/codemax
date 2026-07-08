import tempfile
import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.context import CodeParserService, ContextRetriever, language_for_path
from app.scheduler import TaskScheduler


class S12SchedulerContextTests(unittest.TestCase):
    def test_scheduler_limits_concurrent_tasks_and_keeps_fifo_queue(self):
        scheduler = TaskScheduler(max_concurrent_tasks=2)

        first = scheduler.submit("task-1")
        second = scheduler.submit("task-2")
        third = scheduler.submit("task-3")

        self.assertEqual(first.status, "running")
        self.assertEqual(second.status, "running")
        self.assertEqual(third.status, "queued")
        self.assertEqual(scheduler.snapshot().running_task_ids, ["task-1", "task-2"])
        self.assertEqual(scheduler.snapshot().queued_task_ids, ["task-3"])

    def test_scheduler_failed_task_releases_slot_without_touching_other_tasks(self):
        scheduler = TaskScheduler(max_concurrent_tasks=1)
        scheduler.submit("task-1")
        scheduler.submit("task-2")

        failed = scheduler.finish("task-1", success=False, message="boom")

        self.assertEqual(failed.status, "failed")
        self.assertEqual(scheduler.status("task-2").status, "running")
        self.assertEqual(scheduler.status("task-1").message, "boom")

    def test_language_registry_covers_mainstream_languages(self):
        paths = [
            "a.ts",
            "b.py",
            "c.java",
            "d.go",
            "e.rs",
            "f.cpp",
            "g.cs",
            "h.php",
            "i.rb",
            "j.kt",
            "k.swift",
            "l.dart",
            "m.scala",
            "n.lua",
            "o.ex",
            "p.hs",
            "q.zig",
            "r.sol",
            "s.jl",
            "t.clj",
        ]

        languages = {language_for_path(path).language_id for path in paths}

        self.assertGreaterEqual(
            languages,
            {
                "typescript",
                "python",
                "java",
                "go",
                "rust",
                "cpp",
                "csharp",
                "php",
                "ruby",
                "kotlin",
                "swift",
                "dart",
                "scala",
                "lua",
                "elixir",
                "haskell",
                "zig",
                "solidity",
                "julia",
                "clojure",
            },
        )

    def test_parser_extracts_imports_functions_and_classes(self):
        parser = CodeParserService()

        result = parser.parse_text(
            "sample.py",
            "import os\nclass Service:\n    def run(self):\n        return os.getcwd()\n",
        )

        self.assertEqual(result.language, "python")
        self.assertIn("os", result.imports)
        self.assertIn("Service", result.classes)
        self.assertIn("run", result.functions)
        self.assertIn(result.parser_mode, {"tree-sitter", "fallback"})

    def test_context_retriever_is_bounded_and_prefers_relevant_files(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            (root / "payment.py").write_text(
                "class PaymentService:\n    def refund(self):\n        pass\n",
                encoding="utf-8",
            )
            (root / "unrelated.py").write_text(
                "def paint():\n    pass\n",
                encoding="utf-8",
            )

            result = ContextRetriever(max_files=1).retrieve(root, "fix payment refund")

        self.assertEqual([item.path for item in result.items], ["payment.py"])
        self.assertEqual(result.scanned_file_count, 2)
        self.assertTrue(result.truncated)

    def test_context_retriever_stops_at_scan_limit(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            root = Path(temp_dir)
            for index in range(20):
                (root / f"file_{index:02}.py").write_text(
                    f"def marker_{index}():\n    pass\n",
                    encoding="utf-8",
                )

            result = ContextRetriever(max_files=2, max_scan_files=5).retrieve(root, "marker")

        self.assertLessEqual(result.scanned_file_count, 5)
        self.assertTrue(result.truncated)


if __name__ == "__main__":
    unittest.main()
