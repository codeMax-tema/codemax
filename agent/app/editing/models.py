from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field


class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)


class StructuredTodo(StrictModel):
    id: str = Field(min_length=1)
    title: str = Field(min_length=1)
    description: str = ""


class TodoPlan(StrictModel):
    todos: list[StructuredTodo] = Field(min_length=1)


class CreateEdit(StrictModel):
    operation: Literal["create"]
    path: str = Field(min_length=1)
    content: str
    summary: str = Field(min_length=1)


class UpdateEdit(StrictModel):
    operation: Literal["update"]
    path: str = Field(min_length=1)
    content: str
    summary: str = Field(min_length=1)


class DeleteEdit(StrictModel):
    operation: Literal["delete"]
    path: str = Field(min_length=1)
    summary: str = Field(min_length=1)


FileEdit = Annotated[CreateEdit | UpdateEdit | DeleteEdit, Field(discriminator="operation")]


class EditingPlan(StrictModel):
    edits: list[FileEdit] = Field(min_length=1)
