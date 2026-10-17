"""Portable OCR with lazy Windows.Media.Ocr and macOS Vision backends."""

from .host import OcrHost
from .models import OcrLine, OcrResult

__all__ = ["OcrHost", "OcrLine", "OcrResult"]
