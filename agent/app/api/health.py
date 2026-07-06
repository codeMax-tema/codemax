from typing import Literal

from fastapi import APIRouter
from pydantic import BaseModel

from app import __version__

router = APIRouter(prefix="/health", tags=["health"])


class HealthResponse(BaseModel):
    service: str
    status: Literal["ok"]
    version: str


@router.get("", response_model=HealthResponse)
def health() -> HealthResponse:
    return HealthResponse(service="codemax-agent", status="ok", version=__version__)
