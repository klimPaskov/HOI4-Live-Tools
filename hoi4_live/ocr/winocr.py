"""Compatibility imports for callers of the original Windows OCR facade.

OcrHost now selects Windows.Media.Ocr or macOS Vision lazily.
"""

from .host import OcrHost
from .models import OcrLine, OcrResult

__all__ = ["OcrHost", "OcrLine", "OcrResult"]
