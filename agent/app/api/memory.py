from fastapi import APIRouter, HTTPException, Query
from pydantic import BaseModel, ConfigDict, Field

from app.memory import (
    ConversationMessage,
    MemoryContextBundle,
    MemoryItem,
    MemorySafetyError,
    MemoryService,
    NewConversationMessage,
    RollingSummary,
    TempContextSummary,
)

router = APIRouter(prefix="/api/v1/memory", tags=["memory"])
_memory = MemoryService()


class MemoryModel(BaseModel):
    model_config = ConfigDict(populate_by_name=True)


class LoadMemoryContextRequest(MemoryModel):
    conversation_id: str = Field(alias="conversationId", min_length=1)
    repository_path: str | None = Field(default=None, alias="repositoryPath")
    task_id: str | None = Field(default=None, alias="taskId")
    query: str | None = None
    recent_message_limit: int | None = Field(default=None, alias="recentMessageLimit")


class SummarizeConversationRequest(MemoryModel):
    conversation_id: str = Field(alias="conversationId", min_length=1)
    recent_message_limit: int | None = Field(default=None, alias="recentMessageLimit")


class ExtractMemoriesRequest(MemoryModel):
    conversation_id: str | None = Field(default=None, alias="conversationId")
    repository_path: str | None = Field(default=None, alias="repositoryPath")
    task_id: str | None = Field(default=None, alias="taskId")


class TempContextSummaryRequest(MemoryModel):
    task_id: str = Field(alias="taskId", min_length=1)
    items: list[str] = Field(min_length=1)


@router.post("/messages", response_model=ConversationMessage)
def add_memory_message(request: NewConversationMessage) -> ConversationMessage:
    try:
        return _memory.add_message(request)
    except MemorySafetyError as error:
        raise memory_safety_http_error(error) from error


@router.post("/context", response_model=MemoryContextBundle)
def load_memory_context(request: LoadMemoryContextRequest) -> MemoryContextBundle:
    return _memory.load_context(
        conversation_id=request.conversation_id,
        repository_path=request.repository_path,
        task_id=request.task_id,
        query=request.query,
        recent_message_limit=request.recent_message_limit,
    )


@router.post("/summaries", response_model=RollingSummary | None)
def summarize_conversation(request: SummarizeConversationRequest) -> RollingSummary | None:
    return _memory.summarize_conversation(
        conversation_id=request.conversation_id,
        recent_message_limit=request.recent_message_limit,
    )


@router.post("/extract", response_model=list[MemoryItem])
def extract_memories(request: ExtractMemoriesRequest) -> list[MemoryItem]:
    return _memory.extract_memories(
        conversation_id=request.conversation_id,
        repository_path=request.repository_path,
        task_id=request.task_id,
    )


@router.get("/search", response_model=list[MemoryItem])
def search_memories(
    repository_path: str | None = Query(default=None, alias="repositoryPath"),
    task_id: str | None = Query(default=None, alias="taskId"),
    query: str | None = None,
    limit: int = Query(default=20, ge=1, le=100),
) -> list[MemoryItem]:
    return _memory.retrieve_memories(
        repository_path=repository_path,
        task_id=task_id,
        query=query,
        limit=limit,
    )


@router.post("/temp-context/summary", response_model=TempContextSummary)
def summarize_temp_context(request: TempContextSummaryRequest) -> TempContextSummary:
    try:
        return _memory.summarize_temp_context(task_id=request.task_id, items=request.items)
    except MemorySafetyError as error:
        raise memory_safety_http_error(error) from error


def memory_safety_http_error(error: MemorySafetyError) -> HTTPException:
    return HTTPException(
        status_code=400,
        detail={"code": "memory.safetyRejected", "message": error.message},
    )
