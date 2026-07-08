from __future__ import annotations

from collections import OrderedDict, deque
from dataclasses import dataclass, field
from threading import Lock


@dataclass(frozen=True, slots=True)
class ScheduledTask:
    task_id: str
    status: str
    message: str = ""


@dataclass(frozen=True, slots=True)
class SchedulerSnapshot:
    max_concurrent_tasks: int
    running_task_ids: list[str] = field(default_factory=list)
    queued_task_ids: list[str] = field(default_factory=list)
    completed_task_ids: list[str] = field(default_factory=list)
    failed_task_ids: list[str] = field(default_factory=list)
    cancelled_task_ids: list[str] = field(default_factory=list)


class TaskScheduler:
    def __init__(self, max_concurrent_tasks: int = 2) -> None:
        if max_concurrent_tasks < 1:
            raise ValueError("max_concurrent_tasks must be at least 1")
        self.max_concurrent_tasks = max_concurrent_tasks
        self._tasks: OrderedDict[str, ScheduledTask] = OrderedDict()
        self._queue: deque[str] = deque()
        self._lock = Lock()

    def submit(self, task_id: str) -> ScheduledTask:
        clean_task_id = task_id.strip()
        if not clean_task_id:
            raise ValueError("task_id is required")

        with self._lock:
            if clean_task_id in self._tasks:
                return self._tasks[clean_task_id]

            status = "running" if self._running_count_locked() < self.max_concurrent_tasks else "queued"
            task = ScheduledTask(task_id=clean_task_id, status=status)
            self._tasks[clean_task_id] = task
            if status == "queued":
                self._queue.append(clean_task_id)
            return task

    def finish(self, task_id: str, *, success: bool, message: str = "") -> ScheduledTask:
        with self._lock:
            task = self._required_task_locked(task_id)
            terminal_status = "completed" if success else "failed"
            finished = ScheduledTask(task_id=task.task_id, status=terminal_status, message=message)
            self._tasks[task.task_id] = finished
            self._promote_next_locked()
            return finished

    def cancel(self, task_id: str, message: str = "") -> ScheduledTask:
        with self._lock:
            task = self._required_task_locked(task_id)
            if task.status == "queued":
                self._queue = deque(item for item in self._queue if item != task.task_id)
            cancelled = ScheduledTask(task_id=task.task_id, status="cancelled", message=message)
            self._tasks[task.task_id] = cancelled
            self._promote_next_locked()
            return cancelled

    def status(self, task_id: str) -> ScheduledTask:
        with self._lock:
            return self._required_task_locked(task_id)

    def snapshot(self) -> SchedulerSnapshot:
        with self._lock:
            grouped = {
                "running": [],
                "queued": [],
                "completed": [],
                "failed": [],
                "cancelled": [],
            }
            for task in self._tasks.values():
                if task.status in grouped:
                    grouped[task.status].append(task.task_id)

            return SchedulerSnapshot(
                max_concurrent_tasks=self.max_concurrent_tasks,
                running_task_ids=grouped["running"],
                queued_task_ids=grouped["queued"],
                completed_task_ids=grouped["completed"],
                failed_task_ids=grouped["failed"],
                cancelled_task_ids=grouped["cancelled"],
            )

    def _required_task_locked(self, task_id: str) -> ScheduledTask:
        clean_task_id = task_id.strip()
        if clean_task_id not in self._tasks:
            raise KeyError(f"Unknown scheduled task: {clean_task_id}")
        return self._tasks[clean_task_id]

    def _running_count_locked(self) -> int:
        return sum(1 for task in self._tasks.values() if task.status == "running")

    def _promote_next_locked(self) -> None:
        while self._queue and self._running_count_locked() < self.max_concurrent_tasks:
            next_task_id = self._queue.popleft()
            current = self._tasks.get(next_task_id)
            if current is None or current.status != "queued":
                continue
            self._tasks[next_task_id] = ScheduledTask(task_id=next_task_id, status="running")
