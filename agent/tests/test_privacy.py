from app.privacy import redact_model_context


def test_redact_model_context_covers_common_multiline_and_quoted_credentials() -> None:
    secrets = [
        "quoted password value",
        "password value",
        "bearer-secret-value",
        "url-user-secret",
        "url-password-secret",
        "query-token-secret",
        "private-key-secret",
        "openai-prefixed-secret",
        "github-prefixed-secret",
        "json-secret-value",
        "standalone-provider-token",
        "jwtpayloadsecret",
    ]
    source = (
        'password="quoted password value"\n'
        "Authorization: Bearer bearer-secret-value\n"
        "https://url-user-secret:url-password-secret@example.invalid/path"
        "?token=query-token-secret\n"
        "-----BEGIN PRIVATE KEY-----\n"
        "private-key-secret\n"
        "-----END PRIVATE KEY-----\n"
        "OPENAI_API_KEY=openai-prefixed-secret\n"
        "GITHUB_TOKEN=github-prefixed-secret\n"
        '{"api_key": "json-secret-value", "token_count": 42}\n'
        "ghp_standalone-provider-token\n"
        "eyJhbGciOiJIUzI1NiJ9.jwtpayloadsecret.signaturesecret"
    )

    redacted = redact_model_context(source)

    assert all(secret not in redacted for secret in secrets)
    assert redacted.count("[REDACTED]") >= 9
    assert '"token_count": 42' in redacted


def test_redacts_escaped_quoted_secrets_without_leaking_the_suffix() -> None:
    secret_suffix = "def-secret-suffix"
    payload = '{"password":"abc\\\"' + secret_suffix + '"}'

    redacted = redact_model_context(payload)

    assert secret_suffix not in redacted
    assert "[REDACTED]" in redacted


def test_redacts_hugging_face_and_google_provider_tokens() -> None:
    hugging_face = "hf_abcdefghijklmnopqrstuvwxyz123456"
    google_api_key = "AIzaSyabcdefghijklmnopqrstuvwxyz123456"

    redacted = redact_model_context(f"{hugging_face} {google_api_key}")

    assert hugging_face not in redacted
    assert google_api_key not in redacted
    assert redacted.count("[REDACTED]") == 2


def test_redacts_unprefixed_32_character_hex_tokens_and_whitespace_assignments() -> None:
    hex_token = "0123456789abcdef0123456789abcdef"
    password_value = "fictional-password-value"
    token_value = "fictional-token-value"

    redacted = redact_model_context(
        f"digest={hex_token}\npassword {password_value}\ntoken {token_value}"
    )

    assert hex_token not in redacted
    assert password_value not in redacted
    assert token_value not in redacted
    assert redacted.count("[REDACTED]") == 3
