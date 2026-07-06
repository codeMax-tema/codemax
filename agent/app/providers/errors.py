from dataclasses import dataclass

from openai import (
    APIConnectionError,
    APIError,
    APIStatusError,
    APITimeoutError,
    AuthenticationError,
    BadRequestError,
    RateLimitError,
)


@dataclass(frozen=True, slots=True)
class ModelProviderError(Exception):
    code: str
    message: str
    http_status: int = 502

    def __str__(self) -> str:
        return self.message


def map_openai_error(error: Exception) -> ModelProviderError:
    if isinstance(error, APITimeoutError):
        return ModelProviderError(
            code="model.timeout",
            message=(
                "Model request timed out. Check the provider endpoint or increase "
                "CODEMAX_MODEL_TIMEOUT_SECONDS."
            ),
            http_status=504,
        )

    if isinstance(error, AuthenticationError):
        return ModelProviderError(
            code="model.authenticationFailed",
            message="Model authentication failed. Check CODEMAX_MODEL_API_KEY and Base URL.",
            http_status=401,
        )

    if isinstance(error, RateLimitError):
        return ModelProviderError(
            code="model.rateLimited",
            message="Model provider rate limit was reached. Retry later or switch model config.",
            http_status=429,
        )

    if isinstance(error, BadRequestError):
        return ModelProviderError(
            code="model.badRequest",
            message=safe_error_message(error, "Model provider rejected the request."),
            http_status=400,
        )

    if isinstance(error, APIConnectionError):
        return ModelProviderError(
            code="model.connectionFailed",
            message="Unable to connect to the model provider. Check Base URL and network access.",
            http_status=503,
        )

    if isinstance(error, APIStatusError):
        return map_openai_status_error(error)

    if isinstance(error, APIError):
        return ModelProviderError(
            code="model.providerError",
            message=safe_error_message(error, "Model provider returned an error."),
            http_status=502,
        )

    return ModelProviderError(
        code="model.unknownError",
        message=f"Unexpected model provider error: {type(error).__name__}",
        http_status=500,
    )


def map_openai_status_error(error: APIStatusError) -> ModelProviderError:
    if error.status_code == 401:
        return ModelProviderError(
            code="model.authenticationFailed",
            message="Model authentication failed. Check CODEMAX_MODEL_API_KEY and Base URL.",
            http_status=401,
        )

    if error.status_code == 429:
        return ModelProviderError(
            code="model.rateLimited",
            message="Model provider rate limit was reached. Retry later or switch model config.",
            http_status=429,
        )

    if error.status_code == 408:
        return ModelProviderError(
            code="model.timeout",
            message="Model provider timed out while processing the request.",
            http_status=504,
        )

    if 400 <= error.status_code < 500:
        return ModelProviderError(
            code="model.requestRejected",
            message=safe_error_message(error, "Model provider rejected the request."),
            http_status=400,
        )

    return ModelProviderError(
        code="model.providerUnavailable",
        message=safe_error_message(error, "Model provider is temporarily unavailable."),
        http_status=502,
    )


def safe_error_message(error: Exception, fallback: str) -> str:
    message = str(error).strip()
    if not message:
        return fallback

    return message[:500]
