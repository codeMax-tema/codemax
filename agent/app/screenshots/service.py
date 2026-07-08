from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path


@dataclass(frozen=True, slots=True)
class ScreenshotArtifact:
    task_id: str
    url: str
    status: str
    screenshot_path: str | None
    message: str


class ScreenshotService:
    def __init__(self, playwright_available: bool | None = None) -> None:
        self.playwright_available = (
            self._detect_playwright() if playwright_available is None else playwright_available
        )

    def capture(self, task_id: str, url: str, output_dir: str | Path) -> ScreenshotArtifact:
        output_path = Path(output_dir)
        output_path.mkdir(parents=True, exist_ok=True)
        if not self.playwright_available:
            return ScreenshotArtifact(
                task_id=task_id,
                url=url,
                status="browserUnavailable",
                screenshot_path=None,
                message="Playwright is not installed; screenshot capture was skipped.",
            )

        screenshot_path = output_path / f"{task_id}-screenshot.png"
        try:
            from playwright.sync_api import sync_playwright

            with sync_playwright() as playwright:
                browser = playwright.chromium.launch()
                try:
                    page = browser.new_page()
                    page.goto(url, wait_until="networkidle", timeout=15_000)
                    page.screenshot(path=str(screenshot_path), full_page=True)
                finally:
                    browser.close()
        except Exception as error:
            return ScreenshotArtifact(
                task_id=task_id,
                url=url,
                status="captureFailed",
                screenshot_path=None,
                message=f"Playwright screenshot capture failed: {error}",
            )

        if not screenshot_path.exists() or screenshot_path.stat().st_size == 0:
            return ScreenshotArtifact(
                task_id=task_id,
                url=url,
                status="captureFailed",
                screenshot_path=None,
                message="Playwright did not produce a non-empty screenshot file.",
            )

        return ScreenshotArtifact(
            task_id=task_id,
            url=url,
            status="captured",
            screenshot_path=str(screenshot_path),
            message="Screenshot metadata recorded.",
        )

    def _detect_playwright(self) -> bool:
        try:
            import playwright  # noqa: F401
        except Exception:
            return False
        return True
