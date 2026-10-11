"""MCP tools. Importing this package registers every tool with the registry.

Phase 1 modules: session_tools (session and safety), lowlevel (screen and input), testmode (console, events,
logs, saves, scenarios), time_tools (time and popups). Phase 2: telemetry_tools (the companion mod) and
understanding (state, briefing, screen, map). Phase 3: politics_tools (focus, politics, decisions, diplomacy and
trade) and economy_tools (research, production, construction). Phase 4: military_tools (army, orders,
navy and air). Phase 5: campaign_loop_tools (advance, journal). Later phases add modules here and remove their
names from registry.PLANNED.
"""

from . import lowlevel, session_tools, testmode, time_tools  # noqa: F401  (registration side effects)
from . import telemetry_tools, understanding  # noqa: F401  (phase 2)
from . import economy_tools, politics_tools  # noqa: F401  (phase 3)
from . import military_tools, front_tools  # noqa: F401  (phase 4, 7)
from . import campaign_loop_tools  # noqa: F401  (phase 5)
from .registry import all_tools, get  # noqa: F401
