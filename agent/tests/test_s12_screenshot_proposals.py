import tempfile
import unittest
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from app.proposals import ProposalService
from app.screenshots import ScreenshotService


class S12ScreenshotProposalTests(unittest.TestCase):
    def test_screenshot_service_records_artifact_metadata_without_sqlite_blob(self):
        service = ScreenshotService(playwright_available=False)

        with tempfile.TemporaryDirectory() as temp_dir:
            result = service.capture(
                task_id="task-1",
                url="http://127.0.0.1:5173",
                output_dir=Path(temp_dir),
            )

        self.assertEqual(result.status, "browserUnavailable")
        self.assertIsNone(result.screenshot_path)
        self.assertEqual(result.task_id, "task-1")

    def test_screenshot_service_does_not_report_empty_placeholder_as_captured(self):
        service = ScreenshotService(playwright_available=True)

        with tempfile.TemporaryDirectory() as temp_dir:
            result = service.capture(
                task_id="task-1",
                url="http://127.0.0.1:9/unavailable",
                output_dir=Path(temp_dir),
            )

        self.assertEqual(result.status, "captureFailed")
        self.assertIsNone(result.screenshot_path)

    def test_proposal_service_generates_selectable_recommended_options(self):
        proposals = ProposalService().generate(
            "Refactor payment module",
            "Need safer checkout boundary",
        )

        self.assertGreaterEqual(len(proposals), 2)
        self.assertLessEqual(len(proposals), 3)
        self.assertEqual(sum(1 for proposal in proposals if proposal.recommended), 1)
        self.assertTrue(all(proposal.risks for proposal in proposals))

    def test_proposal_service_regenerates_with_feedback(self):
        service = ProposalService()
        original = service.generate(
            "Refactor payment module",
            "Need safer checkout boundary",
        )

        regenerated = service.regenerate(
            "Refactor payment module",
            "Prefer smaller diff",
            original,
        )

        self.assertNotEqual(regenerated[0].id, original[0].id)
        self.assertIn("smaller diff", regenerated[0].rationale.lower())


if __name__ == "__main__":
    unittest.main()
