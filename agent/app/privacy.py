from __future__ import annotations

import base64
import json
import re
from dataclasses import dataclass
from typing import Any
from urllib.parse import unquote

_REDACTED = "[REDACTED]"
_BLOCKED = "[BLOCKED: sensitive content omitted before model request]"

_PRIVATE_MATERIAL_PATTERN = re.compile(
    r"-----BEGIN [^-\r\n]*(?:PRIVATE KEY|CERTIFICATE)-----.*?-----END [^-\r\n]*(?:PRIVATE KEY|CERTIFICATE)-----",
    re.IGNORECASE | re.DOTALL,
)
_BEARER_PATTERN = re.compile(r"(?i)(\bAuthorization\s*:\s*Bearer\s+)([^\s,;]+)")
_URL_USERINFO_PATTERN = re.compile(r"(?i)(\b[a-z][a-z0-9+.-]*://)([^\s/@:]+)(?::([^\s/@]*))?@")
_SECRET_ASSIGNMENT_PATTERN = re.compile(
    r'''(?ix)
    (
        ["']?
        (?:[a-z0-9]+[_-])*
        (?:
            api[_-]?key|access[_-]?token|refresh[_-]?token|auth[_-]?token|token|
            client[_-]?secret|secret|password|passwd|credential|authorization
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
_JWT_PATTERN = re.compile(r"\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b")
_HEX_32_PATTERN = re.compile(r"(?i)(?<![0-9a-f])[0-9a-f]{32}(?![0-9a-f])")
_WINDOWS_USER_PATH = re.compile(r"(?i)\b([a-z]:[\\/]+users[\\/]+)([^\\/\s]+)")
_POSIX_USER_PATH = re.compile(r"(?i)(?<![A-Za-z0-9_])(/(?:users|home)/)([^/\s]+)")
_BASE64_CANDIDATE = re.compile(r"(?<![A-Za-z0-9+/=])[A-Za-z0-9+/]{24,}={0,2}(?![A-Za-z0-9+/=])")
_PERCENT_CANDIDATE = re.compile(r"(?:%[0-9A-Fa-f]{2}){6,}")
_UNICODE_ESCAPE_CANDIDATE = re.compile(r"(?:\\u[0-9A-Fa-f]{4}){4,}")
_SENSITIVE_KEY_MARKERS = ("token", "secret", "password", "passwd", "credential", "authorization", "api_key", "apikey")


@dataclass(frozen=True, slots=True)
class SanitizedPayload:
    value: Any
    action: str
    sensitivity_level: str
    findings: tuple[str, ...]
    redacted: bool
    blocked: bool
    original_size_bytes: int
    tokens_estimate: int


def estimate_tokens(value: object) -> int:
    try:
        serialized = json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True)
    except (TypeError, ValueError):
        serialized = str(value)
    return max(1, len(serialized) // 4)


def redact_model_context(text: str) -> str:
    """Remove common credentials and user-identifying home paths from model-bound text."""
    return _sanitize_text(text)[0]


def sanitize_model_payload(value: Any, source_ref: str) -> SanitizedPayload:
    findings: set[str] = set()
    redacted = False
    blocked = False

    def visit(item: Any, path: str, key_sensitive: bool = False) -> Any:
        nonlocal redacted, blocked
        if isinstance(item, str):
            if key_sensitive:
                findings.add("sensitive_json_field")
                redacted = True
                return _REDACTED
            safe, text_findings, text_redacted, text_blocked = _sanitize_text(item)
            findings.update(text_findings)
            redacted = redacted or text_redacted
            blocked = blocked or text_blocked
            return safe
        if isinstance(item, list):
            return [visit(child, f"{path}[{index}]") for index, child in enumerate(item)]
        if isinstance(item, tuple):
            return tuple(visit(child, f"{path}[{index}]") for index, child in enumerate(item))
        if isinstance(item, dict):
            return {
                key: visit(child, f"{path}.{key}", _is_sensitive_key(str(key)))
                for key, child in item.items()
            }
        if item is None or isinstance(item, (bool, int, float)):
            return item
        raise TypeError(f"Unsupported model payload type at {path}: {type(item).__name__}")

    safe_value = visit(value, source_ref)
    canonical = json.dumps(value, ensure_ascii=False, separators=(",", ":"), sort_keys=True, default=str)
    if _contains_fragmented_secret(canonical):
        findings.add("fragmented_or_escaped_secret")
        blocked = True
    if blocked:
        safe_value = _blocked_shape(value)
    action = "blocked" if blocked else "redacted" if redacted else "allowed"
    sensitivity = "blocked" if blocked else "high" if redacted else "none"
    return SanitizedPayload(
        value=safe_value,
        action=action,
        sensitivity_level=sensitivity,
        findings=tuple(sorted(findings)),
        redacted=redacted,
        blocked=blocked,
        original_size_bytes=len(canonical.encode("utf-8")),
        tokens_estimate=estimate_tokens(safe_value),
    )


def sanitize_exception_message(text: str) -> str:
    safe = redact_model_context(text)
    return safe if safe != text else "Model provider request failed."


def _sanitize_text(text: str) -> tuple[str, set[str], bool, bool]:
    findings: set[str] = set()
    redacted = text
    blocked = False

    if _PRIVATE_MATERIAL_PATTERN.search(redacted):
        findings.add("private_key_or_certificate")
        blocked = True
        redacted = _PRIVATE_MATERIAL_PATTERN.sub(_REDACTED, redacted)

    redacted, encoded_findings, encoded_blocked = _redact_encoded_values(redacted)
    findings.update(encoded_findings)
    blocked = blocked or encoded_blocked

    def redact_url_userinfo(match: re.Match[str]) -> str:
        password = f":{_REDACTED}" if match.group(3) is not None else ""
        return f"{match.group(1)}{_REDACTED}{password}@"

    before = redacted
    redacted = _BEARER_PATTERN.sub(rf"\1{_REDACTED}", redacted)
    redacted = _URL_USERINFO_PATTERN.sub(redact_url_userinfo, redacted)
    redacted = _SECRET_ASSIGNMENT_PATTERN.sub(rf"\1{_REDACTED}", redacted)
    redacted = _PROVIDER_TOKEN_PATTERN.sub(_REDACTED, redacted)
    redacted = _JWT_PATTERN.sub(_REDACTED, redacted)
    redacted = _HEX_32_PATTERN.sub(_REDACTED, redacted)
    redacted = _WINDOWS_USER_PATH.sub(rf"\1[USER]", redacted)
    redacted = _POSIX_USER_PATH.sub(rf"\1[USER]", redacted)
    if redacted != before:
        findings.add("credential_or_user_path")

    if _contains_fragmented_secret(text):
        findings.add("fragmented_or_escaped_secret")
        blocked = True

    return redacted, findings, redacted != text, blocked


def _redact_encoded_values(text: str) -> tuple[str, set[str], bool]:
    findings: set[str] = set()
    blocked = False

    def replace_percent(match: re.Match[str]) -> str:
        nonlocal blocked
        decoded = unquote(match.group(0))
        if _looks_sensitive(decoded):
            findings.add("percent_encoded_secret")
            blocked = True
            return _REDACTED
        return match.group(0)

    def replace_unicode(match: re.Match[str]) -> str:
        nonlocal blocked
        try:
            decoded = json.loads(f'"{match.group(0)}"')
        except (json.JSONDecodeError, UnicodeDecodeError):
            return match.group(0)
        if _looks_sensitive(decoded):
            findings.add("unicode_escaped_secret")
            blocked = True
            return _REDACTED
        return match.group(0)

    def replace_base64(match: re.Match[str]) -> str:
        nonlocal blocked
        candidate = match.group(0)
        try:
            decoded = base64.b64decode(candidate, validate=True).decode("utf-8", errors="strict")
        except (ValueError, UnicodeDecodeError):
            return candidate
        if _looks_sensitive(decoded):
            findings.add("base64_encoded_secret")
            blocked = True
            return _REDACTED
        return candidate

    redacted = _PERCENT_CANDIDATE.sub(replace_percent, text)
    redacted = _UNICODE_ESCAPE_CANDIDATE.sub(replace_unicode, redacted)
    redacted = _BASE64_CANDIDATE.sub(replace_base64, redacted)
    return redacted, findings, blocked


def _looks_sensitive(value: str) -> bool:
    return bool(
        _PRIVATE_MATERIAL_PATTERN.search(value)
        or _PROVIDER_TOKEN_PATTERN.search(value)
        or _JWT_PATTERN.search(value)
        or _SECRET_ASSIGNMENT_PATTERN.search(value)
        or _BEARER_PATTERN.search(value)
    )


def _contains_private_material(value: str) -> bool:
    return bool(_PRIVATE_MATERIAL_PATTERN.search(value))


def _contains_fragmented_secret(value: str) -> bool:
    compact = re.sub(r'''[\s"'`+\\/,;:{}\[\]()]+''', "", value)
    return bool(_PROVIDER_TOKEN_PATTERN.search(compact) or _PRIVATE_MATERIAL_PATTERN.search(compact))


def _is_sensitive_key(key: str) -> bool:
    normalized = key.casefold().replace("-", "_")
    return any(marker in normalized for marker in _SENSITIVE_KEY_MARKERS)


def _blocked_shape(value: Any) -> Any:
    if isinstance(value, str):
        return _BLOCKED
    if isinstance(value, list):
        return [_BLOCKED]
    if isinstance(value, tuple):
        return (_BLOCKED,)
    if isinstance(value, dict):
        return {"blocked": True, "content": _BLOCKED}
    return _BLOCKED
