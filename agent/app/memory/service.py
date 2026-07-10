import hashlib
import json
import re
from datetime import UTC, datetime
from os import environ
from pathlib import Path
from threading import Lock
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field

DEFAULT_RECENT_MESSAGE_LIMIT = 50
MAX_RECENT_MESSAGE_LIMIT = 200
VISIBLE_ROLES = {"system", "user", "assistant", "tool"}
FORBIDDEN_ROLES = {"analysis", "reasoning", "internal", "chain_of_thought"}
FORBIDDEN_CONTENT_MARKERS = (
    "<analysis>",
    "</analysis>",
    "chain-of-thought",
    "chain of thought",
    "internal reasoning",
    "hidden reasoning",
)


def utc_now() -> str:
    return datetime.now(tz=UTC).isoformat()


class MemoryModel(BaseModel):
    model_config = ConfigDict(populate_by_name=True)


class ConversationMessage(MemoryModel):
    id: str
    conversation_id: str = Field(alias="conversationId")
    role: Literal["system", "user", "assistant", "tool"]
    content: str
    task_id: str | None = Field(default=None, alias="taskId")
    repository_path: str | None = Field(default=None, alias="repositoryPath")
    token_count: int | None = Field(default=None, alias="tokenCount")
    retention_class: str = Field(default="recent", alias="retentionClass")
    created_at: str = Field(default_factory=utc_now, alias="createdAt")


class NewConversationMessage(MemoryModel):
    id: str | None = None
    conversation_id: str = Field(alias="conversationId")
    role: str
    content: str
    task_id: str | None = Field(default=None, alias="taskId")
    repository_path: str | None = Field(default=None, alias="repositoryPath")
    token_count: int | None = Field(default=None, alias="tokenCount")
    retention_class: str = Field(default="recent", alias="retentionClass")


class RollingSummary(MemoryModel):
    id: str
    conversation_id: str = Field(alias="conversationId")
    covered_message_count: int = Field(alias="coveredMessageCount")
    summary: str
    updated_at: str = Field(default_factory=utc_now, alias="updatedAt")


class MemoryItem(MemoryModel):
    id: str
    scope: Literal["global", "repository", "task"]
    scope_id: str | None = Field(default=None, alias="scopeId")
    key: str
    value: str
    category: Literal["preference", "repository_command", "solution_choice", "approval_decision"]
    confidence: float = 0.7
    source: str
    is_user_editable: bool = Field(default=True, alias="isUserEditable")
    created_at: str = Field(default_factory=utc_now, alias="createdAt")
    updated_at: str = Field(default_factory=utc_now, alias="updatedAt")


class MemoryContextBundle(MemoryModel):
    conversation_id: str = Field(alias="conversationId")
    recent_message_limit: int = Field(alias="recentMessageLimit")
    recent_messages: list[ConversationMessage] = Field(alias="recentMessages")
    rolling_summary: RollingSummary | None = Field(default=None, alias="rollingSummary")
    long_term_memories: list[MemoryItem] = Field(alias="longTermMemories")
    safety_notice: str = Field(alias="safetyNotice")


class TempContextSummary(MemoryModel):
    id: str
    task_id: str = Field(alias="taskId")
    summary: str
    source_item_count: int = Field(alias="sourceItemCount")
    created_at: str = Field(default_factory=utc_now, alias="createdAt")


class MemorySafetyError(Exception):
    def __init__(self, message: str) -> None:
        super().__init__(message)
        self.message = message


class MemoryStoreData(MemoryModel):
    messages: list[ConversationMessage] = Field(default_factory=list)
    summaries: list[RollingSummary] = Field(default_factory=list)
    memories: list[MemoryItem] = Field(default_factory=list)
    temp_context_summaries: list[TempContextSummary] = Field(
        default_factory=list,
        alias="tempContextSummaries",
    )


class MemoryService:
    def __init__(self, root: Path | None = None, recent_message_limit: int | None = None) -> None:
        self.root = root or default_memory_root()
        self.recent_message_limit = clamp_recent_message_limit(
            recent_message_limit or default_recent_message_limit()
        )
        self.path = self.root / "memory.json"
        self.lock = Lock()

    def add_message(self, message: NewConversationMessage) -> ConversationMessage:
        role = normalize_role(message.role)
        content = sanitize_visible_content(message.content)
        saved = ConversationMessage(
            id=message.id
            or stable_id("message", message.conversation_id, role, content, utc_now()),
            conversationId=message.conversation_id,
            role=role,
            content=content,
            taskId=message.task_id,
            repositoryPath=message.repository_path,
            tokenCount=message.token_count,
            retentionClass=message.retention_class,
        )

        with self.lock:
            data = self.load_data()
            data.messages.append(saved)
            extracted = extract_memories_from_message(saved)
            data.memories = upsert_memories(data.memories, persistable_memories(extracted))
            self.save_data(data)

        return saved

    def load_context(
        self,
        conversation_id: str,
        repository_path: str | None = None,
        task_id: str | None = None,
        query: str | None = None,
        recent_message_limit: int | None = None,
    ) -> MemoryContextBundle:
        limit = clamp_recent_message_limit(recent_message_limit or self.recent_message_limit)

        with self.lock:
            data = self.load_data()
            messages = [
                message
                for message in data.messages
                if message.conversation_id == conversation_id
                and message_is_relevant(message, task_id)
            ]
            recent_messages = messages[-limit:]
            older_messages = messages[: max(len(messages) - limit, 0)]
            summary = summarize_messages(conversation_id, older_messages)
            if summary is not None:
                data.summaries = upsert_summary(data.summaries, summary)
                self.save_data(data)

            memories = retrieve_relevant_memories(
                data.memories,
                repository_path=repository_path,
                task_id=task_id,
                query=query,
                limit=20,
            )

        return MemoryContextBundle(
            conversationId=conversation_id,
            recentMessageLimit=limit,
            recentMessages=recent_messages,
            rollingSummary=summary,
            longTermMemories=memories,
            safetyNotice=(
                "Only user-visible messages, summaries, decisions, and results "
                "are stored."
            ),
        )

    def summarize_conversation(
        self,
        conversation_id: str,
        recent_message_limit: int | None = None,
    ) -> RollingSummary | None:
        limit = clamp_recent_message_limit(recent_message_limit or self.recent_message_limit)

        with self.lock:
            data = self.load_data()
            messages = [
                message for message in data.messages if message.conversation_id == conversation_id
            ]
            summary = summarize_messages(conversation_id, messages[: max(len(messages) - limit, 0)])
            if summary is not None:
                data.summaries = upsert_summary(data.summaries, summary)
                self.save_data(data)

        return summary

    def extract_memories(
        self,
        conversation_id: str | None = None,
        repository_path: str | None = None,
        task_id: str | None = None,
    ) -> list[MemoryItem]:
        with self.lock:
            data = self.load_data()
            candidates = [
                message
                for message in data.messages
                if (conversation_id is None or message.conversation_id == conversation_id)
                and (repository_path is None or message.repository_path == repository_path)
                and message_is_relevant(message, task_id)
            ]
            extracted = [
                memory
                for message in candidates
                for memory in extract_memories_from_message(message)
            ]
            data.memories = upsert_memories(data.memories, persistable_memories(extracted))
            self.save_data(data)

        return extracted

    def retrieve_memories(
        self,
        repository_path: str | None = None,
        task_id: str | None = None,
        query: str | None = None,
        limit: int = 20,
    ) -> list[MemoryItem]:
        with self.lock:
            data = self.load_data()
            return retrieve_relevant_memories(
                data.memories,
                repository_path=repository_path,
                task_id=task_id,
                query=query,
                limit=limit,
            )

    def summarize_temp_context(self, task_id: str, items: list[str]) -> TempContextSummary:
        visible_items = [sanitize_visible_content(item) for item in items if item.strip()]
        summary = summarize_text_items(visible_items)
        saved = TempContextSummary(
            id=stable_id("temp-context", task_id, summary),
            taskId=task_id,
            summary=summary,
            sourceItemCount=len(visible_items),
        )

        with self.lock:
            data = self.load_data()
            data.temp_context_summaries = [
                item for item in data.temp_context_summaries if item.id != saved.id
            ]
            data.temp_context_summaries.append(saved)
            self.save_data(data)

        return saved

    def load_data(self) -> MemoryStoreData:
        if not self.path.is_file():
            return MemoryStoreData()

        return MemoryStoreData.model_validate(json.loads(self.path.read_text(encoding="utf-8")))

    def save_data(self, data: MemoryStoreData) -> None:
        self.root.mkdir(parents=True, exist_ok=True)
        temp_path = self.path.with_suffix(".tmp")
        temp_path.write_text(
            json.dumps(data.model_dump(mode="json", by_alias=True), indent=2),
            encoding="utf-8",
        )
        temp_path.replace(self.path)


def clamp_recent_message_limit(value: int) -> int:
    if value < 1:
        return 1
    if value > MAX_RECENT_MESSAGE_LIMIT:
        return MAX_RECENT_MESSAGE_LIMIT
    return value


def default_memory_root() -> Path:
    configured = environ.get("CODEMAX_AGENT_MEMORY_DIR", "").strip()
    if configured:
        return Path(configured).expanduser()

    return Path.home() / ".codemax" / "agent" / "memory"


def default_recent_message_limit() -> int:
    value = environ.get("CODEMAX_KEEP_RECENT_MESSAGES", str(DEFAULT_RECENT_MESSAGE_LIMIT))
    try:
        return int(value)
    except ValueError:
        return DEFAULT_RECENT_MESSAGE_LIMIT


def normalize_role(role: str) -> Literal["system", "user", "assistant", "tool"]:
    normalized = role.strip().lower()
    if normalized in FORBIDDEN_ROLES:
        raise MemorySafetyError("Internal reasoning roles cannot be saved to memory.")
    if normalized not in VISIBLE_ROLES:
        raise MemorySafetyError(f"Unsupported visible message role: {role}")
    return normalized  # type: ignore[return-value]


def sanitize_visible_content(content: str) -> str:
    value = content.strip()
    lowered = value.lower()
    if any(marker in lowered for marker in FORBIDDEN_CONTENT_MARKERS):
        raise MemorySafetyError("Internal reasoning content cannot be saved to memory.")
    if not value:
        raise MemorySafetyError("Empty content cannot be saved to memory.")
    return value[:20_000]


def message_is_relevant(message: ConversationMessage, task_id: str | None) -> bool:
    return task_id is None or message.task_id in {None, task_id}


def summarize_messages(
    conversation_id: str,
    messages: list[ConversationMessage],
) -> RollingSummary | None:
    if not messages:
        return None

    excerpts = [compact_excerpt(message.content) for message in messages[-12:]]
    role_counts = role_count_text(messages)
    summary = (
        f"Compressed {len(messages)} older visible messages for conversation "
        f"{conversation_id}. Roles: {role_counts}. Key excerpts: " + " | ".join(excerpts)
    )
    return RollingSummary(
        id=stable_id("summary", conversation_id),
        conversationId=conversation_id,
        coveredMessageCount=len(messages),
        summary=summary,
    )


def summarize_text_items(items: list[str]) -> str:
    if not items:
        return "No temporary context items were provided."

    excerpts = [compact_excerpt(item) for item in items[-12:]]
    return f"Compressed {len(items)} temporary context items. Key excerpts: " + " | ".join(excerpts)


def role_count_text(messages: list[ConversationMessage]) -> str:
    counts: dict[str, int] = {}
    for message in messages:
        counts[message.role] = counts.get(message.role, 0) + 1
    return ", ".join(f"{role}={count}" for role, count in sorted(counts.items()))


def compact_excerpt(value: str, limit: int = 180) -> str:
    text = re.sub(r"\s+", " ", value).strip()
    return text if len(text) <= limit else f"{text[: limit - 3]}..."


def extract_memories_from_message(message: ConversationMessage) -> list[MemoryItem]:
    if message.role not in VISIBLE_ROLES:
        return []

    content = message.content
    memories: list[MemoryItem] = []
    scope, scope_id = memory_scope_for_message(message)

    if looks_like_preference(content):
        memories.append(
            build_memory_item(
                category="preference",
                scope=scope,
                scope_id=scope_id,
                key_prefix="preference",
                value=compact_excerpt(content, 500),
                source=f"message:{message.id}",
                confidence=0.75,
            )
        )

    for command in extract_commands(content):
        memories.append(
            build_memory_item(
                category="repository_command",
                scope="repository" if message.repository_path else scope,
                scope_id=message.repository_path or scope_id,
                key_prefix="repository.command",
                value=command,
                source=f"message:{message.id}",
                confidence=0.85,
            )
        )

    if looks_like_solution_choice(content):
        memories.append(
            build_memory_item(
                category="solution_choice",
                scope=scope,
                scope_id=scope_id,
                key_prefix="decision",
                value=compact_excerpt(content, 500),
                source=f"message:{message.id}",
                confidence=0.7,
            )
        )

    if looks_like_approval_decision(content):
        memories.append(
            build_memory_item(
                category="approval_decision",
                scope="task" if message.task_id else scope,
                scope_id=message.task_id or scope_id,
                key_prefix="approval.decision",
                value=compact_excerpt(content, 500),
                source=f"message:{message.id}",
                confidence=0.8,
            )
        )

    return memories


def memory_scope_for_message(
    message: ConversationMessage,
) -> tuple[Literal["global", "repository", "task"], str | None]:
    if message.task_id:
        return "task", message.task_id
    if message.repository_path:
        return "repository", message.repository_path
    return "global", None


def looks_like_preference(content: str) -> bool:
    lowered = content.lower()
    return any(
        marker in lowered
        for marker in [
            "prefer",
            "preference",
            "default to",
            "以后",
            "偏好",
            "默认",
            "用中文",
            "不要",
            "请记住",
        ]
    )


def extract_commands(content: str) -> list[str]:
    pattern = re.compile(
        r"\b((?:npm|pnpm|yarn|pytest|ruff|cargo|go|mvn|gradle|python)\s+[^\n\r;`]{1,160})",
        re.IGNORECASE,
    )
    return [compact_excerpt(match.group(1), 200) for match in pattern.finditer(content)]


def looks_like_solution_choice(content: str) -> bool:
    lowered = content.lower()
    return any(
        marker in lowered
        for marker in ["choose", "chosen", "decision", "方案", "采用", "选择", "决定"]
    )


def looks_like_approval_decision(content: str) -> bool:
    lowered = content.lower()
    return any(
        marker in lowered
        for marker in ["approved", "rejected", "approval", "同意", "拒绝", "批准", "审批"]
    )


def build_memory_item(
    category: Literal["preference", "repository_command", "solution_choice", "approval_decision"],
    scope: Literal["global", "repository", "task"],
    scope_id: str | None,
    key_prefix: str,
    value: str,
    source: str,
    confidence: float,
) -> MemoryItem:
    key = f"{key_prefix}.{short_hash(scope, scope_id or '', value)}"
    return MemoryItem(
        id=stable_id("memory", scope, scope_id or "", key),
        scope=scope,
        scopeId=scope_id,
        key=key,
        value=value,
        category=category,
        confidence=confidence,
        source=source,
    )


def upsert_memories(existing: list[MemoryItem], new_items: list[MemoryItem]) -> list[MemoryItem]:
    by_id = {item.id: item for item in existing}
    for item in new_items:
        current = by_id.get(item.id)
        if current is None:
            by_id[item.id] = item
        else:
            by_id[item.id] = item.model_copy(update={"created_at": current.created_at})
    return sorted(by_id.values(), key=lambda item: item.updated_at)


def persistable_memories(items: list[MemoryItem]) -> list[MemoryItem]:
    return [item for item in items if item.category != "preference"]


def upsert_summary(existing: list[RollingSummary], summary: RollingSummary) -> list[RollingSummary]:
    return [item for item in existing if item.id != summary.id] + [summary]


def retrieve_relevant_memories(
    memories: list[MemoryItem],
    repository_path: str | None,
    task_id: str | None,
    query: str | None,
    limit: int,
) -> list[MemoryItem]:
    query_terms = set(normalize_terms(query or ""))
    scored = []
    for memory in memories:
        score = 0.0
        if memory.scope == "global":
            score += 0.5
        if repository_path and memory.scope == "repository" and memory.scope_id == repository_path:
            score += 3.0
        if task_id and memory.scope == "task" and memory.scope_id == task_id:
            score += 4.0

        if query_terms:
            memory_terms = set(normalize_terms(f"{memory.key} {memory.value} {memory.category}"))
            score += len(query_terms & memory_terms)

        if score > 0:
            scored.append((score, memory.updated_at, memory))

    scored.sort(key=lambda item: (item[0], item[1]), reverse=True)
    return [memory for _, _, memory in scored[: max(limit, 1)]]


def normalize_terms(value: str) -> list[str]:
    return re.findall(r"[\w\u4e00-\u9fff]+", value.lower())


def stable_id(*parts: str) -> str:
    return short_hash(*parts, length=24)


def short_hash(*parts: str, length: int = 12) -> str:
    digest = hashlib.sha256("\n".join(parts).encode("utf-8")).hexdigest()
    return digest[:length]
