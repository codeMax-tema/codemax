from __future__ import annotations

import re

_REDACTED = "[REDACTED]"

_PRIVATE_KEY_PATTERN = re.compile(
    r"-----BEGIN [^-\r\n]*PRIVATE KEY-----.*?-----END [^-\r\n]*PRIVATE KEY-----",
    re.IGNORECASE | re.DOTALL,
)
_BEARER_PATTERN = re.compile(r"(?i)(\bAuthorization\s*:\s*Bearer\s+)([^\s,;]+)")
_URL_USERINFO_PATTERN = re.compile(
    r"(?i)(\b[a-z][a-z0-9+.-]*://)([^\s/@:]+)(?::([^\s/@]*))?@"
)
_SECRET_ASSIGNMENT_PATTERN = re.compile(
    r'''(?ix)
    (
        ["']?
        (?:[a-z0-9]+[_-])*
        (?:
            api[_-]?key|access[_-]?token|refresh[_-]?token|auth[_-]?token|token|
            client[_-]?secret|secret|password|passwd
        )
        ["']?
        (?:\s*[:=]\s*|\s+)
    )
    (?:"(?:\\.|[^"\\\r\n])*"|'(?:\\.|[^'\\\r\n])*'|[^\s,;}]+)
    '''
)
_PROVIDER_TOKEN_PATTERN = re.compile(
    r"(?<![A-Za-z0-9])(?:"
    r"sk-[A-Za-z0-9._-]{8,}|"
    r"gh[pousr]_[A-Za-z0-9._-]{8,}|"
    r"github_pat_[A-Za-z0-9_]{8,}|"
    r"xox[baprs]-[A-Za-z0-9._-]{8,}|"
    r"AKIA[A-Z0-9]{12,}|"
    r"glpat-[A-Za-z0-9_-]{8,}|"
    r"npm_[A-Za-z0-9]{8,}|"
    r"hf_[A-Za-z0-9]{8,}|"
    r"AIza[A-Za-z0-9_-]{20,}"
    r")"
)
_JWT_PATTERN = re.compile(
    r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b"
)
_HEX_32_PATTERN = re.compile(r"(?i)(?<![0-9a-f])[0-9a-f]{32}(?![0-9a-f])")


def redact_model_context(text: str) -> str:
    """Remove common credential forms before user-controlled context reaches a model."""

    redacted = _PRIVATE_KEY_PATTERN.sub(_REDACTED, text)
    redacted = _BEARER_PATTERN.sub(rf"\1{_REDACTED}", redacted)

    def redact_url_userinfo(match: re.Match[str]) -> str:
        password = f":{_REDACTED}" if match.group(3) is not None else ""
        return f"{match.group(1)}{_REDACTED}{password}@"

    redacted = _URL_USERINFO_PATTERN.sub(redact_url_userinfo, redacted)
    redacted = _SECRET_ASSIGNMENT_PATTERN.sub(rf"\1{_REDACTED}", redacted)
    redacted = _PROVIDER_TOKEN_PATTERN.sub(_REDACTED, redacted)
    redacted = _JWT_PATTERN.sub(_REDACTED, redacted)
    return _HEX_32_PATTERN.sub(_REDACTED, redacted)
