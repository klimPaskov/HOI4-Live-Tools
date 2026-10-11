@echo off
rem HOI4 Live Tools MCP server (stdio). MCP clients run this file; extra arguments pass through
rem (list, call TOOL JSON, docs, selftest).
rem
rem Python is chosen in this order: HOI4_LIVE_PYTHON, the installer's virtual environment (..\venv next to
rem an installed app folder, or .venv in a checkout), a python on PATH (or the py launcher) that has numpy, Pillow and OpenCV.
rem An installed copy keeps its settings and output outside the app folder: config.json and artifacts\ in
rem the folder above (HOI4_LIVE_CONFIG and HOI4_LIVE_ARTIFACTS override).
setlocal
set "HERE=%~dp0"
set "PY="
set "PYARGS="
if defined HOI4_LIVE_PYTHON set "PY=%HOI4_LIVE_PYTHON%"
if not defined PY if exist "%HERE%..\venv\Scripts\python.exe" set "PY=%HERE%..\venv\Scripts\python.exe"
if not defined PY if exist "%HERE%.venv\Scripts\python.exe" set "PY=%HERE%.venv\Scripts\python.exe"
if not defined PY (
  python -c "import numpy, PIL, cv2" >nul 2>nul && set "PY=python"
)
if not defined PY (
  where py >nul 2>nul && py -3 -c "import numpy, PIL, cv2" >nul 2>nul && (set "PY=py" & set "PYARGS=-3")
)
if not defined PY set "PY=python"
if exist "%HERE%..\install.json" (
  if not defined HOI4_LIVE_CONFIG if exist "%HERE%..\config.json" set "HOI4_LIVE_CONFIG=%HERE%..\config.json"
  if not defined HOI4_LIVE_ARTIFACTS set "HOI4_LIVE_ARTIFACTS=%HERE%..\artifacts"
)
set "PYTHONPATH=%HERE%;%PYTHONPATH%"
set "PYTHONIOENCODING=utf-8"
set "PYTHONUNBUFFERED=1"
"%PY%" %PYARGS% -m hoi4_live %*
