"""Portable OCR text and pixel-box results."""

import re


class OcrLine:
    __slots__ = ("text", "box", "words")

    def __init__(self, text, box, words):
        self.text = text
        self.box = box            # [x, y, w, h]
        self.words = words        # [{"text", "box"}]

    @property
    def center(self):
        x, y, w, h = self.box
        return x + w // 2, y + h // 2

    def to_dict(self, words=False):
        d = {"text": self.text, "box": self.box}
        if words:
            d["words"] = self.words
        return d


class OcrResult:
    def __init__(self, lines, region, scale):
        self.lines = lines
        self.region = region
        self.scale = scale

    @property
    def text(self):
        return "\n".join(l.text for l in self.lines)

    def find(self, pattern, flags=re.I):
        """Lines matching a regex (or a plain substring when pattern is not a valid regex)."""
        try:
            rx = re.compile(pattern, flags)
        except re.error:
            rx = re.compile(re.escape(pattern), flags)
        return [l for l in self.lines if rx.search(l.text)]

    def find_words(self, pattern, flags=re.I):
        rx = re.compile(pattern, flags)
        out = []
        for l in self.lines:
            for w in l.words:
                if rx.search(w["text"]):
                    out.append(w)
        return out

    def to_dict(self, words=False):
        return {"region": self.region, "scale": self.scale, "lines": [l.to_dict(words) for l in self.lines]}


