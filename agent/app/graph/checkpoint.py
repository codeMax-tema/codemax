import json
from os import environ
from pathlib import Path

from app.graph.state import AgentState, utc_now


class CheckpointStore:
    def __init__(self, root: Path | None = None) -> None:
        self.root = root or default_checkpoint_root()

    def save(self, state: AgentState) -> AgentState:
        self.root.mkdir(parents=True, exist_ok=True)
        saved = state.model_copy(
            update={
                "checkpoint_index": state.checkpoint_index + 1,
                "updated_at": utc_now(),
            }
        )
        path = self.path_for(saved.task_id)
        temp_path = path.with_suffix(".tmp")
        temp_path.write_text(
            json.dumps(saved.model_dump(mode="json", by_alias=True), indent=2),
            encoding="utf-8",
        )
        temp_path.replace(path)
        return saved

    def load(self, task_id: str) -> AgentState | None:
        path = self.path_for(task_id)
        if not path.is_file():
            return None

        return AgentState.model_validate(json.loads(path.read_text(encoding="utf-8")))

    def exists(self, task_id: str) -> bool:
        return self.path_for(task_id).is_file()

    def path_for(self, task_id: str) -> Path:
        return self.root / f"{safe_file_stem(task_id)}.json"


def default_checkpoint_root() -> Path:
    configured = environ.get("CODEMAX_AGENT_CHECKPOINT_DIR", "").strip()
    if configured:
        return Path(configured).expanduser()

    return Path.home() / ".codemax" / "agent" / "checkpoints"


def safe_file_stem(value: str) -> str:
    stem = "".join(
        character
        if character.isascii() and (character.isalnum() or character in {"-", "_", "."})
        else "_"
        for character in value
    ).strip("._")
    return stem[:120] or "task"
