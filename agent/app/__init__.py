"""Codemax Agent service package."""

from importlib.metadata import PackageNotFoundError, version

try:
    __version__ = version("codemax-agent")
except PackageNotFoundError:
    __version__ = "0.0.0"
