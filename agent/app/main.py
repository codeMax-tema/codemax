from fastapi import FastAPI

from app.api.health import router as health_router
from app.api.memory import router as memory_router
from app.api.models import router as models_router
from app.api.tasks import router as tasks_router
from app.core.config import load_settings


def create_app() -> FastAPI:
    app = FastAPI(title="Codemax Agent Engine", version="0.0.0")
    app.include_router(health_router)
    app.include_router(memory_router)
    app.include_router(models_router)
    app.include_router(tasks_router)
    return app


app = create_app()


def main() -> None:
    import os
    import sys

    import uvicorn

    if sys.stdout is None:
        sys.stdout = open(os.devnull, "w", encoding="utf-8")
    if sys.stderr is None:
        sys.stderr = open(os.devnull, "w", encoding="utf-8")

    settings = load_settings()
    uvicorn.run(
        app,
        host=settings.host,
        port=settings.port,
        log_level=settings.log_level,
    )


if __name__ == "__main__":
    main()
